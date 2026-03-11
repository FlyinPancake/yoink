use crate::cls;
use leptos::prelude::*;
use lucide_leptos::{ArrowLeft, Bookmark, Check, Download, GitCompareArrows, RefreshCw, Trash2, X};

use yoink_shared::{
    DownloadJob, DownloadStatus, MatchSuggestion, MonitoredAlbum, MonitoredArtist, ProviderLink,
    Quality, ServerAction, TrackInfo, Uuid, album_cover_url, album_type_label, build_latest_jobs,
    provider_display_name, status_label_text,
};

use super::provider_icon_svg;
use crate::components::toast::{dispatch_with_toast, dispatch_with_toast_loading};
use crate::components::{
    Badge, BadgeSize, BadgeSurface, BadgeVariant, Breadcrumb, BreadcrumbItem, Button, ButtonSize,
    ButtonVariant, ConfirmDialog, ErrorPanel, PageShell, Panel, PanelBody, PanelHeader, PanelTitle,
    QualitySelect, ResolveArtistDialog, download_status_badge_variant, fallback_initial,
};
use crate::hooks::{set_page_title, use_sse_version};
use crate::styles::MUTED;

// ── DTO ─────────────────────────────────────────────────────

/// A resolved album artist credit for the UI.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ResolvedArtistCredit {
    pub name: String,
    /// If the artist is monitored locally, their UUID (clickable link).
    pub artist_id: Option<yoink_shared::Uuid>,
    /// Provider that sourced this credit.
    pub provider: Option<String>,
    /// External ID in that provider (for linking).
    pub external_id: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AlbumDetailData {
    pub album: Option<MonitoredAlbum>,
    /// Primary artist (backward compat).
    pub artist: Option<MonitoredArtist>,
    /// All album artists — resolved against local monitored artists.
    pub album_artists: Vec<ResolvedArtistCredit>,
    pub tracks: Vec<TrackInfo>,
    pub jobs: Vec<DownloadJob>,
    pub provider_links: Vec<ProviderLink>,
    pub match_suggestions: Vec<MatchSuggestion>,
    pub default_quality: Quality,
}

// ── Server function ─────────────────────────────────────────

#[server(GetAlbumDetail, "/leptos")]
pub async fn get_album_detail(album_id: String) -> Result<AlbumDetailData, ServerFnError> {
    use yoink_shared::Uuid;

    let ctx = crate::actions::require_ctx()?;

    let album_uuid: Uuid = album_id
        .parse()
        .map_err(|_| ServerFnError::new("invalid album UUID"))?;

    let (album, artist, album_artists) = {
        let albums = ctx.monitored_albums.read().await;
        let album = albums.iter().find(|a| a.id == album_uuid).cloned();
        let artist_id = album.as_ref().map(|a| a.artist_id);
        let all_artist_ids = album
            .as_ref()
            .map(|a| a.artist_ids.clone())
            .unwrap_or_default();
        let credits = album
            .as_ref()
            .map(|a| a.artist_credits.clone())
            .unwrap_or_default();
        drop(albums);

        let monitored_artists = ctx.monitored_artists.read().await;
        let artist =
            artist_id.and_then(|aid| monitored_artists.iter().find(|ar| ar.id == aid).cloned());

        // Build resolved album artist list.
        // If we have artist_credits from the provider, use those (richer info).
        // Otherwise fall back to the linked artist_ids.
        let album_artists: Vec<ResolvedArtistCredit> = if !credits.is_empty() {
            credits
                .iter()
                .map(|c| {
                    // Try to resolve to a local monitored artist via provider link
                    let local_id = all_artist_ids
                        .iter()
                        .find(|&aid| {
                            monitored_artists
                                .iter()
                                .any(|ar| ar.id == *aid && ar.name == c.name)
                        })
                        .copied();
                    ResolvedArtistCredit {
                        name: c.name.clone(),
                        artist_id: local_id,
                        provider: c.provider.clone(),
                        external_id: c.external_id.clone(),
                    }
                })
                .collect()
        } else {
            // No provider credits — build from linked artist_ids
            all_artist_ids
                .iter()
                .filter_map(|aid| {
                    monitored_artists.iter().find(|ar| ar.id == *aid).map(|ar| {
                        ResolvedArtistCredit {
                            name: ar.name.clone(),
                            artist_id: Some(ar.id),
                            provider: None,
                            external_id: None,
                        }
                    })
                })
                .collect()
        };

        (album, artist, album_artists)
    };

    // Fetch tracks, links, and suggestions concurrently (all are independent)
    let (tracks, jobs, provider_links, match_suggestions) = if album.is_some() {
        let tracks_fut = (ctx.fetch_tracks)(album_uuid);
        let links_fut = (ctx.fetch_album_links)(album_uuid);
        let suggestions_fut = (ctx.fetch_album_match_suggestions)(album_uuid);

        let (tracks_res, links_res, suggestions_res) =
            futures::future::join3(tracks_fut, links_fut, suggestions_fut).await;

        let jobs = ctx.download_jobs.read().await.clone();
        let tracks = tracks_res
            .map_err(|e| ServerFnError::new(format!("failed to load album tracks: {e}")))?;
        let provider_links = links_res
            .map_err(|e| ServerFnError::new(format!("failed to load album provider links: {e}")))?;
        let match_suggestions = suggestions_res.map_err(|e| {
            ServerFnError::new(format!("failed to load album match suggestions: {e}"))
        })?;

        (tracks, jobs, provider_links, match_suggestions)
    } else {
        let jobs = ctx.download_jobs.read().await.clone();
        (Vec::new(), jobs, Vec::new(), Vec::new())
    };

    Ok(AlbumDetailData {
        album,
        artist,
        album_artists,
        tracks,
        jobs,
        provider_links,
        match_suggestions,
        default_quality: ctx.default_quality,
    })
}

