use crate::cls;
use leptos::prelude::*;
use lucide_leptos::{GitMerge, X};

use yoink_shared::{
    MonitoredAlbum, MonitoredArtist, ProviderLink, ServerAction, TrackInfo, provider_display_name,
};

use crate::components::toast::dispatch_with_toast;
use crate::components::{
    Badge, BadgeSize, BadgeSurface, BadgeVariant, Breadcrumb, BreadcrumbItem, Button, ButtonSize,
    ButtonVariant, ErrorPanel, PageShell, Panel, PanelBody, PanelHeader, PanelTitle,
};
use crate::hooks::{set_page_title, use_sse_version};
use crate::styles::{MUTED, SELECT};

// ── Data structures ─────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MergeCandidate {
    pub album_a: MonitoredAlbum,
    pub album_b: MonitoredAlbum,
    pub links_a: Vec<ProviderLink>,
    pub links_b: Vec<ProviderLink>,
    pub tracks_a: Vec<TrackInfo>,
    pub tracks_b: Vec<TrackInfo>,
    pub merged_tracks: Vec<TrackInfo>,
    pub merged_links: Vec<ProviderLink>,
    pub match_kind: String,
    pub confidence: u8,
    pub explanation: Option<String>,
    /// All suggestion IDs that contributed to this candidate (for dismiss).
    pub suggestion_ids: Vec<yoink_shared::Uuid>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MergeAlbumsData {
    pub artist: Option<MonitoredArtist>,
    pub candidates: Vec<MergeCandidate>,
}

// ── Server function ─────────────────────────────────────────

#[server(GetMergeAlbumsData, "/leptos")]
pub async fn get_merge_albums_data(artist_id: String) -> Result<MergeAlbumsData, ServerFnError> {
    use yoink_shared::Uuid;

    let ctx = crate::actions::require_ctx()?;

    let artist_uuid: Uuid = artist_id
        .parse()
        .map_err(|_| ServerFnError::new("invalid artist UUID"))?;

    let artists = ctx.monitored_artists.read().await;
    let artist = artists.iter().find(|a| a.id == artist_uuid).cloned();
    drop(artists);

    let albums: Vec<MonitoredAlbum> = ctx
        .monitored_albums
        .read()
        .await
        .iter()
        .filter(|a| a.artist_id == artist_uuid)
        .cloned()
        .collect();

    let mut album_by_id: std::collections::HashMap<Uuid, MonitoredAlbum> =
        std::collections::HashMap::new();
    let mut links_by_album: std::collections::HashMap<Uuid, Vec<ProviderLink>> =
        std::collections::HashMap::new();
    let mut pair_to_album: std::collections::HashMap<(String, String), Uuid> =
        std::collections::HashMap::new();
    let mut tracks_by_album: std::collections::HashMap<Uuid, Vec<TrackInfo>> =
        std::collections::HashMap::new();

    for album in &albums {
        album_by_id.insert(album.id, album.clone());
        let links = (ctx.fetch_album_links)(album.id).await.map_err(|e| {
            ServerFnError::new(format!(
                "failed to load provider links for album {}: {e}",
                album.id
            ))
        })?;
        for link in &links {
            pair_to_album.insert((link.provider.clone(), link.external_id.clone()), album.id);
        }
        links_by_album.insert(album.id, links);
        tracks_by_album.insert(
            album.id,
            (ctx.fetch_tracks)(album.id).await.map_err(|e| {
                ServerFnError::new(format!("failed to load tracks for album {}: {e}", album.id))
            })?,
        );
    }

    // Deduplicate by ordered album-pair (regardless of which suggestion triggered it).
    let mut dedupe: std::collections::HashSet<(Uuid, Uuid)> = std::collections::HashSet::new();
    // Collect suggestion IDs per album pair for dismiss support.
    let mut suggestion_ids_map: std::collections::HashMap<(Uuid, Uuid), Vec<Uuid>> =
        std::collections::HashMap::new();
    // Store first-seen metadata per pair.
    struct PairMeta {
        match_kind: String,
        confidence: u8,
        explanation: Option<String>,
    }
    let mut pair_meta: std::collections::HashMap<(Uuid, Uuid), PairMeta> =
        std::collections::HashMap::new();

    for album in &albums {
        let suggestions = (ctx.fetch_album_match_suggestions)(album.id)
            .await
            .map_err(|e| {
                ServerFnError::new(format!(
                    "failed to load match suggestions for album {}: {e}",
                    album.id
                ))
            })?;

        for s in suggestions.into_iter().filter(|s| s.status == "pending") {
            let pair = (s.right_provider.clone(), s.right_external_id.clone());
            let Some(&other_album_id) = pair_to_album.get(&pair) else {
                continue;
            };
            if other_album_id == album.id {
                continue;
            }

            let key = if album.id < other_album_id {
                (album.id, other_album_id)
            } else {
                (other_album_id, album.id)
            };

            suggestion_ids_map.entry(key).or_default().push(s.id);

            pair_meta.entry(key).or_insert(PairMeta {
                match_kind: s.match_kind.clone(),
                confidence: s.confidence,
                explanation: s.explanation.clone(),
            });

            dedupe.insert(key);
        }
    }

    let mut candidates = Vec::new();

    for key in &dedupe {
        let Some(a) = album_by_id.get(&key.0).cloned() else {
            continue;
        };
        let Some(b) = album_by_id.get(&key.1).cloned() else {
            continue;
        };
        let a_links = links_by_album.get(&key.0).cloned().unwrap_or_default();
        let b_links = links_by_album.get(&key.1).cloned().unwrap_or_default();
        let a_tracks = tracks_by_album.get(&key.0).cloned().unwrap_or_default();
        let b_tracks = tracks_by_album.get(&key.1).cloned().unwrap_or_default();
        let merged_tracks = build_merged_track_preview(&a_tracks, &b_tracks);
        let merged_links = build_merged_links_preview(&a_links, &b_links);
        let Some(meta) = pair_meta.remove(key) else {
            return Err(ServerFnError::new(
                "inconsistent merge candidate metadata for album pair",
            ));
        };
        let suggestion_ids = suggestion_ids_map.remove(key).unwrap_or_default();

        candidates.push(MergeCandidate {
            album_a: a,
            album_b: b,
            links_a: a_links,
            links_b: b_links,
            tracks_a: a_tracks,
            tracks_b: b_tracks,
            merged_tracks,
            merged_links,
            match_kind: meta.match_kind,
            confidence: meta.confidence,
            explanation: meta.explanation,
            suggestion_ids,
        });
    }

    candidates.sort_by(|a, b| {
        b.confidence
            .cmp(&a.confidence)
            .then_with(|| a.album_a.title.cmp(&b.album_a.title))
    });

    Ok(MergeAlbumsData { artist, candidates })
}

