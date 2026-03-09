use crate::cls;
use leptos::prelude::*;
use lucide_leptos::{ArrowDown, ArrowUp, ChevronDown, ChevronRight};

use yoink_shared::{LibraryTrack, SearchTrackResult, ServerAction};

use crate::components::toast::dispatch_with_toast_loading;
use crate::components::{
    Badge, BadgeVariant, Breadcrumb, BreadcrumbItem, Button, ButtonVariant, PageShell, Panel,
    PanelBody, PanelHeader, PanelTitle,
};
use crate::hooks::set_page_title;
use crate::search_result_keys::provider_result_key;
use crate::styles::{EMPTY, MUTED, SEARCH_INPUT, SELECT};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SearchTracksResult {
    pub results: Vec<SearchTrackResult>,
    pub error: Option<String>,
}

#[server(GetLibraryTracksData, "/leptos")]
pub async fn get_library_tracks_data() -> Result<Vec<LibraryTrack>, ServerFnError> {
    let ctx = use_context::<yoink_shared::ServerContext>()
        .ok_or_else(|| ServerFnError::new("ServerContext not available"))?;

    (ctx.fetch_library_tracks)()
        .await
        .map_err(ServerFnError::new)
}

#[server(SearchTracksLibrary, "/leptos")]
pub async fn search_tracks_library(query: String) -> Result<SearchTracksResult, ServerFnError> {
    let ctx = use_context::<yoink_shared::ServerContext>()
        .ok_or_else(|| ServerFnError::new("ServerContext not available"))?;

    let q = query.trim().to_string();
    if q.is_empty() {
        return Ok(SearchTracksResult {
            results: vec![],
            error: None,
        });
    }

    match (ctx.search_tracks)(q).await {
        Ok(results) => Ok(SearchTracksResult {
            results,
            error: None,
        }),
        Err(err) => Ok(SearchTracksResult {
            results: vec![],
            error: Some(err.to_string()),
        }),
    }
}

// ── Sort direction ──────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SortDir {
    Asc,
    Desc,
}

impl SortDir {
    fn toggle(self) -> Self {
        match self {
            SortDir::Asc => SortDir::Desc,
            SortDir::Desc => SortDir::Asc,
        }
    }
}

// ── Sort key ────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SortKey {
    Title,
    Artist,
    Album,
    Duration,
}

impl SortKey {
    fn value(self) -> &'static str {
        match self {
            SortKey::Title => "az",
            SortKey::Artist => "artist",
            SortKey::Album => "album",
            SortKey::Duration => "duration",
        }
    }

    fn from_value(s: &str) -> Self {
        match s {
            "az" => SortKey::Title,
            "album" => SortKey::Album,
            "duration" => SortKey::Duration,
            _ => SortKey::Artist,
        }
    }
}

// ── Filter key ──────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FilterKey {
    All,
    Wanted,
    Acquired,
    Idle,
}

impl FilterKey {
    fn value(self) -> &'static str {
        match self {
            FilterKey::All => "all",
            FilterKey::Wanted => "wanted",
            FilterKey::Acquired => "acquired",
            FilterKey::Idle => "idle",
        }
    }

    fn from_value(s: &str) -> Self {
        match s {
            "wanted" => FilterKey::Wanted,
            "acquired" => FilterKey::Acquired,
            "idle" => FilterKey::Idle,
            _ => FilterKey::All,
        }
    }

    fn matches(self, track: &yoink_shared::TrackInfo) -> bool {
        match self {
            FilterKey::All => true,
            FilterKey::Wanted => track.monitored && !track.acquired,
            FilterKey::Acquired => track.acquired,
            FilterKey::Idle => !track.monitored && !track.acquired,
        }
    }
}

// ── Group by ────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GroupBy {
    None,
    Artist,
    Album,
}