// ── Page component ──────────────────────────────────────────

#[component]
pub fn AlbumDetailPage() -> impl IntoView {
    set_page_title("Album");
    let params = leptos_router::hooks::use_params_map();
    let album_id = move || params.read().get("album_id");
    let artist_id = move || params.read().get("artist_id");

    let version = use_sse_version();
    let data: Resource<Result<AlbumDetailData, ServerFnError>> = Resource::new(
        move || (album_id(), version.get()),
        |(id, _)| async move {
            let id = id.ok_or_else(|| ServerFnError::new("missing route parameter: album_id"))?;
            get_album_detail(id).await
        },
    );

    view! {
        <PageShell active="library-artists">
                <Transition fallback=move || view! {
                    <div>
                        <Breadcrumb items=vec![
                            BreadcrumbItem::link("Library", "/library"),
                            BreadcrumbItem::link("\u{2026}", "#"),
                            BreadcrumbItem::current("\u{2026}"),
                        ] />
                        <div class="p-6 max-md:p-4">
                            // Skeleton hero card
                            <div class="mb-5 bg-white/70 dark:bg-zinc-800/60 rounded-xl border border-black/[.06] dark:border-white/[.08] p-5">
                                <div class="flex flex-col md:flex-row gap-6 animate-pulse">
                                    <div class="size-60 max-md:size-48 max-md:mx-auto rounded-xl bg-zinc-200 dark:bg-zinc-700 shrink-0"></div>
                                    <div class="flex-1 min-w-0">
                                        <div class="h-4 w-32 bg-zinc-200 dark:bg-zinc-700 rounded mb-3"></div>
                                        <div class="h-7 w-56 bg-zinc-200 dark:bg-zinc-700 rounded mb-3"></div>
                                        <div class="h-3.5 w-40 bg-zinc-200 dark:bg-zinc-700 rounded mb-4"></div>
                                        <div class="flex flex-wrap gap-1.5 mb-4">
                                            {(0..4).map(|_| view! {
                                                <div class="h-7 w-20 bg-zinc-200 dark:bg-zinc-700 rounded-lg"></div>
                                            }).collect_view()}
                                        </div>
                                        <div class="flex flex-wrap gap-1.5">
                                            {(0..3).map(|_| view! {
                                                <div class="h-8 w-24 bg-zinc-200 dark:bg-zinc-700 rounded-lg"></div>
                                            }).collect_view()}
                                        </div>
                                    </div>
                                </div>
                            </div>
                            // Skeleton tracklist
                            <div class="bg-white/70 dark:bg-zinc-800/60 rounded-xl border border-black/[.06] dark:border-white/[.08] overflow-hidden">
                                <div class="px-5 py-3 border-b border-black/[.06] dark:border-white/[.06]">
                                    <div class="h-4 w-24 bg-zinc-200 dark:bg-zinc-700 rounded animate-pulse"></div>
                                </div>
                                <div class="p-5">
                                    {(0..8).map(|_| view! {
                                        <div class="flex gap-3 mb-2.5 animate-pulse">
                                            <div class="h-3.5 w-6 bg-zinc-200 dark:bg-zinc-700 rounded"></div>
                                            <div class="h-3.5 flex-1 bg-zinc-200 dark:bg-zinc-700 rounded"></div>
                                            <div class="h-3.5 w-10 bg-zinc-200 dark:bg-zinc-700 rounded"></div>
                                        </div>
                                    }).collect_view()}
                                </div>
                            </div>
                        </div>
                    </div>
                }>
                    {move || {
                        let aid = artist_id();
                        data.get().map(|result| match result {
                            Err(e) => view! {
                                <div class="p-6 max-md:p-4">
                                    <ErrorPanel
                                        message="Failed to load album details."
                                        details=e.to_string()
                                    />
                                </div>
                            }.into_any(),
                            Ok(data) => match data.album {
                                None => {
                                    let back_href = aid
                                        .as_deref()
                                        .map(|id| format!("/artists/{id}"))
                                        .unwrap_or_else(|| "/library".to_string());
                                    view! {
                                        <div>
                                            <Breadcrumb items=vec![
                                                BreadcrumbItem::link("Library", "/library"),
                                                BreadcrumbItem::link("Artist", back_href),
                                                BreadcrumbItem::current("Not Found"),
                                            ] />
                                            <div class="p-6 max-md:p-4">
                                                <div class="text-zinc-500">"Album not found."</div>
                                                <Button href="/library" class="mt-4">
                                                    <ArrowLeft size=14 />
                                                    "Library"
                                                </Button>
                                            </div>
                                        </div>
                                    }.into_any()
                                }
                                Some(album) => {
                                    view! {
                                        <AlbumDetailContent
                                            album=album
                                            artist=data.artist
                                            album_artists=data.album_artists
                                            tracks=data.tracks
                                            jobs=data.jobs
                                            provider_links=data.provider_links
                                            match_suggestions=data.match_suggestions
                                            default_quality=data.default_quality
                                            artist_id_param=aid
                                        />
                                    }.into_any()
                                }
                            }
                        })
                    }}
                </Transition>
        </PageShell>
    }
}