#[cfg(feature = "ssr")]
fn build_merged_track_preview(target: &[TrackInfo], source: &[TrackInfo]) -> Vec<TrackInfo> {
    let mut by_key: std::collections::HashMap<String, TrackInfo> = std::collections::HashMap::new();

    let make_key = |t: &TrackInfo| {
        if let Some(isrc) = &t.isrc
            && !isrc.trim().is_empty()
        {
            return format!("isrc:{}", isrc.trim().to_ascii_uppercase());
        }
        format!(
            "{}:{}:{}",
            t.disc_number,
            t.track_number,
            t.title.to_ascii_lowercase()
        )
    };

    for t in target {
        by_key.insert(make_key(t), t.clone());
    }
    for t in source {
        let key = make_key(t);
        by_key.entry(key).or_insert_with(|| t.clone());
    }

    let mut merged: Vec<TrackInfo> = by_key.into_values().collect();
    merged.sort_by(|a, b| {
        a.disc_number
            .cmp(&b.disc_number)
            .then_with(|| a.track_number.cmp(&b.track_number))
            .then_with(|| a.title.cmp(&b.title))
    });
    merged
}

#[cfg(feature = "ssr")]
fn build_merged_links_preview(
    target: &[ProviderLink],
    source: &[ProviderLink],
) -> Vec<ProviderLink> {
    let mut map: std::collections::HashMap<(String, String), ProviderLink> =
        std::collections::HashMap::new();

    for l in target {
        map.insert((l.provider.clone(), l.external_id.clone()), l.clone());
    }
    for l in source {
        map.entry((l.provider.clone(), l.external_id.clone()))
            .or_insert_with(|| l.clone());
    }

    let mut out: Vec<ProviderLink> = map.into_values().collect();
    out.sort_by(|a, b| {
        a.provider
            .cmp(&b.provider)
            .then_with(|| a.external_id.cmp(&b.external_id))
    });
    out
}

