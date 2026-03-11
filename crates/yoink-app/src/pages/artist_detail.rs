use crate::cls;
use leptos::prelude::*;
use lucide_leptos::{
    ArrowLeft, Bookmark, BookmarkCheck, BookmarkX, GitMerge, Pencil, Plus, RefreshCw, Trash2, X,
};

use yoink_shared::{
    ArtistImageOption, DownloadJob, DownloadStatus, MatchSuggestion, MonitoredAlbum,
    MonitoredArtist, ProviderLink, ServerAction, album_type_label, album_type_rank,
    build_latest_jobs, provider_display_name,
};

use leptoaster::{ToastBuilder, ToastLevel, ToastPosition, expect_toaster};

use super::provider_icon_svg;
use crate::actions::dispatch_action;
use crate::components::toast::{
    dispatch_with_toast, dispatch_with_toast_loading, toast_missing_context,
};
use crate::components::{
    AlbumCard, Badge, BadgeSize, BadgeSurface, BadgeVariant, Breadcrumb, BreadcrumbItem, Button,
    ButtonSize, ButtonVariant, ConfirmDialog, EditArtistDialog, ErrorPanel, LinkProviderDialog,
    MonitorToggle, PageShell, Panel, PanelBody, PanelHeader, PanelTitle, SleeveBadge,
    fallback_initial,
};
use crate::hooks::{set_page_title, use_sse_version};
use crate::styles::{EMPTY, GLASS, GLASS_BODY, MUTED, SELECT};

// ── DTO ─────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ArtistDetailData {
    pub artist: Option<MonitoredArtist>,
    pub albums: Vec<MonitoredAlbum>,
    pub jobs: Vec<DownloadJob>,
    pub provider_links: Vec<ProviderLink>,
    pub match_suggestions: Vec<MatchSuggestion>,
}

// ── Server function ─────────────────────────────────────────

#[server(GetArtistDetail, "/leptos")]
pub async fn get_artist_detail(artist_id: String) -> Result<ArtistDetailData, ServerFnError> {
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
        .filter(|a| a.artist_id == artist_uuid || a.artist_ids.contains(&artist_uuid))
        .cloned()
        .collect();

    let jobs = ctx.download_jobs.read().await.clone();

    let provider_links = if artist.is_some() {
        (ctx.fetch_artist_links)(artist_uuid)
            .await
            .map_err(|e| ServerFnError::new(format!("failed to load artist provider links: {e}")))?
    } else {
        Vec::new()
    };

    let match_suggestions = if artist.is_some() {
        (ctx.fetch_artist_match_suggestions)(artist_uuid)
            .await
            .map_err(|e| {
                ServerFnError::new(format!("failed to load artist match suggestions: {e}"))
            })?
    } else {
        Vec::new()
    };

    Ok(ArtistDetailData {
        artist,
        albums,
        jobs,
        provider_links,
        match_suggestions,
    })
}

/// Fetch available artist images from linked providers.
#[server(GetArtistImages, "/leptos")]
pub async fn get_artist_images(artist_id: String) -> Result<Vec<ArtistImageOption>, ServerFnError> {
    use yoink_shared::Uuid;

    let ctx = crate::actions::require_ctx()?;

    let artist_uuid: Uuid = artist_id
        .parse()
        .map_err(|_| ServerFnError::new("invalid artist UUID"))?;

    (ctx.fetch_artist_images)(artist_uuid)
        .await
        .map_err(ServerFnError::new)
}

// ── Page component ──────────────────────────────────────────