impl GroupBy {
    fn value(self) -> &'static str {
        match self {
            GroupBy::None => "none",
            GroupBy::Artist => "artist",
            GroupBy::Album => "album",
        }
    }

    fn from_value(s: &str) -> Self {
        match s {
            "artist" => GroupBy::Artist,
            "album" => GroupBy::Album,
            _ => GroupBy::None,
        }
    }
}

// ── Stat card helper ────────────────────────────────────────

const STAT_CARD: &str = "flex items-center gap-2.5 px-3.5 py-2.5 rounded-lg bg-white/50 dark:bg-zinc-800/40 border border-black/[.04] dark:border-white/[.06] cursor-pointer transition-all duration-150 hover:border-blue-500/30 hover:bg-blue-500/[.04]";
const STAT_CARD_ACTIVE: &str = "flex items-center gap-2.5 px-3.5 py-2.5 rounded-lg bg-blue-500/[.08] dark:bg-blue-500/[.12] border border-blue-500/30 dark:border-blue-500/40 cursor-pointer transition-all duration-150";

// ── Table header cell ───────────────────────────────────────

const TH_BASE: &str = "text-[11px] font-semibold uppercase tracking-wider text-zinc-400 dark:text-zinc-500 select-none cursor-pointer transition-colors duration-150 hover:text-zinc-600 dark:hover:text-zinc-300 flex items-center gap-1";
const TH_ACTIVE: &str = "text-[11px] font-semibold uppercase tracking-wider text-blue-500 dark:text-blue-400 select-none cursor-pointer flex items-center gap-1";

// ── Group header ────────────────────────────────────────────

const GROUP_HEADER: &str = "flex items-center gap-2 px-5 max-md:px-3.5 py-2.5 bg-zinc-50/80 dark:bg-zinc-800/80 border-b border-black/[.04] dark:border-white/[.04] cursor-pointer select-none transition-colors duration-100 hover:bg-zinc-100/80 dark:hover:bg-zinc-700/40";

// ── Cover thumbnail ─────────────────────────────────────────

const COVER_THUMB: &str =
    "w-10 h-10 rounded-md overflow-hidden bg-zinc-200 dark:bg-zinc-700 shrink-0";
const COVER_FALLBACK: &str = "w-full h-full flex items-center justify-center text-xs font-bold text-zinc-400 dark:text-zinc-600";
const DESKTOP_TRACK_GRID: &str =
    "grid-cols-[40px_minmax(0,2.4fr)_minmax(120px,1fr)_minmax(120px,1fr)_max-content_max-content]";

// ── Main component ──────────────────────────────────────────