// ── UI helpers ──────────────────────────────────────────────

/// Confidence badge tone.
fn confidence_variant(confidence: u8) -> BadgeVariant {
    if confidence >= 80 {
        BadgeVariant::Success
    } else if confidence >= 50 {
        BadgeVariant::Warning
    } else {
        BadgeVariant::Danger
    }
}

fn match_kind_label(kind: &str) -> &str {
    match kind {
        "isrc_exact" => "ISRC match",
        "title_fuzzy" => "Fuzzy title",
        _ => kind,
    }
}

/// Render a compact provider badge.
#[component]
fn ProviderBadge(link: ProviderLink) -> impl IntoView {
    let label = provider_display_name(&link.provider);
    match link.external_url {
        Some(url) => view! {
            <Badge
                variant=BadgeVariant::Info
                surface=BadgeSurface::Outline
                href=url
                new_tab=true
                title=link.external_id
            >
                {label}
            </Badge>
        }
        .into_any(),
        None => view! {
            <Badge surface=BadgeSurface::Outline title=link.external_id>
                {label}
            </Badge>
        }
        .into_any(),
    }
}

/// Compact album card used in the side-by-side comparison.
#[component]
fn AlbumCard(album: MonitoredAlbum, links: Vec<ProviderLink>, track_count: usize) -> impl IntoView {
    view! {
        <div class="flex items-start gap-3 min-w-0">
            {match album.cover_url.clone() {
                Some(url) => view! {
                    <img src=url alt="" class="size-14 rounded-lg object-cover border border-black/[.06] dark:border-white/[.08] shrink-0 bg-zinc-200 dark:bg-zinc-800" />
                }.into_any(),
                None => view! {
                    <div class="size-14 rounded-lg inline-flex items-center justify-center border border-black/[.06] dark:border-white/[.08] bg-zinc-200 dark:bg-zinc-800 text-zinc-400 dark:text-zinc-500 text-[10px] shrink-0">"No Art"</div>
                }.into_any(),
            }}
            <div class="min-w-0 flex-1">
                <div class="text-sm font-semibold text-zinc-900 dark:text-zinc-100 truncate">{album.title}</div>
                {album.release_date.map(|d| view! {
                    <div class="text-xs text-zinc-500 dark:text-zinc-400">{d}</div>
                })}
                <div class="text-xs text-zinc-500 dark:text-zinc-400 mt-0.5">
                    {format!("{} tracks", track_count)}
                    {if album.acquired { " · acquired" } else { "" }}
                    {if album.monitored { " · monitored" } else { "" }}
                </div>
                <div class="flex flex-wrap gap-1 mt-1.5">
                    {links.into_iter().map(|link| view! { <ProviderBadge link=link /> }).collect_view()}
                </div>
            </div>
        </div>
    }
}

// ── Merge card component ────────────────────────────────────

