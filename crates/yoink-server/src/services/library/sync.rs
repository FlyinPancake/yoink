use std::collections::{HashMap, HashSet};

use sea_orm::{
    ActiveModelBehavior, ActiveModelTrait, ActiveValue::Set, ColumnTrait, EntityLoaderTrait,
    EntityTrait, IntoActiveModel, ModelTrait, QueryFilter, TransactionTrait,
};
use uuid::Uuid;

use crate::{
    db::{self, album_type::AlbumType, provider::Provider, wanted_status::WantedStatus},
    error::{AppError, AppResult},
    providers::{ProviderAlbum, provider_image_url},
    state::AppState,
    util::provider_priority,
};

/// Sync albums and tracks for an artist from all linked metadata providers.
pub(crate) async fn sync_artist(state: &AppState, artist_id: Uuid) -> AppResult<()> {
    tracing::info!(%artist_id, "starting album sync");
    let artist = db::artist::Entity::load()
        .filter_by_id(artist_id)
        .with(db::artist_provider_link::Entity)
        .one(&state.db)
        .await?
        .ok_or_else(|| AppError::not_found("artist", Some(artist_id.to_string())))?;

    if artist.provider_links.is_empty() {
        return Err(AppError::not_found(
            "artist provider links",
            Some(artist_id.to_string()),
        ));
    }

    let mut all_incoming = vec![];

    for link in artist.clone().provider_links {
        let Some(provider) = state.registry.metadata_provider(link.provider) else {
            tracing::warn!(%artist_id, provider = ?link.provider, "no metadata provider registered, skipping");
            continue;
        };

        match provider.fetch_albums(&link.external_id).await {
            Ok(albums) => {
                tracing::debug!(%artist_id, provider = ?link.provider, count = albums.len(), "fetched albums");
                all_incoming.extend(albums.into_iter().map(|album| (link.provider, album)))
            }
            Err(e) => {
                tracing::error!(
                    "Failed to fetch albums for artist {} from provider {}: {}",
                    artist_id,
                    link.provider,
                    e
                );
            }
        }
    }

    if all_incoming.is_empty() {
        tracing::info!(%artist_id, "no albums returned from any provider, nothing to sync");
        return Ok(());
    }

    tracing::info!(%artist_id, total = all_incoming.len(), "collected albums from all providers");

    let mut groups: HashMap<String, Vec<(Provider, ProviderAlbum)>> = HashMap::new();

    for (provider, album) in all_incoming {
        let key = album_identity_key(&album.title, album.release_date.map(|d| d.to_string()));
        groups.entry(key).or_default().push((provider, album));
    }

    // Merge dateless groups into dated ones with the same normalized title.
    let dateless_keys: Vec<String> = groups
        .keys()
        .filter(|k| k.ends_with('|'))
        .cloned()
        .collect();
    for dateless_key in dateless_keys {
        let title_part = &dateless_key[..dateless_key.len() - 1];
        let dated_match = groups
            .keys()
            .find(|k| {
                *k != &dateless_key
                    && k.starts_with(title_part)
                    && k.as_bytes().get(title_part.len()) == Some(&b'|')
            })
            .cloned();
        if let Some(dated_key) = dated_match
            && let Some(entries) = groups.remove(&dateless_key)
        {
            groups.entry(dated_key).or_default().extend(entries);
        }
    }

    tracing::debug!(%artist_id, groups = groups.len(), "deduplication complete");

    let incoming_keys = groups.keys().cloned().collect::<HashSet<_>>();

    for entries in groups.values() {
        let (best_provider, best_album) = entries
            .iter()
            .skip(1)
            .fold(&entries[0], |acc, candidate| {
                if should_prefer_album(&acc.0, &acc.1, &candidate.0, &candidate.1) {
                    candidate
                } else {
                    acc
                }
            })
            .clone();

        let mut local_album_id = None;
        for (prov, album) in entries {
            let link = db::album_provider_link::Entity::find()
                .filter(
                    db::album_provider_link::Column::ProviderAlbumId.eq(album.external_id.clone()),
                )
                .filter(db::album_provider_link::Column::Provider.eq(*prov))
                .one(&state.db)
                .await?;

            if let Some(link) = link {
                local_album_id = Some(link.album_id);
                break;
            }
        }

        let album = match local_album_id {
            Some(album_id) => {
                tracing::debug!(%artist_id, %album_id, title = %best_album.title, provider = ?best_provider, "updating existing album");
                let Some(album) = db::album::Entity::find_by_id(album_id)
                    .one(&state.db)
                    .await?
                else {
                    continue;
                };
                let album = album.into_active_model().into_ex();
                Some(
                    album
                        .set_title(best_album.title.clone())
                        .set_album_type(
                            best_album
                                .album_type
                                .as_deref()
                                .map(AlbumType::parse)
                                .unwrap_or(AlbumType::Unknown),
                        )
                        .set_release_date(best_album.release_date)
                        .set_cover_url(
                            best_album
                                .cover_ref
                                .as_deref()
                                .map(|c| provider_image_url(best_provider, c, 640)),
                        )
                        .set_explicit(best_album.explicit)
                        .update(&state.db)
                        .await?,
                )
            }
            None => {
                tracing::debug!(%artist_id, title = %best_album.title, provider = ?best_provider, "inserting new album");

                let album_type = best_album
                    .album_type
                    .as_deref()
                    .map(AlbumType::parse)
                    .unwrap_or(AlbumType::Unknown);

                let cover_url = best_album
                    .cover_ref
                    .as_deref()
                    .map(|c| provider_image_url(best_provider, c, 640));

                let tx = state.db.begin().await?;

                let model = db::album::ActiveModel {
                    title: Set(best_album.title),
                    album_type: Set(album_type),
                    release_date: Set(best_album.release_date),
                    cover_url: Set(cover_url),
                    explicit: Set(best_album.explicit),
                    wanted_status: Set(WantedStatus::Unmonitored),
                    ..db::album::ActiveModel::new()
                };
                let new_album = model.insert(&tx).await?;
                let new_id = new_album.id;

                for (prov, album) in entries {
                    let link = db::album_provider_link::ActiveModel {
                        album_id: Set(new_id),
                        provider: Set(*prov),
                        provider_album_id: Set(album.external_id.clone()),
                        external_url: Set(album.url.clone()),
                        external_name: Set(Some(album.title.clone())),
                        ..db::album_provider_link::ActiveModel::new()
                    };
                    link.insert(&tx).await?;
                }

                let junction = db::album_artist::ActiveModel {
                    album_id: Set(new_id),
                    artist_id: Set(artist_id),
                    priority: Set(0),
                };
                junction.insert(&tx).await?;

                tx.commit().await?;

                tracing::info!(%artist_id, %new_id, title = %new_album.title, "created new album");
                Some(new_album.into_ex())
            }
        };

        if let Some(ref album) = album {
            sync_album_tracks(state, best_provider, &best_album.external_id, album.id).await?;
        }

        // TODO insert additional artists
    }

    // Remove stale albums

    if artist.monitored {
        let mut ids_to_remove = vec![];
        let albums = artist
            .find_related(db::album::Entity)
            .all(&state.db)
            .await?;

        for album in albums {
            let key = album_identity_key(&album.title, album.release_date.map(|d| d.to_string()));
            if !incoming_keys.contains(&key) {
                ids_to_remove.push(album.id);
            }
        }

        if !ids_to_remove.is_empty() {
            tracing::info!(%artist_id, count = ids_to_remove.len(), "removing stale albums");
            db::album::Entity::delete_many()
                .filter(db::album::Column::Id.is_in(ids_to_remove))
                .exec(&state.db)
                .await?;
        }
    }

    tracing::info!(%artist_id, "album sync complete");
    state.notify_sse();
    Ok(())
}