#[component]
pub fn LibraryTracksTab() -> impl IntoView {
    let data = Resource::new(|| (), |_| get_library_tracks_data());
    let (query, set_query) = signal(String::new());
    let (sort_key, set_sort_key) = signal(SortKey::Artist);
    let (sort_dir, set_sort_dir) = signal(SortDir::Asc);
    let (filter_key, set_filter_key) = signal(FilterKey::All);
    let (group_by, set_group_by) = signal(GroupBy::None);
    // Track which groups are collapsed (by group name)
    let (collapsed_groups, set_collapsed_groups) =
        signal(std::collections::HashSet::<String>::new());

    let search_result: Resource<Result<SearchTracksResult, ServerFnError>> = Resource::new(
        move || query.get(),
        |q| async move { search_tracks_library(q).await },
    );

    // Helper to sort by clicking column headers
    let on_sort_click = move |col: SortKey| {
        move |_: leptos::ev::MouseEvent| {
            if sort_key.get() == col {
                set_sort_dir.set(sort_dir.get().toggle());
            } else {
                set_sort_key.set(col);
                set_sort_dir.set(SortDir::Asc);
            }
        }
    };

    view! {
        <Transition fallback=move || view! {
            <div class="p-6 max-md:p-4 space-y-5">
                // Skeleton stat bar
                <Panel>
                    <PanelBody>
                        <div class="flex flex-wrap gap-2 animate-pulse">
                            {(0..4).map(|_| view! {
                                <div class="h-12 w-28 rounded-lg bg-zinc-200 dark:bg-zinc-700"></div>
                            }).collect_view()}
                        </div>
                    </PanelBody>
                </Panel>
                // Skeleton table
                <Panel>
                    <PanelHeader>
                        <div class="h-4 w-20 bg-zinc-200 dark:bg-zinc-700 rounded animate-pulse"></div>
                    </PanelHeader>
                    <div class="px-5 py-4">
                        {(0..8).map(|_| view! {
                            <div class="flex items-center gap-3 mb-3 animate-pulse">
                                <div class="w-10 h-10 rounded-md bg-zinc-200 dark:bg-zinc-700 shrink-0"></div>
                                <div class="flex-1">
                                    <div class="h-3.5 w-48 bg-zinc-200 dark:bg-zinc-700 rounded mb-1.5"></div>
                                    <div class="h-3 w-32 bg-zinc-200 dark:bg-zinc-700 rounded"></div>
                                </div>
                                <div class="h-3.5 w-10 bg-zinc-200 dark:bg-zinc-700 rounded"></div>
                                <div class="h-5 w-16 bg-zinc-200 dark:bg-zinc-700 rounded"></div>
                            </div>
                        }).collect_view()}
                    </div>
                </Panel>
            </div>
        }>
            {move || {
                data.get().map(|result| match result {
                    Err(e) => view! { <div class="p-6 text-sm text-red-500">{e.to_string()}</div> }.into_any(),
                    Ok(tracks) => {
                        let tracks = StoredValue::new(tracks);
                        view! {
                            <div class="p-6 max-md:p-4 space-y-5">
                                // ── Summary stat bar ────────────────────
                                {move || {
                                    tracks.with_value(|all| {
                                        let total = all.len();
                                        let acquired = all.iter().filter(|r| r.track.acquired).count();
                                        let wanted = all.iter().filter(|r| r.track.monitored && !r.track.acquired).count();
                                        let idle = total - acquired - wanted;

                                        let current_filter = filter_key.get();

                                        let stat = move |label: &'static str, count: usize, filter_val: FilterKey, color_dot: &'static str| {
                                            let is_active = current_filter == filter_val;
                                            let card_class = if is_active { STAT_CARD_ACTIVE } else { STAT_CARD };
                                            view! {
                                                <button
                                                    type="button"
                                                    class=card_class
                                                    on:click=move |_| {
                                                        if filter_key.get() == filter_val {
                                                            set_filter_key.set(FilterKey::All);
                                                        } else {
                                                            set_filter_key.set(filter_val);
                                                        }
                                                    }
                                                >
                                                    <span class={cls!("w-2 h-2 rounded-full shrink-0", color_dot)}></span>
                                                    <div class="text-left">
                                                        <div class="text-lg font-bold tabular-nums text-zinc-900 dark:text-zinc-100 leading-none">{count}</div>
                                                        <div class="text-[10px] font-medium text-zinc-500 dark:text-zinc-400 uppercase tracking-wider">{label}</div>
                                                    </div>
                                                </button>
                                            }
                                        };

                                        view! {
                                            <Panel>
                                                <PanelBody>
                                                    <div class="flex flex-wrap gap-2">
                                                        {stat("Total", total, FilterKey::All, "bg-blue-500")}
                                                        {stat("Acquired", acquired, FilterKey::Acquired, "bg-green-500")}
                                                        {stat("Wanted", wanted, FilterKey::Wanted, "bg-amber-500")}
                                                        {stat("Idle", idle, FilterKey::Idle, "bg-zinc-400 dark:bg-zinc-600")}
                                                    </div>
                                                </PanelBody>
                                            </Panel>
                                        }
                                    })
                                }}

                                // ── Filter / sort / group toolbar ───────
                                <Panel>
                                    <PanelBody>
                                        <div class="flex flex-wrap items-center gap-2">
                                            <input
                                                type="text"
                                                class={cls!(SEARCH_INPUT, "max-w-[360px]")}
                                                placeholder="Search tracks (local + provider)..."
                                                prop:value=move || query.get()
                                                on:input=move |ev| set_query.set(event_target_value(&ev))
                                            />
                                            <select
                                                class=SELECT
                                                prop:value=move || filter_key.get().value()
                                                on:change=move |ev| set_filter_key.set(FilterKey::from_value(&event_target_value(&ev)))
                                            >
                                                <option value="all">"All"</option>
                                                <option value="wanted">"Wanted"</option>
                                                <option value="acquired">"Acquired"</option>
                                                <option value="idle">"Idle"</option>
                                            </select>
                                            <select
                                                class=SELECT
                                                prop:value=move || sort_key.get().value()
                                                on:change=move |ev| {
                                                    set_sort_key.set(SortKey::from_value(&event_target_value(&ev)));
                                                    set_sort_dir.set(SortDir::Asc);
                                                }
                                            >
                                                <option value="az">"A-Z"</option>
                                                <option value="album">"By Album"</option>
                                                <option value="artist">"By Artist"</option>
                                                <option value="duration">"Duration"</option>
                                            </select>
                                            <select
                                                class=SELECT
                                                prop:value=move || group_by.get().value()
                                                on:change=move |ev| {
                                                    set_group_by.set(GroupBy::from_value(&event_target_value(&ev)));
                                                    set_collapsed_groups.set(std::collections::HashSet::new());
                                                }
                                            >
                                                <option value="none">"No Grouping"</option>
                                                <option value="artist">"Group by Artist"</option>
                                                <option value="album">"Group by Album"</option>
                                            </select>
                                        </div>
                                    </PanelBody>
                                </Panel>

                                // ── Tracks table ────────────────────────
                                <Panel>
                                    <PanelHeader>
                                        <PanelTitle>"Tracks"</PanelTitle>
                                        <span class={cls!(MUTED, "text-xs tabular-nums")}>
                                            {move || {
                                                let q = query.get().trim().to_lowercase();
                                                let filter = filter_key.get();
                                                tracks.with_value(|all| {
                                                    let count = all.iter().filter(|row| {
                                                        let t = &row.track;
                                                        if !q.is_empty()
                                                            && !t.title.to_lowercase().contains(&q)
                                                            && !row.album_title.to_lowercase().contains(&q)
                                                            && !row.artist_name.to_lowercase().contains(&q)
                                                        {
                                                            return false;
                                                        }
                                                        filter.matches(t)
                                                    }).count();
                                                    format!("{count} tracks")
                                                })
                                            }}
                                        </span>
                                    </PanelHeader>
                                    <PanelBody class="p-0!">
                                        // ── Table header row (hidden on mobile) ──
                                        <div class={cls!(
                                            "max-md:hidden grid gap-3 px-5 py-2.5 border-b border-black/[.06] dark:border-white/[.06] items-center",
                                            DESKTOP_TRACK_GRID,
                                        )}>
                                            <span></span>
                                            <span
                                                class=move || if sort_key.get() == SortKey::Title { TH_ACTIVE } else { TH_BASE }
                                                on:click=on_sort_click(SortKey::Title)
                                            >
                                                "Title"
                                                {move || {
                                                    if sort_key.get() == SortKey::Title {
                                                        if sort_dir.get() == SortDir::Asc {
                                                            view! { <ArrowUp size=12 /> }.into_any()
                                                        } else {
                                                            view! { <ArrowDown size=12 /> }.into_any()
                                                        }
                                                    } else {
                                                        view! { <span></span> }.into_any()
                                                    }
                                                }}
                                            </span>
                                            <span
                                                class=move || if sort_key.get() == SortKey::Artist { TH_ACTIVE } else { TH_BASE }
                                                on:click=on_sort_click(SortKey::Artist)
                                            >
                                                "Artist"
                                                {move || {
                                                    if sort_key.get() == SortKey::Artist {
                                                        if sort_dir.get() == SortDir::Asc {
                                                            view! { <ArrowUp size=12 /> }.into_any()
                                                        } else {
                                                            view! { <ArrowDown size=12 /> }.into_any()
                                                        }
                                                    } else {
                                                        view! { <span></span> }.into_any()
                                                    }
                                                }}
                                            </span>
                                            <span
                                                class=move || if sort_key.get() == SortKey::Album { TH_ACTIVE } else { TH_BASE }
                                                on:click=on_sort_click(SortKey::Album)
                                            >
                                                "Album"
                                                {move || {
                                                    if sort_key.get() == SortKey::Album {
                                                        if sort_dir.get() == SortDir::Asc {
                                                            view! { <ArrowUp size=12 /> }.into_any()
                                                        } else {
                                                            view! { <ArrowDown size=12 /> }.into_any()
                                                        }
                                                    } else {
                                                        view! { <span></span> }.into_any()
                                                    }
                                                }}
                                            </span>
                                            <span
                                                class=move || if sort_key.get() == SortKey::Duration { TH_ACTIVE } else { TH_BASE }
                                                on:click=on_sort_click(SortKey::Duration)
                                            >
                                                "Time"
                                                {move || {
                                                    if sort_key.get() == SortKey::Duration {
                                                        if sort_dir.get() == SortDir::Asc {
                                                            view! { <ArrowUp size=12 /> }.into_any()
                                                        } else {
                                                            view! { <ArrowDown size=12 /> }.into_any()
                                                        }
                                                    } else {
                                                        view! { <span></span> }.into_any()
                                                    }
                                                }}
                                            </span>
                                            <span class={cls!(TH_BASE, "cursor-default hover:text-zinc-400 dark:hover:text-zinc-500")}>"Status"</span>
                                        </div>

                                        // ── Track rows ──
                                        {move || {
                                            let q = query.get().trim().to_lowercase();
                                            let filter = filter_key.get();
                                            let sort = sort_key.get();
                                            let dir = sort_dir.get();
                                            let grouping = group_by.get();
                                            let collapsed = collapsed_groups.get();

                                            tracks.with_value(|all| {
                                                let mut rows: Vec<_> = all
                                                    .iter()
                                                    .filter(|row| {
                                                        let t = &row.track;
                                                        if !q.is_empty()
                                                            && !t.title.to_lowercase().contains(&q)
                                                            && !row.album_title.to_lowercase().contains(&q)
                                                            && !row.artist_name.to_lowercase().contains(&q)
                                                        {
                                                            return false;
                                                        }
                                                        filter.matches(t)
                                                    })
                                                    .cloned()
                                                    .collect();

                                                // Sort
                                                rows.sort_by(|a, b| {
                                                    let cmp = match sort {
                                                        SortKey::Title => a.track.title.to_lowercase().cmp(&b.track.title.to_lowercase()),
                                                        SortKey::Album => a.album_title.to_lowercase().cmp(&b.album_title.to_lowercase())
                                                            .then_with(|| a.track.disc_number.cmp(&b.track.disc_number))
                                                            .then_with(|| a.track.track_number.cmp(&b.track.track_number)),
                                                        SortKey::Duration => a.track.duration_secs.cmp(&b.track.duration_secs),
                                                        SortKey::Artist => a.artist_name.to_lowercase().cmp(&b.artist_name.to_lowercase())
                                                            .then_with(|| a.album_title.to_lowercase().cmp(&b.album_title.to_lowercase()))
                                                            .then_with(|| a.track.disc_number.cmp(&b.track.disc_number))
                                                            .then_with(|| a.track.track_number.cmp(&b.track.track_number)),
                                                    };
                                                    if dir == SortDir::Desc { cmp.reverse() } else { cmp }
                                                });

                                                if rows.is_empty() {
                                                    return view! { <div class=EMPTY>"No matching tracks"</div> }.into_any();
                                                }

                                                // Render based on grouping mode
                                                match grouping {
                                                    GroupBy::Artist | GroupBy::Album => {
                                                        render_grouped_tracks(rows, grouping, &collapsed, set_collapsed_groups)
                                                    }
                                                    GroupBy::None => {
                                                        render_flat_tracks(rows)
                                                    }
                                                }
                                            })
                                        }}
                                    </PanelBody>
                                </Panel>

                                // ── Provider search results ─────────────
                                <Suspense>
                                    {move || {
                                        if query.get().trim().is_empty() {
                                            return Some(view! { <span></span> }.into_any());
                                        }
                                        search_result.get().map(|res| match res {
                                            Err(e) => view! { <div class="text-sm text-red-500">{e.to_string()}</div> }.into_any(),
                                            Ok(sr) => {
                                                if sr.results.is_empty() {
                                                    return view! { <span></span> }.into_any();
                                                }
                                                let results = StoredValue::new(sr.results);
                                                view! {
                                                    <Panel>
                                                        <PanelHeader>
                                                            <PanelTitle>"Add Tracks From Providers"</PanelTitle>
                                                        </PanelHeader>
                                                        <PanelBody class="p-0!">
                                                            <div class="divide-y divide-black/[.04] dark:divide-white/[.04]">
                                                                <For
                                                                    each=move || results.with_value(|results| results.clone())
                                                                    key=|result| provider_result_key(&result.provider, &result.external_id)
                                                                    let:result
                                                                >
                                                                    <ProviderTrackSearchRow result=result />
                                                                </For>
                                                            </div>
                                                        </PanelBody>
                                                    </Panel>
                                                }.into_any()
                                            }
                                        })
                                    }}
                                </Suspense>
                            </div>
                        }.into_any()
                    }
                })
            }}
        </Transition>
    }
}