#[component]
pub fn ArtistDetailPage() -> impl IntoView {
    set_page_title("Artist");
    let params = leptos_router::hooks::use_params_map();
    let artist_id = move || params.read().get("id");

    let version = use_sse_version();
    let data: Resource<Result<ArtistDetailData, ServerFnError>> = Resource::new(
        move || (artist_id(), version.get()),
        |(id, _)| async move {
            let id = id.ok_or_else(|| ServerFnError::new("missing route parameter: id"))?;
            get_artist_detail(id).await
        },
    );

    // Stable signals — created once, updated by an Effect when the Resource
    // refetches. This lets ArtistDetailContent be mounted once and patched
    // in place via reactive reads instead of being recreated on every SSE update.
    let artist_sig: RwSignal<Option<MonitoredArtist>> = RwSignal::new(None);
    let albums_sig: RwSignal<Vec<MonitoredAlbum>> = RwSignal::new(Vec::new());
    let jobs_sig: RwSignal<Vec<DownloadJob>> = RwSignal::new(Vec::new());
    let links_sig: RwSignal<Vec<ProviderLink>> = RwSignal::new(Vec::new());
    let match_suggestions_sig: RwSignal<Vec<MatchSuggestion>> = RwSignal::new(Vec::new());
    let load_error: RwSignal<Option<String>> = RwSignal::new(None);
    let has_loaded = RwSignal::new(false);

    // Push data into signals whenever the resource produces a new value.
    // No Transition/Suspense needed — the Effect subscribes to the resource
    // and the UI reads from the stable signals, so the DOM is patched in place.
    Effect::new(move || {
        if let Some(result) = data.get() {
            match result {
                Err(e) => {
                    load_error.set(Some(e.to_string()));
                }
                Ok(d) => {
                    load_error.set(None);
                    artist_sig.set(d.artist);
                    albums_sig.set(d.albums);
                    jobs_sig.set(d.jobs);
                    links_sig.set(d.provider_links);
                    match_suggestions_sig.set(d.match_suggestions);
                }
            }
            has_loaded.set(true);
        }
    });

    view! {
        <PageShell active="library-artists">
                // Skeleton — shown only until first data arrives
                <Show when=move || !has_loaded.get()>
                    <div>
                        <Breadcrumb items=vec![
                            BreadcrumbItem::link("Library", "/library"),
                            BreadcrumbItem::current("\u{2026}"),
                        ] />
                        <div class="p-6 max-md:p-4">
                            <div class="mb-5 bg-white/70 dark:bg-zinc-800/60 rounded-xl border border-black/[.06] dark:border-white/[.08] p-5">
                                <div class="flex flex-wrap items-center gap-5 animate-pulse">
                                    <div class="size-20 rounded-full bg-zinc-200 dark:bg-zinc-700 shrink-0"></div>
                                    <div class="flex-1 min-w-0">
                                        <div class="h-6 w-40 bg-zinc-200 dark:bg-zinc-700 rounded mb-3"></div>
                                        <div class="h-3.5 w-64 bg-zinc-200 dark:bg-zinc-700 rounded mb-3"></div>
                                        <div class="flex flex-wrap gap-1.5">
                                            {(0..4).map(|_| view! {
                                                <div class="h-7 w-20 bg-zinc-200 dark:bg-zinc-700 rounded-lg"></div>
                                            }).collect_view()}
                                        </div>
                                    </div>
                                </div>
                            </div>
                            <div class="bg-white/70 dark:bg-zinc-800/60 rounded-xl border border-black/[.06] dark:border-white/[.08] overflow-hidden">
                                <div class="px-5 py-3 border-b border-black/[.06] dark:border-white/[.06]">
                                    <div class="h-4 w-24 bg-zinc-200 dark:bg-zinc-700 rounded animate-pulse"></div>
                                </div>
                                <div class="p-4">
                                    <div class="grid grid-cols-[repeat(auto-fill,minmax(180px,1fr))] gap-5 max-md:grid-cols-[repeat(auto-fill,minmax(140px,1fr))] max-md:gap-3">
                                        {(0..6).map(|_| view! {
                                            <div class="rounded-xl overflow-hidden border border-black/[.04] dark:border-white/[.04] animate-pulse">
                                                <div class="w-full" style="padding-top:100%;background:var(--tw-color-zinc-200,.oklch(.923 0 0))">
                                                </div>
                                                <div class="p-3">
                                                    <div class="h-3.5 w-24 bg-zinc-200 dark:bg-zinc-700 rounded mb-2"></div>
                                                    <div class="h-3 w-16 bg-zinc-200 dark:bg-zinc-700 rounded"></div>
                                                </div>
                                            </div>
                                        }).collect_view()}
                                    </div>
                                </div>
                            </div>
                        </div>
                    </div>
                </Show>
                // Error state
                <Show when=move || has_loaded.get() && load_error.get().is_some()>
                    {move || {
                        let e = load_error.get().unwrap_or_default();
                        view! {
                            <div class="p-6 max-md:p-4">
                                <ErrorPanel
                                    message="Failed to load artist details."
                                    details=e
                                />
                            </div>
                        }
                    }}
                </Show>
                // Artist not found
                <Show when=move || has_loaded.get() && artist_sig.get().is_none() && load_error.get().is_none()>
                    <div>
                        <Breadcrumb items=vec![
                            BreadcrumbItem::link("Library", "/library"),
                            BreadcrumbItem::current("Not Found"),
                        ] />
                        <div class="p-6 max-md:p-4">
                            <div class="text-zinc-500">"Artist not found."</div>
                            <Button size=ButtonSize::Lg class="mt-4" href="/library">
                                <ArrowLeft size=14 />
                                "Library"
                            </Button>
                        </div>
                    </div>
                </Show>
                // Main content — mounted once, patched in place via signals
                <Show when=move || artist_sig.get().is_some()>
                    <ArtistDetailContent
                        artist=artist_sig
                        albums=albums_sig
                        jobs=jobs_sig
                        provider_links=links_sig
                        match_suggestions=match_suggestions_sig
                    />
                </Show>
        </PageShell>
    }
}