/// Sync tracks for an album from a metadata provider.
///
/// Fetches tracks, deduplicates against local state, and inserts or updates.
/// Does not delete local tracks missing from the provider (preserves user files).
/// New tracks are always inserted as `Unmonitored`.
pub(crate) async fn sync_album_tracks(
    state: &AppState,
    provider: Provider,
    external_album_id: &str,
    album_id: Uuid,
) -> AppResult<()> {
    let metadata = state.registry.metadata_provider(provider).ok_or_else(|| {
        AppError::unavailable(
            "metadata provider",
            format!("unknown provider '{provider}'"),
        )
    })?;

    let (provider_tracks, _album_extra) = metadata.fetch_tracks(external_album_id).await?;

    if provider_tracks.is_empty() {
        tracing::debug!(%album_id, %provider, "no tracks returned from provider");
        return Ok(());
    }

    // Pre-load existing tracks and their provider links for this album.
    let existing_tracks = db::track::Entity::load()
        .filter(db::track::Column::AlbumId.eq(album_id))
        .all(&state.db)
        .await?;

    let existing_links = db::track_provider_link::Entity::find()
        .filter(db::track_provider_link::Column::Provider.eq(provider))
        .filter(
            db::track_provider_link::Column::TrackId
                .is_in(existing_tracks.iter().map(|t| t.id).collect::<Vec<_>>()),
        )
        .all(&state.db)
        .await?;

    // Build lookup maps for dedup.
    let link_by_ext_id: HashMap<&str, &db::track_provider_link::Model> = existing_links
        .iter()
        .map(|l| (l.provider_track_id.as_str(), l))
        .collect();

    let track_by_id: HashMap<Uuid, &db::track::ModelEx> =
        existing_tracks.iter().map(|t| (t.id, t)).collect();

    let tx = state.db.begin().await?;
    let mut inserted = 0u32;
    let mut updated = 0u32;

    for pt in &provider_tracks {
        // Tier 1: match by provider + external_id
        let matched = link_by_ext_id
            .get(pt.external_id.as_str())
            .and_then(|link| track_by_id.get(&link.track_id).copied());

        // Tier 2: match by ISRC
        let matched = matched.or_else(|| {
            pt.isrc.as_deref().and_then(|isrc| {
                existing_tracks
                    .iter()
                    .find(|t| t.isrc.as_deref() == Some(isrc))
            })
        });

        // Tier 3: match by disc + track number
        let matched = matched.or_else(|| {
            existing_tracks.iter().find(|t| {
                t.disc_number == pt.disc_number && t.track_number == Some(pt.track_number)
            })
        });

        if let Some(existing) = matched {
            // Update metadata, leave status/file_path/root_folder_id untouched.
            existing
                .clone()
                .into_active_model()
                .set_title(pt.title.clone())
                .set_version(pt.version.clone())
                .set_disc_number(pt.disc_number)
                .set_track_number(Some(pt.track_number))
                .set_duration(Some(pt.duration_secs))
                .set_isrc(pt.isrc.clone())
                .set_explicit(pt.explicit)
                .update(&tx)
                .await?;

            // Ensure provider link exists for this provider.
            if !link_by_ext_id.contains_key(pt.external_id.as_str()) {
                let link = db::track_provider_link::ActiveModel {
                    track_id: Set(existing.id),
                    provider: Set(provider),
                    provider_track_id: Set(pt.external_id.clone()),
                    ..Default::default()
                };
                link.insert(&tx).await?;
            }

            updated += 1;
        } else {
            let model = db::track::ActiveModel {
                title: Set(pt.title.clone()),
                version: Set(pt.version.clone()),
                disc_number: Set(pt.disc_number),
                track_number: Set(Some(pt.track_number)),
                duration: Set(Some(pt.duration_secs)),
                album_id: Set(album_id),
                explicit: Set(pt.explicit),
                isrc: Set(pt.isrc.clone()),
                status: Set(WantedStatus::Unmonitored),
                ..Default::default()
            };
            let new_track = model.insert(&tx).await?;

            let link = db::track_provider_link::ActiveModel {
                track_id: Set(new_track.id),
                provider: Set(provider),
                provider_track_id: Set(pt.external_id.clone()),
                ..Default::default()
            };
            link.insert(&tx).await?;

            inserted += 1;
        }
    }

    tx.commit().await?;

    tracing::info!(%album_id, ?provider, inserted, updated, total = provider_tracks.len(), "track sync complete");
    Ok(())
}