#[component]
fn ProviderTrackSearchRow(result: SearchTrackResult) -> impl IntoView {
    let loading = RwSignal::new(false);
    let provider = result.provider.clone();
    let external_track_id = result.external_id.clone();
    let external_album_id = result.album_external_id.clone();
    let artist_external_id = result.artist_external_id.clone();
    let artist_name = result.artist_name.clone();
    let cover_url = result.album_cover_url.clone();
    let fallback = crate::components::fallback_initial(&result.album_title);

    view! {
        <div class="px-5 max-md:px-3.5 py-3 flex items-center gap-3">
            <div class=COVER_THUMB>
                {match cover_url {
                    Some(url) => view! {
                        <img class="w-full h-full object-cover" src=url alt="" loading="lazy" />
                    }.into_any(),
                    None => view! {
                        <div class=COVER_FALLBACK>{fallback}</div>
                    }.into_any(),
                }}
            </div>
            <div class="flex-1 min-w-0">
                <div class="text-sm font-medium text-zinc-900 dark:text-zinc-100 truncate">{result.title.clone()}</div>
                <div class={cls!(MUTED, "text-xs truncate")}>{format!("{} · {}", result.artist_name.clone(), result.album_title.clone())}</div>
            </div>
            <span class={cls!(MUTED, "text-xs tabular-nums shrink-0")}>{result.duration_display.clone()}</span>
            <Button variant=ButtonVariant::Primary class="py-1" loading=loading
                on:click=move |_| {
                    dispatch_with_toast_loading(
                        ServerAction::AddTrack {
                            provider: provider.clone(),
                            external_track_id: external_track_id.clone(),
                            external_album_id: external_album_id.clone(),
                            artist_external_id: artist_external_id.clone(),
                            artist_name: artist_name.clone(),
                        },
                        "Track added",
                        Some(loading),
                    );
                }
            >
                "Add"
            </Button>
        </div>
    }
}