#[component]
fn MergeCandidateCard(candidate: MergeCandidate) -> impl IntoView {
    let conf_variant = confidence_variant(candidate.confidence);
    let kind_label = match_kind_label(&candidate.match_kind).to_string();

    // Title options for the dropdown.
    let title_a = candidate.album_a.title.clone();
    let title_b = candidate.album_b.title.clone();
    let titles_differ = title_a != title_b;

    // Cover options for the dropdown.
    let cover_a = candidate.album_a.cover_url.clone();
    let cover_b = candidate.album_b.cover_url.clone();

    // Signals for user selection — default to album A's metadata.
    let (selected_title, set_selected_title) = signal(title_a.clone());
    let (selected_cover, set_selected_cover) = signal(cover_a.clone());

    // IDs for the merge action.
    let merge_target_id = candidate.album_a.id;
    let merge_source_id = candidate.album_b.id;
    let suggestion_ids = candidate.suggestion_ids.clone();

    // Summary stats.
    let merged_link_count = candidate.merged_links.len();
    let merged_track_count = candidate.merged_tracks.len();

    view! {
        <Panel class="overflow-hidden">
            // ── Header: confidence + match kind ────────────────
            <PanelHeader>
                <div class="flex items-center gap-2">
                    <Badge
                        variant=conf_variant
                        surface=BadgeSurface::Outline
                        size=BadgeSize::Sm
                    >
                        {format!("{}%", candidate.confidence)}
                    </Badge>
                    <span class="text-sm font-medium text-zinc-900 dark:text-zinc-100">
                        {kind_label}
                    </span>
                    {candidate.explanation.map(|e| view! {
                        <span class="text-xs text-zinc-500 dark:text-zinc-400">{format!("— {}", e)}</span>
                    })}
                </div>
            </PanelHeader>

            <PanelBody class="space-y-4">
                // ── Side-by-side album cards ───────────────────
                <div class="grid grid-cols-1 md:grid-cols-[1fr_auto_1fr] gap-3 items-stretch">
                    <div class="p-3 rounded-lg border border-black/[.04] dark:border-white/[.04] bg-zinc-50/50 dark:bg-zinc-900/30">
                        <AlbumCard
                            album=candidate.album_a
                            links=candidate.links_a
                            track_count=candidate.tracks_a.len()
                        />
                    </div>
                    <div class="hidden md:flex items-center justify-center text-zinc-400 dark:text-zinc-500 text-lg font-light select-none">
                        "+"
                    </div>
                    <div class="p-3 rounded-lg border border-black/[.04] dark:border-white/[.04] bg-zinc-50/50 dark:bg-zinc-900/30">
                        <AlbumCard
                            album=candidate.album_b
                            links=candidate.links_b
                            track_count=candidate.tracks_b.len()
                        />
                    </div>
                </div>

                // ── Merge result configuration ─────────────────
                <div class="p-3 rounded-lg border border-blue-500/15 bg-blue-500/[.04]">
                    <div class="flex flex-wrap items-center gap-x-4 gap-y-2">
                        // Title selector
                        {if titles_differ {
                            let ta = title_a.clone();
                            let tb = title_b.clone();
                            view! {
                                <label class="flex items-center gap-2 text-xs text-zinc-600 dark:text-zinc-300">
                                    <span class="font-medium">"Title:"</span>
                                    <select
                                        class=SELECT
                                        on:change=move |ev| {
                                            use leptos::prelude::event_target_value;
                                            set_selected_title.set(event_target_value(&ev));
                                        }
                                    >
                                        <option value={ta.clone()} selected=true>{ta.clone()}</option>
                                        <option value={tb.clone()}>{tb.clone()}</option>
                                    </select>
                                </label>
                            }.into_any()
                        } else {
                            view! {
                                <span class="text-xs text-zinc-600 dark:text-zinc-300">
                                    <span class="font-medium">"Title: "</span>
                                    {title_a.clone()}
                                </span>
                            }.into_any()
                        }}

                        // Cover selector (only when both have covers and they differ)
                        {if let Some(cover_a) = cover_a && let Some(cover_b) = cover_b && cover_a != cover_b {
                            view! {
                                <label class="flex items-center gap-2 text-xs text-zinc-600 dark:text-zinc-300">
                                    <span class="font-medium">"Cover:"</span>
                                    <select
                                        class=SELECT
                                        on:change= move |ev| {
                                            use leptos::prelude::event_target_value;
                                            let val = event_target_value(&ev);
                                            set_selected_cover.set(if val == "b" { Some(cover_b.clone()) } else { Some(cover_a.clone()) });
                                        }
                                    >
                                        <option value="a" selected=true>"Album 1 cover"</option>
                                        <option value="b">"Album 2 cover"</option>
                                    </select>
                                </label>
                            }.into_any()
                        } else {
                            view! { <span></span> }.into_any()
                        }}

                        // Summary
                        <span class="text-xs text-zinc-500 dark:text-zinc-400">
                            {format!("{} provider links · {} tracks after merge", merged_link_count, merged_track_count)}
                        </span>

                        // Merged provider badges
                        <div class="flex flex-wrap gap-1">
                            {candidate.merged_links.into_iter().map(|link| view! { <ProviderBadge link=link /> }).collect_view()}
                        </div>
                    </div>
                </div>

                // ── Collapsible track list ─────────────────────
                <details class="group">
                    <summary class="cursor-pointer text-xs font-medium text-zinc-500 dark:text-zinc-400 hover:text-zinc-700 dark:hover:text-zinc-200 select-none transition-colors">
                        {format!("Show merged track list ({} tracks)", merged_track_count)}
                    </summary>
                    <div class="mt-2 grid grid-cols-1 sm:grid-cols-2 gap-x-4 gap-y-0.5 text-xs text-zinc-600 dark:text-zinc-300 max-h-56 overflow-auto">
                        {candidate.merged_tracks.into_iter().map(|t| view! {
                            <div class="truncate">{
                            if let Some(ref v) = t.version {
                                if !v.is_empty() {
                                    format!("{}-{:02}. {} ({})", t.disc_number, t.track_number, t.title, v)
                                } else {
                                    format!("{}-{:02}. {}", t.disc_number, t.track_number, t.title)
                                }
                            } else {
                                format!("{}-{:02}. {}", t.disc_number, t.track_number, t.title)
                            }
                        }</div>
                        }).collect_view()}
                    </div>
                </details>

                // ── Actions ────────────────────────────────────
                <div class="flex items-center justify-end gap-2 pt-1 border-t border-black/[.04] dark:border-white/[.04]">
                    // Dismiss all related suggestions
                    {
                        let sids = suggestion_ids.clone();
                        view! {
                            <Button variant=ButtonVariant::Danger size=ButtonSize::Md
                                on:click=move |_| {
                                    for &sid in &sids {
                                        dispatch_with_toast(
                                            ServerAction::DismissMatchSuggestion { suggestion_id: sid },
                                            "Match dismissed",
                                        );
                                    }
                                }
                            >
                                <X size=12 />
                                "Not a match"
                            </Button>
                        }
                    }
                    // Merge
                    {
                        let target_id = merge_target_id;
                        let source_id = merge_source_id;
                        view! {
                            <Button variant=ButtonVariant::Primary size=ButtonSize::Md
                                on:click=move |_| {
                                    let title = selected_title.get();
                                    let cover = selected_cover.get();
                                    dispatch_with_toast(
                                        ServerAction::MergeAlbums {
                                            target_album_id: target_id,
                                            source_album_id: source_id,
                                            result_title: Some(title),
                                            result_cover_url: cover,
                                        },
                                        "Albums merged",
                                    );
                                }
                            >
                                <GitMerge size=12 />
                                "Merge"
                            </Button>
                        }
                    }
                </div>
            </PanelBody>
        </Panel>
    }
}