fn album_identity_key(title: &str, release_date: Option<String>) -> String {
    let year = release_date
        .as_deref()
        .and_then(|d| d.split('-').next())
        .filter(|y| !y.is_empty());
    format!("{}|{}", normalize_title(title), year.unwrap_or(""))
}

/// Normalize a title for deduplication: lowercase, collapse Unicode punctuation
/// to ASCII equivalents, and strip featuring suffixes so that
/// "First Time (feat. Elipsa)" and "First Time" produce the same key.
fn normalize_title(title: &str) -> String {
    let normalized: String = title
        .trim()
        .chars()
        .map(|c| match c {
            '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}' => '\'',
            '\u{201C}' | '\u{201D}' | '\u{201E}' | '\u{201F}' => '"',
            '\u{2010}' | '\u{2011}' | '\u{2012}' | '\u{2013}' | '\u{2014}' | '\u{2015}' => '-',
            '\u{2026}' => '.',
            other => other,
        })
        .flat_map(|c| c.to_lowercase())
        .collect();

    strip_featuring(&normalized)
}

/// Strip parenthesized or bracketed featuring clauses from a lowercased title.
fn strip_featuring(title: &str) -> String {
    const FEAT_PREFIXES: &[&str] = &["feat. ", "feat ", "ft. ", "ft ", "featuring "];

    let mut result = title.to_string();
    for (open, close) in [('(', ')'), ('[', ']')] {
        if let Some(start) = result.find(open) {
            let inner = &result[start + open.len_utf8()..];
            if let Some(end_offset) = inner.find(close) {
                let inner_trimmed = inner[..end_offset].trim_start();
                if FEAT_PREFIXES.iter().any(|p| inner_trimmed.starts_with(p)) {
                    let end = start + open.len_utf8() + end_offset + close.len_utf8();
                    result = format!("{}{}", &result[..start], &result[end..]);
                    result = result.trim().to_string();
                }
            }
        }
    }
    result
}

/// Decide whether `candidate` should replace `existing` as the display-metadata
/// source for a merged album.
fn should_prefer_album(
    existing_provider: &Provider,
    existing: &ProviderAlbum,
    candidate_provider: &Provider,
    candidate: &ProviderAlbum,
) -> bool {
    let existing_cover = existing.cover_ref.is_some();
    let candidate_cover = candidate.cover_ref.is_some();
    if candidate_cover != existing_cover {
        return candidate_cover;
    }

    let existing_prio = provider_priority(*existing_provider);
    let candidate_prio = provider_priority(*candidate_provider);
    if candidate_prio != existing_prio {
        return candidate_prio > existing_prio;
    }

    let existing_explicit = existing.explicit;
    let candidate_explicit = candidate.explicit;
    if candidate_explicit != existing_explicit {
        return candidate_explicit;
    }

    candidate.external_id > existing.external_id
}