// ── Flat table rendering ────────────────────────────────────

fn render_flat_tracks(rows: Vec<LibraryTrack>) -> leptos::prelude::AnyView {
    rows.into_iter()
        .map(render_track_row)
        .collect_view()
        .into_any()
}

// ── Grouped rendering ───────────────────────────────────────

fn render_grouped_tracks(
    rows: Vec<LibraryTrack>,
    grouping: GroupBy,
    collapsed: &std::collections::HashSet<String>,
    set_collapsed: WriteSignal<std::collections::HashSet<String>>,
) -> leptos::prelude::AnyView {
    // Build ordered groups
    let mut groups: Vec<(String, Vec<LibraryTrack>)> = Vec::new();
    let mut group_map: std::collections::HashMap<String, usize> = std::collections::HashMap::new();

    for row in rows {
        let key = match grouping {
            GroupBy::Artist => row.artist_name.clone(),
            GroupBy::Album => format!("{} \u{2014} {}", row.artist_name, row.album_title),
            GroupBy::None => String::new(),
        };
        if let Some(&idx) = group_map.get(&key) {
            groups[idx].1.push(row);
        } else {
            let idx = groups.len();
            group_map.insert(key.clone(), idx);
            groups.push((key, vec![row]));
        }
    }

    // Clone collapsed set so we own it (avoid lifetime issues in view closures)
    let collapsed = collapsed.clone();

    groups
        .into_iter()
        .map(|(group_name, group_tracks)| {
            let count = group_tracks.len();
            let is_collapsed = collapsed.contains(&group_name);
            let group_name_for_click = group_name.clone();
            let collapsed_snapshot = collapsed.clone();

            view! {
                <div>
                    <div
                        class=GROUP_HEADER
                        on:click={
                            let name = group_name_for_click.clone();
                            let snapshot = collapsed_snapshot.clone();
                            move |_| {
                                let mut current = snapshot.clone();
                                if current.contains(&name) {
                                    current.remove(&name);
                                } else {
                                    current.insert(name.clone());
                                }
                                set_collapsed.set(current);
                            }
                        }
                    >
                        <span class="text-zinc-400 dark:text-zinc-500 [&_svg]:size-4 shrink-0 transition-transform duration-150">
                            {if is_collapsed {
                                view! { <ChevronRight size=16 /> }.into_any()
                            } else {
                                view! { <ChevronDown size=16 /> }.into_any()
                            }}
                        </span>
                        <span class="text-sm font-semibold text-zinc-800 dark:text-zinc-200 truncate">{group_name}</span>
                        <span class={cls!(MUTED, "text-[10px] tabular-nums shrink-0")}>{format!("{count} tracks")}</span>
                    </div>
                    {if !is_collapsed {
                        group_tracks
                            .into_iter()
                            .map(render_track_row)
                            .collect_view()
                            .into_any()
                    } else {
                        view! { <span></span> }.into_any()
                    }}
                </div>
            }
        })
        .collect_view()
        .into_any()
}

