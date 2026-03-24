use std::collections::{HashMap, HashSet};

use sea_orm::{
    ColumnTrait, EntityLoaderTrait, EntityTrait, IntoActiveModel, ModelTrait, QueryFilter,
};
use uuid::Uuid;

use crate::{
    db::{self, album_type::AlbumType, provider::Provider},
    error::{AppError, AppResult},
    providers::{ProviderAlbum, provider_image_url},
    state::AppState,
    util::provider_priority,
};

/// Sync albums for an artist from all linked metadata providers.
pub(crate) async fn sync_artist_albums(state: &AppState, artist_id: Uuid) -> AppResult<()> {
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
            continue;
        };

        match provider.fetch_albums(&link.external_id).await {
            Ok(albums) => {
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
        return Ok(());
    }

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

        let local_album_id = {
            loop {
                let Some((prov, album)) = entries.iter().next() else {
                    break None;
                };
                let link = db::album_provider_link::Entity::find()
                    .filter(
                        db::album_provider_link::Column::ProviderAlbumId
                            .eq(album.external_id.clone()),
                    )
                    .filter(db::album_provider_link::Column::Provider.eq(*prov))
                    .one(&state.db)
                    .await?;

                if let Some(link) = link {
                    break Some(link.album_id);
                }
            }
        };

        let _album = match local_album_id {
            Some(album_id) => {
                let Some(album) = db::album::Entity::find_by_id(album_id)
                    .one(&state.db)
                    .await?
                else {
                    continue;
                };
                let album = album.into_active_model().into_ex();
                Some(
                    album
                        .set_title(best_album.title)
                        .set_album_type(
                            best_album
                                .album_type
                                .map(|ty| serde_json::from_str::<AlbumType>(&ty).unwrap())
                                .unwrap_or(AlbumType::Album),
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
            None => None,
        };

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
            db::album::Entity::delete_many()
                .filter(db::album::Column::Id.is_in(ids_to_remove))
                .exec(&state.db)
                .await?;
        }
    }

    state.notify_sse();
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