// ── Detail content ──────────────────────────────────────────

#[component]
fn AlbumDetailContent(
    album: MonitoredAlbum,
    artist: Option<MonitoredArtist>,
    album_artists: Vec<ResolvedArtistCredit>,
    tracks: Vec<TrackInfo>,
    jobs: Vec<DownloadJob>,
    provider_links: Vec<ProviderLink>,
    match_suggestions: Vec<MatchSuggestion>,
    default_quality: Quality,
    artist_id_param: Option<String>,
) -> impl IntoView {
    set_page_title(&album.title);

    let album_id = album.id;
    let album_title = album.title.clone();
    let release_date = album
        .release_date
        .clone()
        .unwrap_or_else(|| "\u{2014}".to_string());
    let at = album_type_label(album.album_type.as_deref(), &album.title);
    let is_explicit = album.explicit;
    let is_monitored = album.monitored;
    let is_wanted = album.wanted;
    let is_acquired = album.acquired;
    let is_partially_wanted = album.partially_wanted;
    let effective_album_quality = album.quality_override.unwrap_or(default_quality);

    let cover_url = album_cover_url(&album, 640);

    let latest_jobs = build_latest_jobs(jobs);
    let latest_job = latest_jobs.get(&album.id).cloned();
    let job_status = latest_job.as_ref().map(|j| j.status.clone());
    let job_progress = latest_job
        .as_ref()
        .map(|j| (j.completed_tracks, j.total_tracks));
    let job_error = latest_job.as_ref().and_then(|j| j.error.clone());
    let job_quality = latest_job.as_ref().map(|j| j.quality);

    let status_pill_variant = job_status.as_ref().map(download_status_badge_variant);
    let status_pill_text = job_status.as_ref().map(|s| {
        status_label_text(
            s,
            job_progress.map(|(c, _)| c).unwrap_or(0),
            job_progress.map(|(_, t)| t).unwrap_or(0),
        )
    });

    let artist_name = artist
        .as_ref()
        .map(|a| a.name.clone())
        .unwrap_or_else(|| yoink_shared::UNKNOWN_ARTIST.to_string());
    let artist_link = artist_id_param
        .as_deref()
        .map(|id| format!("/artists/{id}"))
        .unwrap_or_else(|| "/library".to_string());

    let fallback_initial = fallback_initial(&album_title);

    let total_duration_secs: u32 = tracks.iter().map(|t| t.duration_secs).sum();
    let total_mins = total_duration_secs / 60;
    let total_secs = total_duration_secs % 60;
    let track_count = tracks.len();
    let duration_display = if total_mins >= 60 {
        let hrs = total_mins / 60;
        let mins = total_mins % 60;
        format!("{hrs} hr {mins} min")
    } else {
        format!("{total_mins} min {total_secs:02} sec")
    };
    let album_title_remove = album_title.clone();
    let duration_display_tracklist = duration_display.clone();
    let artist_name_tracklist = artist_name.clone();
    let album_artists_tracklist = album_artists.clone();

    // Confirmation dialog signals
    let show_remove_files = RwSignal::new(false);

    // Resolve artist dialog state
    let show_resolve_artist = RwSignal::new(false);
    let resolve_credit_name = RwSignal::new(String::new());
    let resolve_credit_provider = RwSignal::new(None::<String>);
    let resolve_credit_external_id = RwSignal::new(None::<String>);

    // Loading state signals
    let download_loading = RwSignal::new(false);

    // Can we show a download / retry button?
    let has_active_job = matches!(
        job_status,
        Some(DownloadStatus::Queued | DownloadStatus::Resolving | DownloadStatus::Downloading)
    );
    let can_download = is_wanted && !is_acquired && !has_active_job;
    let can_retry = matches!(job_status, Some(DownloadStatus::Failed));

    view! {
        // ── Sticky header with breadcrumb ───────────────────
        <Breadcrumb items=vec![
            BreadcrumbItem::link("Library", "/library"),
            BreadcrumbItem::link(artist_name.clone(), artist_link.clone()),
            BreadcrumbItem::current(album_title.clone()),
        ] />

        <div class="p-6 max-md:p-4">
            // ── Hero card ───────────────────────────────────
            <Panel class="mb-5">
                // ── Header bar: status pills (left) + action buttons (right) ──
                <PanelHeader>
                    <div class="flex flex-wrap items-center gap-2">
                        {if is_acquired {
                            view! { <Badge size=BadgeSize::Pill variant=BadgeVariant::Success>"Acquired"</Badge> }.into_any()
                        } else if is_wanted {
                            view! { <Badge size=BadgeSize::Pill variant=BadgeVariant::Warning>"Wanted"</Badge> }.into_any()
                        } else if is_partially_wanted {
                            view! {
                                <Badge
                                    size=BadgeSize::Pill
                                    variant=BadgeVariant::Warning
                                    surface=BadgeSurface::Outline
                                >
                                    "Partially Wanted"
                                </Badge>
                            }
                            .into_any()
                        } else {
                            view! { <span></span> }.into_any()
                        }}
                        {match (status_pill_variant, status_pill_text.clone()) {
                            (Some(variant), Some(text)) => view! {
                                <Badge size=BadgeSize::Pill variant=variant>{text}</Badge>
                            }
                            .into_any(),
                            _ => view! { <span></span> }.into_any(),
                        }}
                        {match (job_status.clone(), job_quality) {
                            (Some(_), Some(q)) => view! {
                                <Badge size=BadgeSize::Pill>{q.to_string()}</Badge>
                            }
                            .into_any(),
                            _ => view! { <span></span> }.into_any(),
                        }}
                    </div>
                    <div class="flex flex-wrap items-center gap-2">
                        {if can_retry {
                            view! {
                                <Button
                                    variant=ButtonVariant::Primary
                                    size=ButtonSize::Md
                                    loading=download_loading
                                    on:click={
                                        move |_| {
                                            dispatch_with_toast_loading(ServerAction::RetryDownload { album_id }, "Download requeued", Some(download_loading));
                                        }
                                    }>
                                    <RefreshCw size=14 />
                                    {move || if download_loading.get() { "Retrying\u{2026}" } else { "Retry" }}
                                </Button>
                            }.into_any()
                        } else if can_download {
                            view! {
                                <Button
                                    variant=ButtonVariant::Primary
                                    size=ButtonSize::Md
                                    loading=download_loading
                                    on:click={
                                        move |_| {
                                            dispatch_with_toast_loading(ServerAction::RetryDownload { album_id }, "Download started", Some(download_loading));
                                        }
                                    }>
                                    <Download size=14 />
                                    {move || if download_loading.get() { "Starting\u{2026}" } else { "Download" }}
                                </Button>
                            }.into_any()
                        } else {
                            view! { <span></span> }.into_any()
                        }}

                        {if is_monitored {
                            view! {
                                <Button
                                    size=ButtonSize::Md
                                    attr:title="Monitored \u{2014} click to unmonitor"
                                    on:click={
                                        move |_| {
                                            dispatch_with_toast(ServerAction::ToggleAlbumMonitor { album_id, monitored: false }, "Album unmonitored");
                                        }
                                    }>
                                    <Bookmark size=14 fill="currentColor" />
                                    "Unmonitor"
                                </Button>
                            }.into_any()
                        } else {
                            view! {
                                <Button
                                    size=ButtonSize::Md
                                    attr:title="Not monitored \u{2014} click to monitor"
                                    on:click={
                                        move |_| {
                                            dispatch_with_toast(ServerAction::ToggleAlbumMonitor { album_id, monitored: true }, "Album monitored");
                                        }
                                    }>
                                    <Bookmark size=14 />
                                    "Monitor"
                                </Button>
                            }.into_any()
                        }}

                        {if is_acquired {
                            view! {
                                <Button
                                    variant=ButtonVariant::Danger
                                    size=ButtonSize::Md
                                    attr:title="Delete downloaded files"
                                    on:click=move |_| {
                                        show_remove_files.set(true);
                                    }>
                                    <Trash2 size=14 />
                                    "Remove Files"
                                </Button>
                            }.into_any()
                        } else {
                            view! { <span></span> }.into_any()
                        }}
                    </div>
                </PanelHeader>

                // ── Error banner (between header and body) ──
                {match &job_error {
                    Some(err) => view! {
                        <div class="text-sm text-red-600 dark:text-red-400 bg-red-500/[.06] dark:bg-red-500/10 border-b border-red-500/20 px-5 max-md:px-3.5 py-2">
                            {err.clone()}
                        </div>
                    }.into_any(),
                    None => view! { <span></span> }.into_any(),
                }}

                // ── Body: cover art + identity info ──
                <PanelBody class="p-5 md:p-6">
                    <div class="flex gap-5">
                        // Cover art — compact square, no glow effects
                        <div class="shrink-0 w-28 h-28 md:w-40 md:h-40 rounded-lg overflow-hidden bg-zinc-200 dark:bg-zinc-800">
                            {match cover_url.clone() {
                                Some(url) => view! {
                                    <img class="w-full h-full object-cover" src=url alt="" />
                                }.into_any(),
                                None => view! {
                                    <div class="w-full h-full flex items-center justify-center text-3xl font-bold text-zinc-400 dark:text-zinc-600">{fallback_initial}</div>
                                }.into_any(),
                            }}
                        </div>

                        // Core identity: type, title, artist, date
                        <div class="flex-1 min-w-0 flex flex-col justify-center">
                            // Album type + explicit
                            <div class="flex items-center gap-2 mb-1">
                                <span class="text-[11px] font-semibold uppercase tracking-wider text-zinc-400 dark:text-zinc-500">{at}</span>
                                {if is_explicit {
                                    view! { <span class="text-[10px] px-1.5 py-0 rounded bg-zinc-200/80 dark:bg-zinc-700/80 text-zinc-500 dark:text-zinc-400 font-medium">"Explicit"</span> }.into_any()
                                } else {
                                    view! { <span></span> }.into_any()
                                }}
                            </div>

                            // Title — wraps on narrow screens
                            <h1 class="text-xl md:text-2xl font-bold text-zinc-900 dark:text-zinc-100 m-0 mb-1.5 leading-snug break-words">{album_title.clone()}</h1>

                            // Artist(s) · date · tracks
                            <div class={cls!(MUTED, "text-sm flex flex-wrap items-center gap-1.5")}>
                                {if album_artists.is_empty() {
                                    // Fallback: show primary artist only
                                    view! {
                                        <a href=artist_link.clone() class="text-zinc-600 dark:text-zinc-300 hover:text-blue-500 dark:hover:text-blue-400 no-underline font-medium">
                                            {artist_name.clone()}
                                        </a>
                                    }.into_any()
                                } else {
                                    view! {
                                        <span class="inline-flex flex-wrap items-center gap-0">
                                            {album_artists.iter().enumerate().map(|(i, credit)| {
                                                let sep = if i > 0 { ", " } else { "" };
                                                if let Some(aid) = credit.artist_id {
                                                    let href = format!("/artists/{aid}");
                                                    view! {
                                                        <><span>{sep}</span><a href=href class="text-zinc-600 dark:text-zinc-300 hover:text-blue-500 dark:hover:text-blue-400 no-underline font-medium">
                                                            {credit.name.clone()}
                                                        </a></>
                                                    }.into_any()
                                                } else {
                                                    let credit_name = credit.name.clone();
                                                    let credit_name_display = credit_name.clone();
                                                    let credit_provider = credit.provider.clone();
                                                    let credit_external_id = credit.external_id.clone();
                                                    view! {
                                                        <><span>{sep}</span><button
                                                            type="button"
                                                            class="text-zinc-400 dark:text-zinc-500 italic hover:text-amber-500 dark:hover:text-amber-400 bg-transparent border-none border-b border-dashed border-zinc-400/50 dark:border-zinc-500/50 cursor-pointer p-0 font-inherit text-sm transition-colors duration-150"
                                                            title="Not linked \u{2014} click to resolve"
                                                            on:click={
                                                                move |_| {
                                                                    resolve_credit_name.set(credit_name.clone());
                                                                    resolve_credit_provider.set(credit_provider.clone());
                                                                    resolve_credit_external_id.set(credit_external_id.clone());
                                                                    show_resolve_artist.set(true);
                                                                }
                                                            }
                                                        >
                                                            {credit_name_display}
                                                        </button></>
                                                    }.into_any()
                                                }
                                            }).collect_view()}
                                        </span>
                                    }.into_any()
                                }}
                                <span>"\u{00b7}"</span>
                                <span>{release_date.clone()}</span>
                                <span>"\u{00b7}"</span>
                                <span>{format!("{track_count} tracks, {duration_display}")}</span>
                            </div>

                            // Provider links (inline under meta on wider screens)
                            {if !provider_links.is_empty() {
                                view! {
                                    <div class="flex flex-wrap items-center gap-1.5 mt-2">
                                        <span class="text-[11px] text-zinc-400 dark:text-zinc-500">"Available on"</span>
                                        {provider_links.iter().map(|link| {
                                            let display = provider_display_name(&link.provider);
                                            let external_url = link.external_url.clone();
                                            let icon_svg = provider_icon_svg(&link.provider);
                                            match external_url {
                                                Some(url) => view! {
                                                    <Badge
                                                        variant=BadgeVariant::Info
                                                        surface=BadgeSurface::Outline
                                                        size=BadgeSize::Sm
                                                        href=url
                                                        new_tab=true
                                                    >
                                                        <span class="shrink-0 [&>svg]:size-3 text-blue-500/60 dark:text-blue-400/60" inner_html=icon_svg></span>
                                                        {display}
                                                    </Badge>
                                                }.into_any(),
                                                None => view! {
                                                    <Badge surface=BadgeSurface::Outline size=BadgeSize::Sm>
                                                        <span class="shrink-0 [&>svg]:size-3 text-zinc-400/60 dark:text-zinc-500/60" inner_html=icon_svg></span>
                                                        {display}
                                                    </Badge>
                                                }.into_any(),
                                            }
                                        }).collect_view()}
                                    </div>
                                }.into_any()
                            } else {
                                view! { <span></span> }.into_any()
                            }}

                            <div class="mt-3 flex flex-col gap-1 max-w-[240px]">
                                <label class="text-[11px] font-semibold uppercase tracking-wider text-zinc-400 dark:text-zinc-500">
                                    "Download Quality"
                                </label>
                                <QualitySelect
                                    selected=album.quality_override
                                    default_quality=default_quality
                                    default_label_prefix="Use default"
                                    on_change=Callback::new(move |quality: Option<Quality>| {
                                        dispatch_with_toast(
                                            ServerAction::SetAlbumQuality {
                                                album_id,
                                                quality,
                                            },
                                            "Album quality updated",
                                        );
                                    })
                                />
                                <span class={cls!(MUTED, "text-[11px]")}>
                                    "Applies to tracks without their own override."
                                </span>
                            </div>
                        </div>
                    </div>
                </PanelBody>
            </Panel>

            // ── Potential matches (own card, only if pending) ─
            {if match_suggestions.iter().any(|m| m.status == "pending") {
                view! {
                    <Panel class="mb-5">
                        <PanelHeader>
                            <PanelTitle class="inline-flex items-center gap-2">
                                <GitCompareArrows size=16 />
                                "Potential Matches"
                            </PanelTitle>
                        </PanelHeader>
                        <PanelBody class="px-5 py-3">
                            <div class="flex flex-col gap-2">
                                {match_suggestions.iter().filter(|m| m.status == "pending").map(|m| {
                                    let suggestion_id = m.id;
                                    let display_provider = provider_display_name(&m.right_provider);
                                    let kind = if m.match_kind == "isrc_exact" { "ISRC" } else { "Fuzzy" };
                                    let display_name = m
                                        .external_name
                                        .clone()
                                        .unwrap_or_else(|| "Unknown album match".to_string());
                                    let explanation = m.explanation.clone().unwrap_or_default();
                                    view! {
                                        <div class="flex items-start gap-3 text-xs text-zinc-700 dark:text-zinc-300">
                                            <div class="flex-1 min-w-0 flex flex-wrap items-center gap-2">
                                                <span
                                                    class="inline-flex items-center px-1.5 py-0.5 rounded-md bg-white/70 dark:bg-zinc-800/70 border border-black/[.06] dark:border-white/[.08]"
                                                    title=explanation
                                                >
                                                    {format!("{} {}%", kind, m.confidence)}
                                                </span>
                                                <span>{format!("{}: {}", display_provider, display_name)}</span>
                                            </div>
                                            <div class="shrink-0 flex items-center gap-1.5">
                                                <Button
                                                    variant=ButtonVariant::Primary
                                                    size=ButtonSize::Xs
                                                    on:click=move |_| {
                                                        dispatch_with_toast(
                                                            ServerAction::AcceptMatchSuggestion { suggestion_id },
                                                            "Match accepted",
                                                        );
                                                    }
                                                >
                                                    <Check size=12 />
                                                    "Accept"
                                                </Button>
                                                <Button
                                                    size=ButtonSize::Xs
                                                    on:click=move |_| {
                                                        dispatch_with_toast(
                                                            ServerAction::DismissMatchSuggestion { suggestion_id },
                                                            "Match dismissed",
                                                        );
                                                    }
                                                >
                                                    <X size=12 />
                                                    "Dismiss"
                                                </Button>
                                            </div>
                                        </div>
                                    }
                                }).collect_view()}
                            </div>
                        </PanelBody>
                    </Panel>
                }.into_any()
            } else {
                view! { <span></span> }.into_any()
            }}

            // ── Tracklist card ───────────────────────────────
            <Panel>
                <PanelHeader>
                    <div class="flex items-center gap-3">
                        <PanelTitle>"Tracklist"</PanelTitle>
                        <span class={cls!(MUTED, "text-xs")}>{format!("{track_count} tracks \u{00b7} {duration_display_tracklist}")}</span>
                    </div>
                </PanelHeader>
                {if tracks.is_empty() {
                    view! {
                        <div class="text-center py-10 px-4 text-zinc-400 dark:text-zinc-600 text-sm">"No tracks available."</div>
                    }.into_any()
                } else {
                    let has_any_artist = tracks.iter().any(|t| t.track_artist.is_some());
                    let has_any_path = tracks.iter().any(|t| t.file_path.is_some());
                    // Build a set of all album-level artist names to suppress in track rows
                    let album_artist_names: std::collections::HashSet<String> = {
                        let mut set: std::collections::HashSet<String> = album_artists_tracklist.iter().map(|c| c.name.clone()).collect();
                        set.insert(artist_name_tracklist.clone());
                        set
                    };
                    view! {
                        <PanelBody class="p-0!">
                            <div class="divide-y divide-black/[.04] dark:divide-white/[.04]">
                                {
                                    let has_multiple_discs = tracks.iter().any(|t| t.disc_number > 1);
                                    tracks.iter().map(|t| {
                                        let track_id = t.id;
                                        let num = t.track_number;
                                        let disc = t.disc_number;
                                        let title = t.title.clone();
                                        let version = t.version.clone();
                                        let dur = t.duration_display.clone();
                                        let explicit = t.explicit;
                                        let isrc = t.isrc.clone();
                                        let track_artist = t.track_artist.clone();
                                        let file_path = t.file_path.clone();
                                        let track_monitored = t.monitored;
                                        let track_acquired = t.acquired;
                                        let track_quality_override = t.quality_override;
                                        let track_num_display = if has_multiple_discs {
                                            format!("{disc}-{num}")
                                        } else {
                                            num.to_string()
                                        };

                                        // Show track artist only if it contains names not in the album artist list
                                        let show_track_artist = has_any_artist && track_artist.as_deref()
                                            .map(|ta| {
                                                // Split on common joinphrases and check if any name is new
                                                ta.split(';')
                                                    .flat_map(|s| s.split(','))
                                                    .flat_map(|s| s.split(" & "))
                                                    .any(|name| !album_artist_names.contains(name.trim()))
                                            })
                                            .unwrap_or(false);
                                        view! {
                                            <div class="flex items-center gap-3 px-5 py-2.5 transition-colors duration-100 hover:bg-blue-500/[.03] dark:hover:bg-blue-500/[.05]">
                                                // Track status indicator
                                                <TrackStatusIndicator
                                                    track_id=track_id
                                                    album_id=album_id
                                                    monitored=track_monitored
                                                    acquired=track_acquired
                                                />
                                                <span class="w-8 text-right text-xs tabular-nums text-zinc-400 dark:text-zinc-500 shrink-0">{track_num_display}</span>
                                                <div class="flex-1 min-w-0">
                                                    <div class="flex items-center gap-1.5 flex-wrap">
                                                        <span class="text-sm text-zinc-800 dark:text-zinc-200 truncate">{title}</span>
                                                        {match version {
                                                            Some(v) if !v.is_empty() => view! {
                                                                <span class="text-xs text-zinc-400 dark:text-zinc-500 shrink-0">{format!("({v})")}</span>
                                                            }.into_any(),
                                                            _ => view! { <span></span> }.into_any(),
                                                        }}
                                                        {if explicit {
                                                            view! {
                                                                <span class="inline-flex items-center justify-center px-1 py-px text-[9px] font-bold leading-none tracking-wide uppercase rounded bg-zinc-200 text-zinc-500 dark:bg-zinc-700 dark:text-zinc-400 shrink-0">"E"</span>
                                                            }.into_any()
                                                        } else {
                                                            view! { <span></span> }.into_any()
                                                        }}
                                                    </div>
                                                    // Track artist (if different from album artist)
                                                    {if show_track_artist {
                                                        view! {
                                                            <span class="block text-[11px] text-zinc-500 dark:text-zinc-400 leading-tight mt-0.5 truncate">
                                                                {track_artist.unwrap_or_default()}
                                                            </span>
                                                        }.into_any()
                                                    } else {
                                                        view! { <span></span> }.into_any()
                                                    }}
                                                    <div class="mt-1.5 flex flex-wrap items-center gap-2">
                                                        <span class={cls!(MUTED, "text-[10px] font-semibold uppercase tracking-wider")}>
                                                            "Quality"
                                                        </span>
                                                        <QualitySelect
                                                            selected=track_quality_override
                                                            default_quality=effective_album_quality
                                                            default_label_prefix="Album default"
                                                            on_change=Callback::new(move |quality: Option<Quality>| {
                                                                dispatch_with_toast(
                                                                    ServerAction::SetTrackQuality {
                                                                        album_id,
                                                                        track_id,
                                                                        quality,
                                                                    },
                                                                    "Track quality updated",
                                                                );
                                                            })
                                                        />
                                                    </div>
                                                    // ISRC + file path metadata line
                                                    {
                                                        let has_isrc = isrc.as_ref().map(|c| !c.is_empty()).unwrap_or(false);
                                                        let has_path = has_any_path && file_path.is_some();
                                                        if has_isrc || has_path {
                                                            view! {
                                                                <div class="flex items-center gap-2 mt-0.5 flex-wrap">
                                                                    {match isrc {
                                                                        Some(code) if !code.is_empty() => view! {
                                                                            <span class="text-[10px] font-mono text-zinc-400/70 dark:text-zinc-600 leading-tight">{code}</span>
                                                                        }.into_any(),
                                                                        _ => view! { <span></span> }.into_any(),
                                                                    }}
                                                                    {match file_path {
                                                                        Some(path) => {
                                                                            let path2 = path.clone();
                                                                            view! {
                                                                                <span class="text-[10px] font-mono text-zinc-400/50 dark:text-zinc-600/80 leading-tight truncate max-w-[400px]" title=path2>
                                                                                    {path}
                                                                                </span>
                                                                            }.into_any()
                                                                        }
                                                                        None => view! { <span></span> }.into_any(),
                                                                    }}
                                                                </div>
                                                            }.into_any()
                                                        } else {
                                                            view! { <span></span> }.into_any()
                                                        }
                                                    }
                                                </div>
                                                <span class="text-xs tabular-nums text-zinc-400 dark:text-zinc-500 shrink-0">{dur}</span>
                                            </div>
                                        }
                                    }).collect_view()
                                }
                            </div>
                        </PanelBody>
                    }.into_any()
                }}
            </Panel>
        </div>

        // ── Confirmation dialog for removing album files ────
        <ConfirmDialog
            open=show_remove_files
            title="Remove Files"
            message=format!("This will delete all downloaded files for \u{201c}{album_title_remove}\u{201d} from disk.")
            confirm_label="Remove Files"
            danger=true
            checkbox_label="Also unmonitor this album"
            on_confirm={
                move |unmonitor: bool| {
                    let msg = if unmonitor { "Album files removed and unmonitored" } else { "Album files removed" };
                    dispatch_with_toast(ServerAction::RemoveAlbumFiles { album_id, unmonitor }, msg);
                }
            }
        />

        // ── Resolve unlinked album artist dialog ────────────
        {move || {
            let name = resolve_credit_name.get();
            let prov = resolve_credit_provider.get().unwrap_or_default();
            let eid = resolve_credit_external_id.get().unwrap_or_default();
            view! {
                <ResolveArtistDialog
                    open=show_resolve_artist
                    album_id=album_id
                    credit_name=name
                    credit_provider=prov
                    credit_external_id=eid
                />
            }
        }}
    }
}