// ── Page component ──────────────────────────────────────────

#[component]
pub fn MergeAlbumsPage() -> impl IntoView {
    set_page_title("Merge Albums");

    let params = leptos_router::hooks::use_params_map();
    let artist_id = move || params.read().get("id");
    let version = use_sse_version();

    let data: Resource<Result<MergeAlbumsData, ServerFnError>> = Resource::new(
        move || (artist_id(), version.get()),
        |(id, _)| async move {
            let id = id.ok_or_else(|| ServerFnError::new("missing route parameter: id"))?;
            get_merge_albums_data(id).await
        },
    );

    view! {
        <PageShell active="library-artists">
                <Transition fallback=move || view! {
                    <div>
                        <Breadcrumb items=vec![
                            BreadcrumbItem::link("Library", "/library"),
                            BreadcrumbItem::link("\u{2026}", "#"),
                            BreadcrumbItem::current("Merge Albums"),
                        ] />
                        <div class="p-6 max-md:p-4">
                            // Skeleton intro card
                            <div class="mb-5 bg-white/70 dark:bg-zinc-800/60 rounded-xl border border-black/[.06] dark:border-white/[.08] overflow-hidden">
                                <div class="px-5 py-3.5 border-b border-black/[.06] dark:border-white/[.06]">
                                    <div class="h-4 w-32 bg-zinc-200 dark:bg-zinc-700 rounded animate-pulse"></div>
                                </div>
                                <div class="px-5 py-4">
                                    <div class="h-3.5 w-full max-w-md bg-zinc-200 dark:bg-zinc-700 rounded animate-pulse"></div>
                                </div>
                            </div>
                            // Skeleton candidate cards
                            {(0..2).map(|_| view! {
                                <div class="mb-4 bg-white/70 dark:bg-zinc-800/60 rounded-xl border border-black/[.06] dark:border-white/[.08] overflow-hidden animate-pulse">
                                    <div class="px-5 py-3.5 border-b border-black/[.06] dark:border-white/[.06] flex items-center gap-2">
                                        <div class="h-5 w-12 bg-zinc-200 dark:bg-zinc-700 rounded"></div>
                                        <div class="h-4 w-24 bg-zinc-200 dark:bg-zinc-700 rounded"></div>
                                    </div>
                                    <div class="px-5 py-4">
                                        <div class="grid grid-cols-1 md:grid-cols-[1fr_auto_1fr] gap-3">
                                            <div class="p-3 rounded-lg border border-black/[.04] dark:border-white/[.04]">
                                                <div class="flex items-start gap-3">
                                                    <div class="size-14 rounded-lg bg-zinc-200 dark:bg-zinc-700 shrink-0"></div>
                                                    <div class="flex-1">
                                                        <div class="h-4 w-28 bg-zinc-200 dark:bg-zinc-700 rounded mb-2"></div>
                                                        <div class="h-3 w-16 bg-zinc-200 dark:bg-zinc-700 rounded"></div>
                                                    </div>
                                                </div>
                                            </div>
                                            <div class="hidden md:flex items-center justify-center text-zinc-300 dark:text-zinc-600">"+"</div>
                                            <div class="p-3 rounded-lg border border-black/[.04] dark:border-white/[.04]">
                                                <div class="flex items-start gap-3">
                                                    <div class="size-14 rounded-lg bg-zinc-200 dark:bg-zinc-700 shrink-0"></div>
                                                    <div class="flex-1">
                                                        <div class="h-4 w-28 bg-zinc-200 dark:bg-zinc-700 rounded mb-2"></div>
                                                        <div class="h-3 w-16 bg-zinc-200 dark:bg-zinc-700 rounded"></div>
                                                    </div>
                                                </div>
                                            </div>
                                        </div>
                                    </div>
                                </div>
                            }).collect_view()}
                        </div>
                    </div>
                }>
                    {move || data.get().map(|result| match result {
                        Err(e) => view! {
                            <div>
                                <Breadcrumb items=vec![
                                    BreadcrumbItem::link("Library", "/library"),
                                    BreadcrumbItem::current("Merge Albums"),
                                ] />
                                <div class="p-6 max-md:p-4">
                                    <ErrorPanel message="Failed to load merge candidates." details=e.to_string() />
                                </div>
                            </div>
                        }.into_any(),
                        Ok(data) => {
                            let artist_name = data
                                .artist
                                .as_ref()
                                .map(|a| a.name.clone())
                                .unwrap_or_else(|| "Artist".to_string());
                            let artist_id_back = data
                                .artist
                                .as_ref()
                                .map(|a| a.id.to_string())
                                .or_else(&artist_id);
                            let artist_link = artist_id_back
                                .as_deref()
                                .map(|id| format!("/artists/{id}"))
                                .unwrap_or_else(|| "/library".to_string());
                            let count = data.candidates.len();

                            view! {
                                // Sticky header with breadcrumb
                                <Breadcrumb items=vec![
                                    BreadcrumbItem::link("Library", "/library"),
                                    BreadcrumbItem::link(artist_name.clone(), artist_link),
                                    BreadcrumbItem::current("Merge Albums"),
                                ] />

                                <div class="p-6 max-md:p-4">
                                    // Intro card
                                    <Panel class="mb-5">
                                        <PanelHeader>
                                            <PanelTitle>"Merge Albums"</PanelTitle>
                                            {if count > 0 {
                                                view! {
                                                    <span class={cls!(MUTED, "text-xs")}>
                                                        {format!("{} candidate{}", count, if count == 1 { "" } else { "s" })}
                                                    </span>
                                                }.into_any()
                                            } else {
                                                view! { <span></span> }.into_any()
                                            }}
                                        </PanelHeader>
                                        <PanelBody>
                                            <p class={cls!(MUTED, "text-sm m-0")}>
                                                "Albums that appear to be the same release across different providers. Review and merge them into a single entry, or dismiss false matches."
                                            </p>
                                        </PanelBody>
                                    </Panel>

                                    {if data.candidates.is_empty() {
                                        view! {
                                            <Panel class="p-8 text-center text-sm text-zinc-400 dark:text-zinc-500">
                                                "No duplicate albums detected \u{2014} nothing to merge."
                                            </Panel>
                                        }.into_any()
                                    } else {
                                        view! {
                                            <div class="flex flex-col gap-4">
                                                {data.candidates.into_iter().map(|candidate| {
                                                    view! { <MergeCandidateCard candidate=candidate /> }
                                                }).collect_view()}
                                            </div>
                                        }.into_any()
                                    }}
                                </div>
                            }.into_any()
                        }
                    })}
                </Transition>
        </PageShell>
    }
}