// ── Individual track row ────────────────────────────────────

fn render_track_row(row: LibraryTrack) -> impl IntoView {
    let t = row.track;
    let href = format!("/artists/{}/albums/{}", row.artist_id, row.album_id);
    let artist_href = format!("/artists/{}", row.artist_id);
    let wanted = t.monitored && !t.acquired;
    let _idle = !t.monitored && !t.acquired;
    let fallback = crate::components::fallback_initial(&row.album_title);

    let status_variant = if t.acquired {
        BadgeVariant::Success
    } else if wanted {
        BadgeVariant::Warning
    } else {
        BadgeVariant::Neutral
    };
    let status_text = if t.acquired {
        "Acquired"
    } else if wanted {
        "Wanted"
    } else {
        "Idle"
    };

    // Explicit badge
    let explicit = t.explicit;

    view! {
        // ── Desktop row (md+): grid layout matching header ──
        <div class={cls!(
            "max-md:hidden grid gap-3 px-5 py-2 items-center transition-colors duration-100 hover:bg-blue-500/[.03] dark:hover:bg-blue-500/[.05] border-b border-black/[.02] dark:border-white/[.02] last:border-b-0",
            DESKTOP_TRACK_GRID,
        )}>
            // Cover thumbnail
            <div class=COVER_THUMB>
                {match &row.album_cover_url {
                    Some(url) => view! {
                        <img class="w-full h-full object-cover" src=url.clone() alt="" loading="lazy" />
                    }.into_any(),
                    None => view! {
                        <div class=COVER_FALLBACK>{fallback.clone()}</div>
                    }.into_any(),
                }}
            </div>
            // Title
            <div class="min-w-0 flex items-center gap-1.5">
                <span class="text-sm text-zinc-900 dark:text-zinc-100 truncate">{t.title.clone()}</span>
                {match &t.version {
                    Some(v) if !v.is_empty() => view! {
                        <span class="text-[11px] text-zinc-400 dark:text-zinc-500 shrink-0">{format!("({v})")}</span>
                    }.into_any(),
                    _ => view! { <span></span> }.into_any(),
                }}
                {if explicit {
                    view! {
                        <Badge
                            variant=BadgeVariant::Explicit
                            class="shrink-0 justify-center gap-0 px-1 text-[9px] font-bold leading-none tracking-wide uppercase"
                        >
                            "E"
                        </Badge>
                    }.into_any()
                } else {
                    view! { <span></span> }.into_any()
                }}
            </div>
            // Artist
            <a href=artist_href.clone() class="text-xs text-zinc-500 dark:text-zinc-400 truncate no-underline hover:text-blue-500 dark:hover:text-blue-400 transition-colors duration-150">
                {row.artist_name.clone()}
            </a>
            // Album
            <a href=href.clone() class="text-xs text-zinc-500 dark:text-zinc-400 truncate no-underline hover:text-blue-500 dark:hover:text-blue-400 transition-colors duration-150">
                {row.album_title.clone()}
            </a>
            // Duration
            <span class={cls!(MUTED, "text-xs tabular-nums")}>{t.duration_display.clone()}</span>
            // Status
            <Badge variant=status_variant>{status_text}</Badge>
        </div>

        // ── Mobile row (<md): compact card layout ───────────
        <div class="md:hidden flex items-center gap-3 px-3.5 py-2.5 transition-colors duration-100 hover:bg-blue-500/[.03] dark:hover:bg-blue-500/[.05] border-b border-black/[.02] dark:border-white/[.02] last:border-b-0">
            // Cover thumbnail
            <a href=href.clone() class="contents">
                <div class=COVER_THUMB>
                    {match &row.album_cover_url {
                        Some(url) => view! {
                            <img class="w-full h-full object-cover" src=url.clone() alt="" loading="lazy" />
                        }.into_any(),
                        None => view! {
                            <div class=COVER_FALLBACK>{fallback}</div>
                        }.into_any(),
                    }}
                </div>
            </a>
            <div class="flex-1 min-w-0">
                <div class="flex items-center gap-1.5">
                    <span class="text-sm text-zinc-900 dark:text-zinc-100 truncate">{t.title}</span>
                    {if explicit {
                        view! {
                            <Badge
                                variant=BadgeVariant::Explicit
                                class="shrink-0 justify-center gap-0 px-1 text-[9px] font-bold leading-none tracking-wide uppercase"
                            >
                                "E"
                            </Badge>
                        }.into_any()
                    } else {
                        view! { <span></span> }.into_any()
                    }}
                </div>
                <div class={cls!(MUTED, "text-xs truncate")}>{format!("{} \u{00b7} {}", row.artist_name, row.album_title)}</div>
            </div>
            <div class="shrink-0 flex flex-col items-end gap-1">
                <Badge variant=status_variant>{status_text}</Badge>
                <span class={cls!(MUTED, "text-[10px] tabular-nums")}>{t.duration_display}</span>
            </div>
        </div>
    }
}

// ── Page wrapper ────────────────────────────────────────────

#[component]
pub fn LibraryTracksPage() -> impl IntoView {
    set_page_title("Tracks");
    view! {
        <PageShell active="library-tracks">
                <Breadcrumb items=vec![
                    BreadcrumbItem::link("Library", "/library/artists"),
                    BreadcrumbItem::current("Tracks"),
                ] />
                <LibraryTracksTab />
        </PageShell>
    }
}
