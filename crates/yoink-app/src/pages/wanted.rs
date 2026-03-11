use crate::cls;
use std::collections::{HashMap, HashSet};

use leptos::prelude::*;
use lucide_leptos::{ChevronDown, ChevronRight, X};

use yoink_shared::{
    DownloadJob, DownloadStatus, MonitoredAlbum, MonitoredArtist, ServerAction, TrackInfo,
    WantedAlbumGroup, WantedArtistGroup, album_cover_url, build_latest_jobs, build_wanted_tree,
};

use crate::components::toast::dispatch_with_toast;
use crate::components::{
    Badge, BadgeSize, Button, ButtonVariant, ErrorPanel, PageHeader, PageShell, Panel, PanelHeader,
    PanelTitle, download_status_badge_variant, fallback_initial,
};
use crate::hooks::{set_page_title, use_sse_version};
use crate::styles::{EMPTY, MUTED};

#[cfg(feature = "hydrate")]
use leptoaster::{ToastBuilder, ToastLevel, ToastPosition, expect_toaster};

const ICON_BTN: &str = "inline-flex items-center justify-center size-7 border border-black/[.08] dark:border-white/10 rounded-lg bg-white/50 dark:bg-zinc-800/50 backdrop-blur-[8px] text-zinc-500 dark:text-zinc-400 cursor-pointer transition-all duration-150 p-0 font-inherit text-[13px] hover:bg-blue-500 hover:border-blue-500 hover:text-white hover:shadow-[0_2px_8px_rgba(59,130,246,.3)]";
const TREE_ARTIST_HEADER: &str = "flex items-center gap-2 px-5 py-3 border-b border-black/[.04] dark:border-white/[.04] bg-blue-500/[.03] dark:bg-blue-500/[.05]";
const TREE_ALBUM_ROW: &str =
    "flex items-center gap-3 px-5 py-3 border-b border-black/[.04] dark:border-white/[.04]";

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WantedAlbumWithTracks {
    pub album: MonitoredAlbum,
    pub tracks: Vec<TrackInfo>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WantedData {
    pub albums: Vec<WantedAlbumWithTracks>,
    pub artists: Vec<MonitoredArtist>,
    pub jobs: Vec<DownloadJob>,
}

#[server(GetWantedData, "/leptos")]
pub async fn get_wanted_data() -> Result<WantedData, ServerFnError> {
    let ctx = crate::actions::require_ctx()?;

    let artists = ctx.monitored_artists.read().await.clone();
    let jobs = ctx.download_jobs.read().await.clone();
    let wanted_albums: Vec<MonitoredAlbum> = ctx
        .monitored_albums
        .read()
        .await
        .iter()
        .filter(|a| a.wanted || a.partially_wanted)
        .cloned()
        .collect();

    let fetches = wanted_albums.iter().map(|album| {
        let fetch_tracks = ctx.fetch_tracks.clone();
        let album = album.clone();
        async move {
            let tracks = (fetch_tracks)(album.id).await.map_err(|e| {
                ServerFnError::new(format!("failed to load tracks for album {}: {e}", album.id))
            })?;
            Ok::<WantedAlbumWithTracks, ServerFnError>(WantedAlbumWithTracks { album, tracks })
        }
    });
    let albums = futures::future::try_join_all(fetches).await?;

    Ok(WantedData {
        albums,
        artists,
        jobs,
    })
}

#[component]
pub fn WantedPage() -> impl IntoView {
    set_page_title("Wanted");
    let version = use_sse_version();
    let wanted_data = Resource::new(move || version.get(), |_| get_wanted_data());

    view! {
        <PageShell active="wanted">
                <Transition fallback=move || view! {
                    <div>
                        <PageHeader title="Wanted"></PageHeader>
                        <div class="p-6 max-md:p-4"><div class=EMPTY>"Loading wanted tree..."</div></div>
                    </div>
                }>
                    {move || {
                        wanted_data.get().map(|result| match result {
                            Err(e) => view! {
                                <div class="p-6 max-md:p-4">
                                    <ErrorPanel
                                        message="Failed to load wanted items."
                                        details=e.to_string()
                                        retry_href="/wanted"
                                    />
                                </div>
                            }
                            .into_any(),
                            Ok(data) => view! { <WantedContent data=data /> }.into_any(),
                        })
                    }}
                </Transition>
        </PageShell>
    }
}

#[component]
fn WantedContent(data: WantedData) -> impl IntoView {
    let latest_jobs = build_latest_jobs(data.jobs);

    let albums_only: Vec<MonitoredAlbum> = data.albums.iter().map(|x| x.album.clone()).collect();
    let queueable_album_ids: Vec<yoink_shared::Uuid> = albums_only
        .iter()
        .filter(|album| {
            let job_status = latest_jobs.get(&album.id).map(|j| j.status.clone());
            job_status.is_none()
                || matches!(
                    job_status,
                    Some(DownloadStatus::Failed) | Some(DownloadStatus::Completed)
                )
        })
        .map(|a| a.id)
        .collect();
    let failed_album_ids: Vec<yoink_shared::Uuid> = albums_only
        .iter()
        .filter(|album| {
            matches!(
                latest_jobs.get(&album.id).map(|j| j.status.clone()),
                Some(DownloadStatus::Failed)
            )
        })
        .map(|a| a.id)
        .collect();

    let tree: Vec<WantedArtistGroup> = build_wanted_tree(
        &data.artists,
        data.albums
            .iter()
            .map(|x| (x.album.clone(), x.tracks.clone()))
            .collect(),
    );

    let total_albums: usize = tree.iter().map(|a| a.albums.len()).sum();
    let total_tracks: usize = tree
        .iter()
        .flat_map(|a| a.albums.iter())
        .map(|ag| {
            if ag.album.monitored {
                0
            } else {
                ag.wanted_tracks.len()
            }
        })
        .sum();
    let summary = if total_tracks > 0 {
        format!("{total_albums} albums · {total_tracks} tracks")
    } else {
        format!("{total_albums} albums")
    };

    let queue_all_loading = RwSignal::new(false);
    let retry_all_loading = RwSignal::new(false);

    let open_artists = RwSignal::new(
        tree.iter()
            .map(|g| g.artist_id)
            .collect::<HashSet<yoink_shared::Uuid>>(),
    );
    let open_albums = RwSignal::new(
        tree.iter()
            .flat_map(|g| g.albums.iter().map(|a| a.album.id))
            .collect::<HashSet<yoink_shared::Uuid>>(),
    );

    let latest_jobs_sv = StoredValue::new(latest_jobs);

    view! {
        <PageHeader title="Wanted" subtitle=summary.clone()></PageHeader>

        <div class="p-6 max-md:p-4">
            <Panel>
                <PanelHeader>
                    <PanelTitle>"Wanted Tree"</PanelTitle>
                    <div class="flex flex-wrap items-center gap-2">
                        {if !queueable_album_ids.is_empty() {
                            let ids = queueable_album_ids.clone();
                            let count = ids.len();
                            view! {
                                <Button variant=ButtonVariant::Primary loading=queue_all_loading
                                    on:click=move |_| {
                                        queue_all_loading.set(true);
                                        let ids = ids.clone();
                                        leptos::task::spawn_local(async move {
                                            let _total = ids.len();
                                            let mut _failed = 0usize;
                                            for album_id in ids {
                                                if crate::actions::dispatch_action(ServerAction::RetryDownload { album_id }).await.is_err() {
                                                    _failed += 1;
                                                }
                                            }
                                            #[cfg(feature = "hydrate")]
                                            {
                                                let toaster = expect_toaster();
                                                if _failed == 0 {
                                                    toaster.toast(
                                                        ToastBuilder::new(format!("Queued {_total} albums"))
                                                            .with_level(ToastLevel::Success)
                                                            .with_position(ToastPosition::BottomRight)
                                                            .with_expiry(Some(4_000)),
                                                    );
                                                } else {
                                                    toaster.toast(
                                                        ToastBuilder::new(format!("Queued {}/{_total} albums, {_failed} failed", _total - _failed))
                                                            .with_level(ToastLevel::Error)
                                                            .with_position(ToastPosition::BottomRight)
                                                            .with_expiry(Some(8_000)),
                                                    );
                                                }
                                            }
                                            queue_all_loading.set(false);
                                        });
                                    }>
                                    {move || if queue_all_loading.get() { "Queueing...".to_string() } else { format!("Download All ({count})") }}
                                </Button>
                            }
                                .into_any()
                        } else {
                            view! { <span></span> }.into_any()
                        }}
                        {if !failed_album_ids.is_empty() {
                            let ids = failed_album_ids.clone();
                            let count = ids.len();
                            view! {
                                <Button variant=ButtonVariant::Danger loading=retry_all_loading
                                    on:click=move |_| {
                                        retry_all_loading.set(true);
                                        let ids = ids.clone();
                                        leptos::task::spawn_local(async move {
                                            let _total = ids.len();
                                            let mut _failed = 0usize;
                                            for album_id in ids {
                                                if crate::actions::dispatch_action(ServerAction::RetryDownload { album_id }).await.is_err() {
                                                    _failed += 1;
                                                }
                                            }
                                            #[cfg(feature = "hydrate")]
                                            {
                                                let toaster = expect_toaster();
                                                if _failed == 0 {
                                                    toaster.toast(
                                                        ToastBuilder::new(format!("Retried {_total} albums"))
                                                            .with_level(ToastLevel::Success)
                                                            .with_position(ToastPosition::BottomRight)
                                                            .with_expiry(Some(4_000)),
                                                    );
                                                } else {
                                                    toaster.toast(
                                                        ToastBuilder::new(format!("Retried {}/{_total} albums, {_failed} failed", _total - _failed))
                                                            .with_level(ToastLevel::Error)
                                                            .with_position(ToastPosition::BottomRight)
                                                            .with_expiry(Some(8_000)),
                                                    );
                                                }
                                            }
                                            retry_all_loading.set(false);
                                        });
                                    }>
                                    {move || if retry_all_loading.get() { "Retrying...".to_string() } else { format!("Retry Failed ({count})") }}
                                </Button>
                            }
                                .into_any()
                        } else {
                            view! { <span></span> }.into_any()
                        }}
                        <span class={cls!(MUTED, "text-xs")}>{summary}</span>
                    </div>
                </PanelHeader>

                {if tree.is_empty() {
                    view! { <div class=EMPTY>"All wanted items are acquired."</div> }.into_any()
                } else {
                    view! {
                        <div>
                            {tree.into_iter().map(|artist_group| {
                                let aid = artist_group.artist_id;
                                let is_open_artist = Signal::derive(move || open_artists.get().contains(&aid));
                                let artist_count = artist_group.albums.len();

                                let artist_albums = artist_group.albums.clone();

                                view! {
                                    <div>
                                        <button
                                            type="button"
                                            class=TREE_ARTIST_HEADER
                                            on:click=move |_| {
                                                open_artists.update(|set| {
                                                    if !set.remove(&aid) {
                                                        set.insert(aid);
                                                    }
                                                });
                                            }
                                        >
                                            {move || if is_open_artist.get() {
                                                view! { <ChevronDown size=14 /> }.into_any()
                                            } else {
                                                view! { <ChevronRight size=14 /> }.into_any()
                                            }}
                                            <span class="text-[13px] font-bold text-blue-500 dark:text-blue-400 uppercase tracking-wide">{artist_group.artist_name}</span>
                                            <span class={cls!(MUTED, "text-xs")}>{format!("{artist_count} albums")}</span>
                                        </button>

                                        {move || {
                                            if !is_open_artist.get() {
                                                return view! { <span></span> }.into_any();
                                            }

                                            let albums = artist_albums.clone();
                                            view! {
                                                <div>
                                                    {albums.into_iter().map(|album_group| {
                                                        let album_id = album_group.album.id;
                                                        let is_open_album = Signal::derive(move || open_albums.get().contains(&album_id));
                                                        let wanted_track_count = album_group.wanted_tracks.len();
                                                        let jobs = latest_jobs_sv.get_value();
                                                        view! {
                                                            <div>
                                                                <div class=TREE_ALBUM_ROW>
                                                                    <button
                                                                        type="button"
                                                                        class="inline-flex items-center justify-center size-5 rounded bg-transparent border-none p-0 text-zinc-500 dark:text-zinc-400"
                                                                        on:click=move |_| {
                                                                            open_albums.update(|set| {
                                                                                if !set.remove(&album_id) {
                                                                                    set.insert(album_id);
                                                                                }
                                                                            });
                                                                        }
                                                                    >
                                                                        {move || if is_open_album.get() {
                                                                            view! { <ChevronDown size=13 /> }.into_any()
                                                                        } else {
                                                                            view! { <ChevronRight size=13 /> }.into_any()
                                                                        }}
                                                                    </button>
                                                                    <AlbumWantedRow album_group=album_group.clone() latest_jobs=jobs />
                                                                </div>

                                                                <Show when=move || is_open_album.get()>
                                                                    <div class="pl-14 pr-5 pb-2 border-b border-black/[.04] dark:border-white/[.04]">
                                                                        {if album_group.album.monitored {
                                                                            view! {
                                                                                <div class="text-xs text-zinc-500 dark:text-zinc-400 py-2">
                                                                                    "Full album monitored"
                                                                                </div>
                                                                            }.into_any()
                                                                        } else if wanted_track_count == 0 {
                                                                            view! {
                                                                                <div class="text-xs text-zinc-500 dark:text-zinc-400 py-2">
                                                                                    "No wanted tracks"
                                                                                </div>
                                                                            }.into_any()
                                                                        } else {
                                                                            view! {
                                                                                <div class="py-1.5 space-y-1.5">
                                                                                    {album_group.wanted_tracks.iter().map(|t| {
                                                                                        view! {
                                                                                            <div class="flex items-center gap-2 text-xs text-zinc-600 dark:text-zinc-300">
                                                                                                <span class="tabular-nums text-zinc-400 dark:text-zinc-500 min-w-8">{format!("{}-{}", t.disc_number, t.track_number)}</span>
                                                                                                <span class="flex-1 truncate">{t.title.clone()}</span>
                                                                                                <span class="text-amber-600 dark:text-amber-300">"Wanted"</span>
                                                                                            </div>
                                                                                        }
                                                                                    }).collect_view()}
                                                                                </div>
                                                                            }.into_any()
                                                                        }}
                                                                    </div>
                                                                </Show>
                                                            </div>
                                                        }
                                                    }).collect_view()}
                                                </div>
                                            }.into_any()
                                        }}
                                    </div>
                                }
                            }).collect_view()}
                        </div>
                    }.into_any()
                }}
            </Panel>
        </div>
    }
}

#[component]
fn AlbumWantedRow(
    album_group: WantedAlbumGroup,
    latest_jobs: HashMap<yoink_shared::Uuid, DownloadJob>,
) -> impl IntoView {
    let album = album_group.album;
    let album_title = album.title.clone();
    let release_date = album
        .release_date
        .clone()
        .unwrap_or_else(|| "—".to_string());
    let cover_url = album_cover_url(&album, 120);
    let fallback_initial = fallback_initial(&album_title);

    let latest_job = latest_jobs.get(&album.id).cloned();
    let job_status = latest_job.as_ref().map(|j| j.status.clone());
    let is_failed = matches!(job_status, Some(DownloadStatus::Failed));
    let is_queueable = job_status.is_none()
        || matches!(
            job_status,
            Some(DownloadStatus::Failed) | Some(DownloadStatus::Completed)
        );

    let status_variant = job_status
        .as_ref()
        .map(download_status_badge_variant)
        .unwrap_or_default();
    let status_text = job_status
        .as_ref()
        .map(|s| s.as_str().to_string())
        .unwrap_or_else(|| "not queued".to_string());

    let album_id = album.id;
    let unmonitor_action = if album.monitored {
        ServerAction::ToggleAlbumMonitor {
            album_id,
            monitored: false,
        }
    } else {
        ServerAction::BulkToggleTrackMonitor {
            album_id,
            monitored: false,
        }
    };

    view! {
        <>
            {match cover_url {
                Some(url) => view! {
                    <img class="size-10 rounded-md object-cover shrink-0 bg-zinc-200 dark:bg-zinc-800" src=url alt="" />
                }.into_any(),
                None => view! {
                    <div class="size-10 rounded-md inline-flex items-center justify-center bg-zinc-200 dark:bg-zinc-800 text-zinc-400 dark:text-zinc-600 font-bold text-sm shrink-0">{fallback_initial}</div>
                }.into_any(),
            }}
            <div class="flex-1 min-w-0">
                <div class="text-sm font-semibold text-zinc-900 dark:text-zinc-100 truncate">{album_title}</div>
                <div class="text-xs text-zinc-500 dark:text-zinc-400">{release_date}</div>
            </div>
            <Badge size=BadgeSize::Pill variant=status_variant>{status_text}</Badge>
            <div class="flex gap-1.5 shrink-0 items-center">
                {if is_failed {
                    view! {
                        <Button variant=ButtonVariant::Danger
                            on:click=move |_| {
                                dispatch_with_toast(ServerAction::RetryDownload { album_id }, "Download queued for retry");
                            }>
                            "Retry"
                        </Button>
                    }.into_any()
                } else if is_queueable {
                    view! {
                        <Button on:click=move |_| {
                                dispatch_with_toast(ServerAction::RetryDownload { album_id }, "Download queued");
                            }>
                            "Download"
                        </Button>
                    }.into_any()
                } else {
                    view! { <span></span> }.into_any()
                }}
                <button
                    type="button"
                    class=ICON_BTN
                    title="Unmonitor"
                    aria-label="Unmonitor"
                    on:click=move |_| {
                        dispatch_with_toast(unmonitor_action.clone(), "Item unmonitored");
                    }
                >
                    <X size=14 />
                </button>
            </div>
        </>
    }
}