#[component]
fn ArtistDetailContent(
    artist: RwSignal<Option<MonitoredArtist>>,
    albums: RwSignal<Vec<MonitoredAlbum>>,
    jobs: RwSignal<Vec<DownloadJob>>,
    provider_links: RwSignal<Vec<ProviderLink>>,
    match_suggestions: RwSignal<Vec<MatchSuggestion>>,
) -> impl IntoView {
    // Helper: unwrap the Option — safe because this component is only
    // rendered inside <Show when=move || artist_sig.get().is_some()>.
    let a = move || {
        artist
            .get()
            .expect("ArtistDetailContent rendered without artist")
    };

    // Confirmation dialog signals
    let show_unmonitor_all = RwSignal::new(false);
    let show_remove_artist = RwSignal::new(false);
    let show_link_provider = RwSignal::new(false);
    let show_edit_artist = RwSignal::new(false);

    // Loading state signals for async buttons
    let sync_loading = RwSignal::new(false);
    let promote_loading = RwSignal::new(false);
    let monitor_all_loading = RwSignal::new(false);
    let removing_artist = RwSignal::new(false);

    let (album_sort, set_album_sort) = signal("type".to_string());

    view! {
        // Header with breadcrumb — reads artist signal reactively
        {move || {
            let a = a();
            set_page_title(&a.name);
            view! {
                <Breadcrumb items=vec![
                    BreadcrumbItem::link("Library", "/library"),
                    BreadcrumbItem::current(a.name),
                ] />
            }
        }}

        <div class="p-6 max-md:p-4">
            // Artist header card — reactive
            {move || {
                let a = a();
                let all_albums = albums.get();
                let album_count = all_albums.len();
                let monitored_count = all_albums.iter().filter(|a| a.monitored).count();
                let acquired_count = all_albums.iter().filter(|a| a.acquired).count();
                let wanted_count = all_albums.iter().filter(|a| a.wanted).count();

                let fallback_initial = fallback_initial(&a.name);

                let artist_id = a.id;
                let artist_monitored = a.monitored;

                view! {
                    <div class={cls!(GLASS, "mb-5")}>
                        <div class={cls!(GLASS_BODY, "flex flex-wrap items-center gap-5")}>
                            {match a.image_url.clone() {
                                Some(url) => view! {
                                    <img class="size-20 rounded-full object-cover border-2 border-blue-500/20 dark:border-blue-500/30 shrink-0 bg-zinc-200 dark:bg-zinc-800" src=url alt="" />
                                }.into_any(),
                                None => view! {
                                    <div class="size-20 rounded-full inline-flex items-center justify-center bg-zinc-200 dark:bg-zinc-800 text-zinc-500 dark:text-zinc-400 font-bold text-[32px] border-2 border-blue-500/20 dark:border-blue-500/30 shrink-0">{fallback_initial}</div>
                                }.into_any(),
                            }}
                            <div class="flex-1 min-w-0">
                                <div class="text-[22px] font-bold mb-1">{a.name.clone()}</div>
                                <div class={cls!(MUTED, "text-[13px] mb-2 flex flex-wrap items-center gap-2")}>
                                    <span>{format!("{album_count} albums \u{00b7} {monitored_count} monitored \u{00b7} {acquired_count} acquired \u{00b7} {wanted_count} wanted")}</span>
                                    {if artist_monitored {
                                        view! {
                                            <Badge
                                                variant=BadgeVariant::Info
                                                surface=BadgeSurface::Outline
                                                size=BadgeSize::Sm
                                            >
                                                "Monitored"
                                            </Badge>
                                        }.into_any()
                                    } else {
                                        view! {
                                            <Badge
                                                variant=BadgeVariant::Warning
                                                surface=BadgeSurface::Outline
                                                size=BadgeSize::Sm
                                            >
                                                "Lightweight"
                                            </Badge>
                                        }.into_any()
                                    }}
                                </div>

                                {if !artist_monitored {
                                    view! {
                                        <div class="text-[12px] text-amber-700 dark:text-amber-300 mb-2">
                                            "This artist is lightweight. Promote to monitored to sync full discography automatically."
                                        </div>
                                    }.into_any()
                                } else {
                                    view! { <span></span> }.into_any()
                                }}

                                // Linked providers — inline chips with icons
                                {move || {
                                    let links = provider_links.get();

                                    view! {
                                        <div class="flex flex-wrap items-center gap-1.5 mb-2.5">
                                            {links.iter().map(|link| {
                                                let provider = link.provider.clone();
                                                let display = provider_display_name(&link.provider);
                                                let external_url = link.external_url.clone();
                                                let external_id = link.external_id.clone();
                                                let icon_svg = provider_icon_svg(&link.provider);

                                                view! {
                                                    <div class="group inline-flex items-center gap-1.5 pl-2 pr-1 py-1 bg-white/50 dark:bg-zinc-800/50 border border-black/[.06] dark:border-white/[.08] rounded-lg text-xs transition-colors hover:border-black/10 dark:hover:border-white/12">
                                                        <span class="shrink-0 [&>svg]:size-3.5 text-zinc-500 dark:text-zinc-400" inner_html=icon_svg></span>
                                                        {match external_url {
                                                            Some(url) => view! {
                                                                <a href=url class="font-medium text-zinc-700 dark:text-zinc-300 hover:text-blue-500 dark:hover:text-blue-400 no-underline" target="_blank" rel="noreferrer">
                                                                    {display}
                                                                </a>
                                                            }.into_any(),
                                                            None => view! {
                                                                <span class="font-medium text-zinc-700 dark:text-zinc-300">{display}</span>
                                                            }.into_any(),
                                                        }}
                                                        <button type="button"
                                                            class="text-[10px] text-zinc-400 dark:text-zinc-500 hover:text-red-500 dark:hover:text-red-400 bg-transparent border-none cursor-pointer p-0.5 opacity-0 group-hover:opacity-100 transition-opacity"
                                                            title="Unlink this provider"
                                                            on:click={

                                                                let prov = provider.clone();
                                                                let eid = external_id.clone();
                                                                move |_| {
                                                                    dispatch_with_toast(
                                                                        ServerAction::UnlinkArtistProvider {
                                                                            artist_id,
                                                                            provider: prov.clone(),
                                                                            external_id: eid.clone(),
                                                                        },
                                                                        "Provider unlinked",
                                                                    );
                                                                }
                                                            }>
                                                            <X size=12 />
                                                        </button>
                                                    </div>
                                                }
                                            }).collect_view()}
                                            <button type="button"
                                                class="inline-flex items-center gap-1 px-2 py-1 text-[11px] font-medium text-zinc-400 dark:text-zinc-500 hover:text-blue-500 dark:hover:text-blue-400 bg-transparent border border-dashed border-zinc-300 dark:border-zinc-600 hover:border-blue-500/30 dark:hover:border-blue-500/30 rounded-lg cursor-pointer transition-colors"
                                                on:click=move |_| {
                                                    show_link_provider.set(true);
                                                }>
                                                <Plus size=12 />
                                                "Link"
                                            </button>
                                        </div>
                                    }
                                }}

                                <div class="flex flex-wrap gap-1.5">
                                    <Button on:click=move |_| {
                                            show_edit_artist.set(true);
                                        }>
                                        <Pencil size=14 />
                                        "Edit"
                                    </Button>
                                    {if artist_monitored {
                                        view! {
                                            <Button loading=sync_loading
                                                on:click={
                                                    move |_| {
                                                        dispatch_with_toast_loading(ServerAction::SyncArtistAlbums { artist_id }, "Album sync started", Some(sync_loading));
                                                    }
                                                }>
                                                <span class=move || if sync_loading.get() { "animate-spin flex" } else { "flex" }><RefreshCw size=14 /></span>
                                                {move || if sync_loading.get() { "Syncing\u{2026}" } else { "Sync Albums" }}
                                            </Button>
                                        }.into_any()
                                    } else {
                                        view! {
                                            <Button variant=ButtonVariant::Primary loading=promote_loading
                                                on:click={
                                                    move |_| {
                                                        dispatch_with_toast_loading(
                                                            ServerAction::ToggleArtistMonitor { artist_id, monitored: true },
                                                            "Artist promoted to monitored",
                                                            Some(promote_loading),
                                                        );
                                                    }
                                                }>
                                                <Bookmark size=14 />
                                                {move || if promote_loading.get() { "Promoting\u{2026}" } else { "Monitor Artist" }}
                                            </Button>
                                        }.into_any()
                                    }}
                                    <Button variant=ButtonVariant::Danger class="ml-auto"
                                        on:click=move |_| {
                                            show_remove_artist.set(true);
                                        }>
                                        <Trash2 size=14 />
                                        "Remove Artist"
                                    </Button>
                                </div>
                            </div>
                        </div>

                        // Bio section
                        {match a.bio.clone() {
                            Some(bio) => view! {
                                <div class="px-5 py-4 border-t border-black/[.06] dark:border-white/[.06]">
                                    <ArtistBio bio=bio />
                                </div>
                            }.into_any(),
                            None => view! { <span></span> }.into_any(),
                        }}
                    </div>
                }
            }}

            // Match suggestions
            {move || {
                let pending = match_suggestions
                    .get()
                    .into_iter()
                    .filter(|m| m.status == "pending")
                    .collect::<Vec<_>>();
                let pending_count = pending.len();
                let a = a();

                if pending.is_empty() {
                    view! { <span></span> }.into_any()
                } else {
                    view! {
                        <Panel class="mb-5">
                            <PanelHeader>
                                <PanelTitle>{format!("Potential Matches ({pending_count})")}</PanelTitle>
                                <Button
                                    on:click={

                                        move |_| {
                                            dispatch_with_toast(
                                                ServerAction::RefreshMatchSuggestions { artist_id: a.id },
                                                "Match suggestions refreshed",
                                            );
                                        }
                                    }
                                >
                                    "Refresh"
                                </Button>
                            </PanelHeader>
                            <PanelBody>
                                <div class="flex flex-col gap-2">
                                    {pending.into_iter().map(|m| {
                                        let suggestion_id = m.id;
                                        let right = provider_display_name(&m.right_provider);
                                        let kind = if m.match_kind == "isrc_exact" { "ISRC" } else { "Fuzzy" };
                                        let name = m
                                            .external_name
                                            .clone()
                                            .unwrap_or_else(|| "Unknown artist match".to_string());
                                        let image_url = m.image_url.clone();
                                        let fallback_initial = fallback_initial(&name);
                                        let type_country: Option<String> = match (&m.artist_type, &m.country) {
                                            (Some(t), Some(c)) => Some(format!("{t} from {c}")),
                                            (Some(t), None) => Some(t.clone()),
                                            (None, Some(c)) => Some(format!("from {c}")),
                                            (None, None) => None,
                                        };
                                        let subtitle: Option<String> = {
                                            let base = m.disambiguation.clone().or(type_country);
                                            match (base, m.popularity) {
                                                (Some(b), Some(p)) => Some(format!("{b} · {p}% popularity")),
                                                (Some(b), None) => Some(b),
                                                (None, Some(p)) => Some(format!("{p}% popularity")),
                                                (None, None) => None,
                                            }
                                        };
                                        let confidence_variant = if m.confidence >= 80 {
                                            BadgeVariant::Success
                                        } else if m.confidence >= 50 {
                                            BadgeVariant::Warning
                                        } else {
                                            BadgeVariant::Danger
                                        };
                                        view! {
                                            <div class="flex items-start gap-3 p-2 rounded-lg border border-black/[.06] dark:border-white/[.08] bg-white/60 dark:bg-zinc-800/60">
                                                {match image_url {
                                                    Some(url) => view! {
                                                        <img class="size-9 rounded-full object-cover border border-blue-500/20 dark:border-blue-500/30 shrink-0 bg-zinc-200 dark:bg-zinc-800" src=url alt="" />
                                                    }.into_any(),
                                                    None => view! {
                                                        <div class="size-9 rounded-full inline-flex items-center justify-center bg-zinc-200 dark:bg-zinc-800 text-zinc-500 dark:text-zinc-400 font-bold text-sm border border-blue-500/20 dark:border-blue-500/30 shrink-0">{fallback_initial}</div>
                                                    }.into_any(),
                                                }}

                                                <div class="flex-1 min-w-0">
                                                    <div class="flex items-center gap-2 flex-wrap">
                                                        <span class="text-[15px] font-semibold text-zinc-900 dark:text-zinc-100">{name}</span>
                                                        {if let Some(url) = m.external_url.clone() {
                                                            view! {
                                                                <Badge
                                                                    variant=BadgeVariant::Info
                                                                    surface=BadgeSurface::Outline
                                                                    size=BadgeSize::Sm
                                                                    href=url
                                                                    new_tab=true
                                                                >
                                                                    {right.clone()}
                                                                </Badge>
                                                            }.into_any()
                                                        } else {
                                                            view! {
                                                                <Badge
                                                                    surface=BadgeSurface::Outline
                                                                    size=BadgeSize::Sm
                                                                >
                                                                    {right.clone()}
                                                                </Badge>
                                                            }.into_any()
                                                        }}
                                                        <Badge size=BadgeSize::Pill>{kind}</Badge>
                                                        <Badge variant=confidence_variant size=BadgeSize::Pill>
                                                            {format!("{}%", m.confidence)}
                                                        </Badge>
                                                    </div>

                                                    {subtitle.map(|s| view! {
                                                        <div class="text-[12px] text-zinc-500 dark:text-zinc-400 mt-0.5 leading-snug">{s}</div>
                                                    })}

                                                    {(!m.tags.is_empty()).then(|| view! {
                                                        <div class="flex flex-wrap gap-1 mt-1">
                                                            {m.tags.into_iter().map(|tag| view! {
                                                                <Badge surface=BadgeSurface::Outline>{tag}</Badge>
                                                            }).collect_view()}
                                                        </div>
                                                    })}

                                                    <div class="text-[11px] text-zinc-500 dark:text-zinc-400 mt-1">{m.explanation.clone().unwrap_or_default()}</div>
                                                    <div class="text-[10px] text-zinc-400 dark:text-zinc-500 mt-0.5">
                                                        {format!("ID: {}", m.right_external_id)}
                                                    </div>
                                                </div>

                                                <div class="flex flex-col lg:flex-row items-center gap-1.5 shrink-0">
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
                }
            }}

            // Link provider dialog
            {move || {
                let a = a();
                let links = provider_links.get();
                let already_linked: Vec<(String, String)> = links
                    .iter()
                    .map(|l| (l.provider.clone(), l.external_id.clone()))
                    .collect();
                view! {
                    <LinkProviderDialog
                        open=show_link_provider
                        artist_id=a.id
                        artist_name=a.name.clone()
                        already_linked=already_linked
                    />
                }
            }}

            // Edit artist dialog
            {move || {
                let a = a();
                let current_name = Signal::derive(move || {
                    artist.get().map(|a| a.name.clone()).unwrap_or_default()
                });
                let current_image_url = Signal::derive(move || {
                    artist.get().and_then(|a| a.image_url.clone())
                });
                let has_bio = Signal::derive(move || {
                    artist.get().map(|a| a.bio.is_some()).unwrap_or(false)
                });
                view! {
                    <EditArtistDialog
                        open=show_edit_artist
                        artist_id=a.id
                        current_name=current_name
                        current_image_url=current_image_url
                        has_bio=has_bio
                    />
                }
            }}

            // Confirmation dialogs
            <ConfirmDialog
                open=show_unmonitor_all
                title="Unmonitor All Albums"
                message="This will unmonitor all albums for this artist."
                confirm_label="Unmonitor All"
                on_confirm={
                    let artist_sig = artist;
                    move |_: bool| {
                        let Some(aid) = artist_sig.get_untracked().map(|a| a.id) else {
                            toast_missing_context("Artist");
                            return;
                        };
                        dispatch_with_toast(ServerAction::BulkMonitor { artist_id: aid, monitored: false }, "All albums unmonitored");
                    }
                }
            />
            <ConfirmDialog
                open=show_remove_artist
                title="Remove Artist"
                message="This will remove the artist and all associated data. This cannot be undone."
                confirm_label="Remove"
                danger=true
                checkbox_label="Also remove downloaded files from disk"
                on_confirm={
                    let artist_sig = artist;
                    move |remove_files: bool| {
                        let navigate = leptos_router::hooks::use_navigate();
                        let toaster = expect_toaster();
                        let Some(aid) = artist_sig.get_untracked().map(|a| a.id) else {
                            toast_missing_context("Artist");
                            return;
                        };
                        removing_artist.set(true);
                        leptos::task::spawn_local(async move {
                            match dispatch_action(ServerAction::RemoveArtist { artist_id: aid, remove_files }).await {
                                Ok(()) => {
                                    toaster.toast(
                                        ToastBuilder::new("Artist removed")
                                            .with_level(ToastLevel::Success)
                                            .with_position(ToastPosition::BottomRight)
                                            .with_expiry(Some(4_000)),
                                    );
                                    navigate("/library", Default::default());
                                }
                                Err(e) => {
                                    toaster.toast(
                                        ToastBuilder::new(format!("Error: {e}"))
                                            .with_level(ToastLevel::Error)
                                            .with_position(ToastPosition::BottomRight)
                                            .with_expiry(Some(8_000)),
                                    );
                                }
                            }
                            removing_artist.set(false);
                        });
                    }
                }
            />

            // Albums grid with sort — reactive
            <Panel>
                <PanelHeader>
                    <div class="flex items-center gap-3">
                        <PanelTitle>"Discography"</PanelTitle>
                        <span class={cls!(MUTED, "text-xs")}>{move || format!("{} albums", albums.get().len())}</span>
                    </div>
                    <div class="flex flex-wrap items-center gap-2">
                        {move || {
                            let a = a();
                            let artist_id = a.id;
                            view! {
                                <Button
                                    href={format!("/artists/{}/merge-albums", artist_id)}
                                >
                                    <GitMerge size=14 />
                                    "Merge Albums"
                                </Button>
                                <Button
                                    variant=ButtonVariant::Primary
                                    loading=monitor_all_loading
                                    on:click={
                                        move |_| {
                                            dispatch_with_toast_loading(ServerAction::BulkMonitor { artist_id, monitored: true }, "All albums monitored", Some(monitor_all_loading));
                                        }
                                    }>
                                    <BookmarkCheck size=14 />
                                    {move || if monitor_all_loading.get() { "Monitoring\u{2026}" } else { "Monitor All" }}
                                </Button>
                                <Button
                                    on:click=move |_| {
                                        show_unmonitor_all.set(true);
                                    }>
                                    <BookmarkX size=14 />
                                    "Unmonitor All"
                                </Button>
                            }
                        }}
                        <div class="w-px h-4 bg-black/10 dark:bg-white/10 mx-1"></div>
                        {move || {
                            let album_count = albums.get().len();
                            if album_count > 0 {
                                view! {
                                    <select
                                        class=SELECT
                                        aria-label="Sort albums"
                                        on:change=move |ev| {
                                            set_album_sort.set(event_target_value(&ev));
                                        }
                                    >
                                        <option value="type" selected=true>"By Type"</option>
                                        <option value="az">"A \u{2013} Z"</option>
                                        <option value="newest">"Newest First"</option>
                                        <option value="oldest">"Oldest First"</option>
                                    </select>
                                }.into_any()
                            } else {
                                view! { <span></span> }.into_any()
                            }
                        }}
                    </div>
                </PanelHeader>
                // Derive a sorted album list that only recomputes when albums or sort order change.
                {
                    let sorted_albums = Memo::new(move |_| {
                        let mut sorted = albums.get();
                        let sort_key = album_sort.get();
                        match sort_key.as_str() {
                            "az" => sorted.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase())),
                            "newest" => sorted.sort_by(|a, b| b.release_date.cmp(&a.release_date).then_with(|| a.title.cmp(&b.title))),
                            "oldest" => sorted.sort_by(|a, b| a.release_date.cmp(&b.release_date).then_with(|| a.title.cmp(&b.title))),
                            _ /* "type" */ => sorted.sort_by(|a, b| {
                                album_type_rank(a.album_type.as_deref(), &a.title)
                                    .cmp(&album_type_rank(b.album_type.as_deref(), &b.title))
                                    .then_with(|| b.release_date.cmp(&a.release_date))
                                    .then_with(|| a.title.cmp(&b.title))
                            }),
                        }
                        sorted
                    });

                    view! {
                        <Show when=move || sorted_albums.get().is_empty()>
                            <div class=EMPTY>"No albums synced. Hit Sync Albums to fetch from provider."</div>
                        </Show>
                        <Show when=move || !sorted_albums.get().is_empty()>
                            <div class={cls!(GLASS_BODY, "p-4")}>
                                <div class="grid grid-cols-[repeat(auto-fill,minmax(180px,1fr))] gap-5 max-md:grid-cols-[repeat(auto-fill,minmax(140px,1fr))] max-md:gap-3">
                                    <For
                                        each=move || sorted_albums.get()
                                        key=|album| album.id
                                        let:album
                                    >
                                        {
                                            let album_id = album.id;
                                            let is_explicit = album.explicit;
                                            let release_date = album.release_date.clone().unwrap_or_else(|| "\u{2014}".to_string());
                                            let at = album_type_label(album.album_type.as_deref(), &album.title);
                                            let detail_url = artist
                                                .get_untracked()
                                                .map(|a| format!("/artists/{}/albums/{}", a.id, album_id))
                                                .unwrap_or_else(|| "/library".to_string());

                                            // Reactive flags derived from the canonical albums signal
                                            let flags = Memo::new(move |_| {
                                                let all = albums.get();
                                                all.iter()
                                                    .find(|a| a.id == album_id)
                                                    .map(|a| (a.monitored, a.wanted, a.acquired))
                                                    .unwrap_or((false, false, false))
                                            });
                                            let is_monitored = Signal::derive(move || flags.get().0);

                                            // Reactive badge from job status
                                            let badge_signal = Signal::derive(move || {
                                                let (_, is_wanted, is_acquired) = flags.get();
                                                let all_jobs = jobs.get();
                                                let latest = build_latest_jobs(all_jobs);
                                                let ji = latest.get(&album_id).cloned();
                                                match ji.as_ref().map(|j| &j.status) {
                                                    Some(DownloadStatus::Downloading) => {
                                                        let j = ji.as_ref().unwrap();
                                                        SleeveBadge::Downloading { completed: j.completed_tracks, total: j.total_tracks }
                                                    }
                                                    Some(DownloadStatus::Resolving) => SleeveBadge::Downloading { completed: 0, total: 0 },
                                                    Some(DownloadStatus::Queued) => SleeveBadge::Queued,
                                                    Some(DownloadStatus::Failed) => SleeveBadge::Failed,
                                                    Some(DownloadStatus::Completed) | None => {
                                                        if is_acquired { SleeveBadge::Acquired }
                                                        else if is_wanted { SleeveBadge::Wanted }
                                                        else { SleeveBadge::None }
                                                    }
                                                }
                                            });

                                            view! {
                                                <AlbumCard
                                                    album=album
                                                    href=detail_url
                                                    cover_resolution=640
                                                    sleeve_badge=badge_signal
                                                    show_explicit=is_explicit
                                                    subtitle=format!("{release_date} \u{00b7} {at}")
                                                    monitor_toggle=MonitorToggle { album_id, is_monitored }
                                                />
                                            }
                                        }
                                    </For>
                                </div>
                            </div>
                        </Show>
                    }
                }
            </Panel>
        </div>
    }
}

/// Collapsible artist bio — shows first 3 lines with "Read more" toggle.
#[component]
fn ArtistBio(bio: String) -> impl IntoView {
    let expanded = RwSignal::new(false);
    let bio_clone = bio.clone();
    let is_long = bio.len() > 280;

    view! {
        <div class="text-[13px] leading-relaxed text-zinc-600 dark:text-zinc-400">
            <p
                class=move || {
                    if expanded.get() || !is_long {
                        ""
                    } else {
                        "line-clamp-3"
                    }
                }
            >
                {bio_clone}
            </p>
            {if is_long {
                view! {
                    <button
                        type="button"
                        class="mt-1 text-[12px] font-medium text-blue-500 dark:text-blue-400 hover:text-blue-600 dark:hover:text-blue-300 bg-transparent border-none cursor-pointer p-0"
                        on:click=move |_| expanded.update(|v| *v = !*v)
                    >
                        {move || if expanded.get() { "Show less" } else { "Read more" }}
                    </button>
                }.into_any()
            } else {
                view! { <span></span> }.into_any()
            }}
        </div>
    }
}