// ── Track status indicator ──────────────────────────────────

/// A small icon button that shows the track's monitored/acquired status
/// and toggles monitoring on click.
///
/// States:
/// - Acquired (downloaded): green check icon (non-interactive)
/// - Monitored (wanted): filled bookmark icon, amber — click to unmonitor
/// - Not monitored: outline bookmark icon, muted — click to monitor
#[component]
fn TrackStatusIndicator(
    track_id: Uuid,
    album_id: Uuid,
    monitored: bool,
    acquired: bool,
) -> impl IntoView {
    if acquired {
        // Acquired: green check, not clickable
        view! {
            <span
                class="shrink-0 flex items-center justify-center w-5 h-5 text-green-500 dark:text-green-400"
                title="Acquired"
            >
                <Check size=14 />
            </span>
        }
        .into_any()
    } else if monitored {
        // Monitored but not acquired: filled bookmark, amber — click to unmonitor
        view! {
            <button
                type="button"
                class="shrink-0 flex items-center justify-center w-5 h-5 text-amber-500 dark:text-amber-400 bg-transparent border-none cursor-pointer p-0 transition-colors duration-150 hover:text-amber-600 dark:hover:text-amber-300"
                title="Monitored for download \u{2014} click to unmonitor"
                on:click=move |e| {
                    e.stop_propagation();
                    dispatch_with_toast(
                        ServerAction::ToggleTrackMonitor { track_id, album_id, monitored: false },
                        "Track unmonitored",
                    );
                }
            >
                <Bookmark size=14 fill="currentColor" />
            </button>
        }
        .into_any()
    } else {
        // Not monitored: outline bookmark, muted — click to monitor
        view! {
            <button
                type="button"
                class="shrink-0 flex items-center justify-center w-5 h-5 text-zinc-300 dark:text-zinc-600 bg-transparent border-none cursor-pointer p-0 transition-colors duration-150 hover:text-amber-500 dark:hover:text-amber-400"
                title="Not monitored \u{2014} click to monitor"
                on:click=move |e| {
                    e.stop_propagation();
                    dispatch_with_toast(
                        ServerAction::ToggleTrackMonitor { track_id, album_id, monitored: true },
                        "Track monitored",
                    );
                }
            >
                <Bookmark size=14 />
            </button>
        }
        .into_any()
    }
}
