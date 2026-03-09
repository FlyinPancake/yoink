use crate::cls;
use std::collections::HashSet;

use leptos::prelude::*;
use lucide_leptos::X;

use yoink_shared::{
    MonitoredAlbum, MonitoredArtist, SearchArtistResult, ServerAction, build_albums_by_artist,
    provider_display_name,
};

use leptoaster::{ToastBuilder, ToastLevel, ToastPosition, expect_toaster};

use crate::actions::dispatch_action;
use crate::components::toast::dispatch_with_toast;
use crate::components::{
    Badge, BadgeSize, BadgeSurface, BadgeVariant, Breadcrumb, BreadcrumbItem, Button,
    ButtonVariant, ErrorPanel, PageShell, Panel, PanelBody, PanelHeader, PanelTitle,
    fallback_initial,
};
use crate::hooks::{set_page_title, use_sse_version};
use crate::search_result_keys::provider_result_key;
use crate::styles::{EMPTY, GLASS, GLASS_BODY, SEARCH_INPUT, SELECT};

// ── Page-specific Tailwind class constants ──────────────────

const SEARCH_RESULT: &str = "flex items-center gap-3.5 px-4 py-3 border-b border-black/[.04] dark:border-white/[.04] transition-[background] duration-[120ms] last:border-b-0 hover:bg-blue-500/[.04] dark:hover:bg-blue-500/[.06]";
const ARTIST_CARD: &str = "bg-white/70 dark:bg-zinc-800/60 backdrop-blur-[12px] border border-black/[.06] dark:border-white/[.08] rounded-xl p-4 flex items-center gap-3.5 transition-[transform,box-shadow,border-color] duration-200 relative overflow-hidden no-underline cursor-pointer hover:-translate-y-0.5 hover:shadow-[0_8px_32px_rgba(59,130,246,.1)] hover:border-blue-500/20 dark:hover:shadow-[0_8px_32px_rgba(59,130,246,.15)] dark:hover:border-blue-500/30";
const ARTIST_AVATAR: &str = "size-12 rounded-full object-cover border-2 border-blue-500/20 dark:border-blue-500/30 shrink-0 bg-zinc-200 dark:bg-zinc-800";
const ARTIST_FALLBACK: &str = "size-12 rounded-full inline-flex items-center justify-center bg-zinc-200 dark:bg-zinc-800 text-zinc-500 dark:text-zinc-400 font-bold text-lg border-2 border-blue-500/20 dark:border-blue-500/30 shrink-0";

// ── DTOs ────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ArtistsData {
    pub monitored: Vec<MonitoredArtist>,
    pub albums: Vec<MonitoredAlbum>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SearchResult {
    pub results: Vec<SearchArtistResult>,
    pub error: Option<String>,
}

// ── Server functions ────────────────────────────────────────

#[server(GetArtistsData, "/leptos")]
pub async fn get_artists_data() -> Result<ArtistsData, ServerFnError> {
    let ctx = use_context::<yoink_shared::ServerContext>()
        .ok_or_else(|| ServerFnError::new("ServerContext not available"))?;

    let monitored = ctx.monitored_artists.read().await.clone();
    let albums = ctx.monitored_albums.read().await.clone();

    Ok(ArtistsData { monitored, albums })
}

#[server(SearchArtists, "/leptos")]
pub async fn search_artists(query: String) -> Result<SearchResult, ServerFnError> {
    let ctx = use_context::<yoink_shared::ServerContext>()
        .ok_or_else(|| ServerFnError::new("ServerContext not available"))?;

    let trimmed = query.trim().to_string();
    if trimmed.is_empty() {
        return Ok(SearchResult {
            results: vec![],
            error: None,
        });
    }

    match (ctx.search_artists)(trimmed).await {
        Ok(results) => Ok(SearchResult {
            results,
            error: None,
        }),
        Err(err) => Ok(SearchResult {
            results: vec![],
            error: Some(err.to_string()),
        }),
    }
}

// ── Page component ──────────────────────────────────────────

#[component]
pub fn ArtistsPage() -> impl IntoView {
    set_page_title("Artists");
    let version = use_sse_version();
    let data = Resource::new(move || version.get(), |_| get_artists_data());

    // Read ?q= from URL query params (works with SPA navigation)
    let url_query = leptos_router::hooks::use_query_map();
    let initial_q = url_query.read_untracked().get("q").unwrap_or_default();

    // Search state with debounce
    let (query, set_query) = signal(initial_q.clone());
    let (debounced_query, set_debounced_query) = signal(initial_q);

    // Debounce: wait 300ms after last keystroke before updating debounced_query
    #[cfg(feature = "hydrate")]
    {
        use std::cell::Cell;
        use std::rc::Rc;
        use wasm_bindgen::JsCast;
        let timer_id: Rc<Cell<Option<i32>>> = Rc::new(Cell::new(None));
        Effect::new({
            let timer_id = timer_id.clone();
            move |_| {
                let val = query.get();
                // Clear previous timer
                if let Some(id) = timer_id.get() {
                    leptos::prelude::window().clear_timeout_with_handle(id);
                }
                // Set new timer
                let set_dq = set_debounced_query;
                let timer_id_inner = timer_id.clone();
                let cb = wasm_bindgen::closure::Closure::once_into_js(move || {
                    set_dq.set(val);
                    timer_id_inner.set(None);
                });
                if let Ok(id) = leptos::prelude::window()
                    .set_timeout_with_callback_and_timeout_and_arguments_0(
                        cb.as_ref().unchecked_ref(),
                        300,
                    )
                {
                    timer_id.set(Some(id));
                }
            }
        });
    }
    // On SSR, just mirror query directly (no debounce needed)
    #[cfg(not(feature = "hydrate"))]
    {
        Effect::new(move |_| {
            set_debounced_query.set(query.get());
        });
    }

    let search_result: Resource<Result<SearchResult, ServerFnError>> = Resource::new(
        move || debounced_query.get(),
        |q| async move {
            if q.trim().is_empty() {
                Ok(SearchResult {
                    results: vec![],
                    error: None,
                })
            } else {
                search_artists(q).await
            }
        },
    );

    view! {
        <PageShell active="library-artists">
                <Transition fallback=move || view! {
                    <div>
                        <Breadcrumb items=vec![
                            BreadcrumbItem::link("Library", "/library"),
                            BreadcrumbItem::current("Artists"),
                        ] />
                        <div class="p-6 max-md:p-4">
                            // Skeleton search bar
                            <div class="mb-5 bg-white/70 dark:bg-zinc-800/60 rounded-xl border border-black/[.06] dark:border-white/[.08] p-4">
                                <div class="h-9 w-full max-w-[360px] bg-zinc-200 dark:bg-zinc-700 rounded-lg animate-pulse"></div>
                            </div>
                            // Skeleton artist card grid
                            <div class="bg-white/70 dark:bg-zinc-800/60 rounded-xl border border-black/[.06] dark:border-white/[.08] overflow-hidden">
                                <div class="px-5 py-3 border-b border-black/[.06] dark:border-white/[.06]">
                                    <div class="h-4 w-28 bg-zinc-200 dark:bg-zinc-700 rounded animate-pulse"></div>
                                </div>
                                <div class="p-4 grid grid-cols-[repeat(auto-fill,minmax(240px,1fr))] gap-4">
                                    {(0..8).map(|_| view! {
                                        <div class="rounded-xl p-4 flex items-center gap-3.5 border border-black/[.04] dark:border-white/[.04] animate-pulse">
                                            <div class="size-12 rounded-full bg-zinc-200 dark:bg-zinc-700 shrink-0"></div>
                                            <div class="flex-1 min-w-0">
                                                <div class="h-4 w-28 bg-zinc-200 dark:bg-zinc-700 rounded mb-2"></div>
                                                <div class="h-3 w-40 bg-zinc-200 dark:bg-zinc-700 rounded"></div>
                                            </div>
                                        </div>
                                    }).collect_view()}
                                </div>
                            </div>
                        </div>
                    </div>
                }>
                    {move || {
                        data.get().map(|result| match result {
                            Err(e) => view! {
                                <div class="p-6 max-md:p-4">
                                    <ErrorPanel
                                        message="Failed to load artists."
                                        details=e.to_string()
                                        retry_href="/library"
                                    />
                                </div>
                            }.into_any(),
                            Ok(data) => {
                                view! {
                                    <ArtistsContent
                                        data=data
                                        query=query
                                        set_query=set_query
                                        search_result=search_result
                                        show_header=true
                                    />
                                }.into_any()
                            }
                        })
                    }}
                </Transition>
        </PageShell>
    }
}

#[component]
pub fn ArtistsTabContent(show_header: bool) -> impl IntoView {
    let version = use_sse_version();
    let data = Resource::new(move || version.get(), |_| get_artists_data());

    let url_query = leptos_router::hooks::use_query_map();
    let initial_q = url_query.read_untracked().get("q").unwrap_or_default();

    let (query, set_query) = signal(initial_q.clone());
    let (debounced_query, set_debounced_query) = signal(initial_q);

    #[cfg(feature = "hydrate")]
    {
        use std::cell::Cell;
        use std::rc::Rc;
        use wasm_bindgen::JsCast;
        let timer_id: Rc<Cell<Option<i32>>> = Rc::new(Cell::new(None));
        Effect::new({
            let timer_id = timer_id.clone();
            move |_| {
                let val = query.get();
                if let Some(id) = timer_id.get() {
                    leptos::prelude::window().clear_timeout_with_handle(id);
                }
                let set_dq = set_debounced_query;
                let timer_id_inner = timer_id.clone();
                let cb = wasm_bindgen::closure::Closure::once_into_js(move || {
                    set_dq.set(val);
                    timer_id_inner.set(None);
                });
                if let Ok(id) = leptos::prelude::window()
                    .set_timeout_with_callback_and_timeout_and_arguments_0(
                        cb.as_ref().unchecked_ref(),
                        300,
                    )
                {
                    timer_id.set(Some(id));
                }
            }
        });
    }

    #[cfg(not(feature = "hydrate"))]
    {
        Effect::new(move |_| {
            set_debounced_query.set(query.get());
        });
    }

    let search_result: Resource<Result<SearchResult, ServerFnError>> = Resource::new(
        move || debounced_query.get(),
        |q| async move {
            if q.trim().is_empty() {
                Ok(SearchResult {
                    results: vec![],
                    error: None,
                })
            } else {
                search_artists(q).await
            }
        },
    );

    view! {
        <Transition fallback=move || view! {
            <div class="p-6 max-md:p-4">
                <div class="mb-5 bg-white/70 dark:bg-zinc-800/60 rounded-xl border border-black/[.06] dark:border-white/[.08] p-4">
                    <div class="h-9 w-full max-w-[360px] bg-zinc-200 dark:bg-zinc-700 rounded-lg animate-pulse"></div>
                </div>
                <div class="bg-white/70 dark:bg-zinc-800/60 rounded-xl border border-black/[.06] dark:border-white/[.08] overflow-hidden">
                    <div class="px-5 py-3 border-b border-black/[.06] dark:border-white/[.06]">
                        <div class="h-4 w-28 bg-zinc-200 dark:bg-zinc-700 rounded animate-pulse"></div>
                    </div>
                    <div class="p-4 grid grid-cols-[repeat(auto-fill,minmax(240px,1fr))] gap-4">
                        {(0..8).map(|_| view! {
                            <div class="rounded-xl p-4 flex items-center gap-3.5 border border-black/[.04] dark:border-white/[.04] animate-pulse">
                                <div class="size-12 rounded-full bg-zinc-200 dark:bg-zinc-700 shrink-0"></div>
                                <div class="flex-1 min-w-0">
                                    <div class="h-4 w-28 bg-zinc-200 dark:bg-zinc-700 rounded mb-2"></div>
                                    <div class="h-3 w-40 bg-zinc-200 dark:bg-zinc-700 rounded"></div>
                                </div>
                            </div>
                        }).collect_view()}
                    </div>
                </div>
            </div>
        }>
            {move || {
                data.get().map(|result| match result {
                    Err(e) => view! {
                        <div class="p-6 max-md:p-4">
                            <ErrorPanel
                                message="Failed to load artists."
                                details=e.to_string()
                                retry_href="/library"
                            />
                        </div>
                    }.into_any(),
                    Ok(data) => {
                        view! {
                            <ArtistsContent
                                data=data
                                query=query
                                set_query=set_query
                                search_result=search_result
                                show_header=show_header
                            />
                        }.into_any()
                    }
                })
            }}
        </Transition>
    }
}

#[component]
fn ArtistsContent(
    data: ArtistsData,
    query: ReadSignal<String>,
    set_query: WriteSignal<String>,
    search_result: Resource<Result<SearchResult, ServerFnError>>,
    show_header: bool,
) -> impl IntoView {
    let monitored_count = data.monitored.len();
    let monitored_names: HashSet<String> = data
        .monitored
        .iter()
        .map(|a| a.name.to_lowercase())
        .collect();
    let albums_by_artist = build_albums_by_artist(data.albums);

    // Client-side sort for the collection grid (filter reuses the search query)
    let (collection_sort, set_collection_sort) = signal("az".to_string());

    // Precompute artist data for filtering/sorting
    let artists_with_albums: Vec<(MonitoredArtist, Vec<MonitoredAlbum>)> = data
        .monitored
        .into_iter()
        .map(|artist| {
            let artist_albums = albums_by_artist
                .get(&artist.id)
                .cloned()
                .unwrap_or_default();
            (artist, artist_albums)
        })
        .collect();
    let artists_with_albums = StoredValue::new(artists_with_albums);

    view! {
        <Show when=move || show_header>
            <Breadcrumb items=vec![
                BreadcrumbItem::link("Library", "/library"),
                BreadcrumbItem::current("Artists"),
            ] />
        </Show>

        <div class="p-6 max-md:p-4">
            // Search panel with clear button (#13)
            <div class={cls!(GLASS, "mb-5 !overflow-visible relative z-50")}>
                <div class={cls!(GLASS_BODY, "!overflow-visible")}>
                    <div class="flex flex-wrap items-center gap-2">
                        <div class="relative w-full max-w-[360px]">
                            <input
                                type="text"
                                class={cls!(SEARCH_INPUT, "max-w-full pr-8")}
                                placeholder="Search artist name..."
                                autocomplete="off"
                                aria-label="Search artists"
                                prop:value=move || query.get()
                                on:input=move |ev| {
                                    let val = event_target_value(&ev);
                                    set_query.set(val);
                                }
                            />
                            <Show when=move || !query.get().is_empty()>
                                <button type="button"
                                    class="absolute right-2 top-1/2 -translate-y-1/2 inline-flex items-center justify-center size-5 rounded-md text-zinc-400 hover:text-zinc-600 dark:hover:text-zinc-200 cursor-pointer bg-transparent border-none p-0 [&_svg]:size-3.5"
                                    on:click=move |_| set_query.set(String::new())
                                    title="Clear search"
                                    aria-label="Clear search"
                                >
                                    <X />
                                </button>
                            </Show>
                        </div>
                    </div>
                </div>
            </div>

            // Monitored artists collection (shown first, filtered by search query)
            <Panel>
                <PanelHeader>
                    <PanelTitle>"Your Collection"</PanelTitle>
                    {if monitored_count > 0 {
                        view! {
                            <select
                                class=SELECT
                                aria-label="Sort collection"
                                on:change=move |ev| {
                                    set_collection_sort.set(event_target_value(&ev));
                                }
                            >
                                <option value="az" selected=true>"A \u{2013} Z"</option>
                                <option value="za">"Z \u{2013} A"</option>
                                <option value="recent">"Recently Added"</option>
                                <option value="wanted">"Most Wanted"</option>
                            </select>
                        }.into_any()
                    } else {
                        view! { <span></span> }.into_any()
                    }}
                </PanelHeader>
                {if monitored_count == 0 {
                    view! { <div class=EMPTY>"No monitored artists yet. Search and add one above."</div> }.into_any()
                } else {
                    view! {
                        <PanelBody class="p-4">
                            <div class="grid grid-cols-[repeat(auto-fill,minmax(240px,1fr))] gap-4">
                                {move || {
                                    let filter = query.get().trim().to_lowercase();
                                    let sort_key = collection_sort.get();
                                    artists_with_albums.with_value(|all| {
                                        let mut filtered: Vec<_> = all.iter()
                                            .filter(|(artist, _)| {
                                                filter.is_empty() || artist.name.to_lowercase().contains(&filter)
                                            })
                                            .collect();

                                        // Sort based on selected option
                                        match sort_key.as_str() {
                                            "za" => filtered.sort_by(|(a, _), (b, _)| b.name.to_lowercase().cmp(&a.name.to_lowercase())),
                                            "recent" => filtered.sort_by(|(a, _), (b, _)| b.added_at.cmp(&a.added_at)),
                                            "wanted" => filtered.sort_by(|(_, aa), (_, ba)| {
                                                let aw = aa.iter().filter(|a| a.wanted).count();
                                                let bw = ba.iter().filter(|a| a.wanted).count();
                                                bw.cmp(&aw)
                                            }),
                                            _ /* "az" */ => filtered.sort_by(|(a, _), (b, _)| a.name.to_lowercase().cmp(&b.name.to_lowercase())),
                                        }

                                        if filtered.is_empty() {
                                            view! {
                                                <div class="col-span-full text-center py-6 text-sm text-zinc-400 dark:text-zinc-600">
                                                    "No matching artists"
                                                </div>
                                            }.into_any()
                                        } else {
                                            filtered.into_iter().map(|(artist, albums)| {
                                                view! { <ArtistCard artist=artist.clone() albums=albums.clone() /> }
                                            }).collect_view().into_any()
                                        }
                                    })
                                }}
                            </div>
                        </PanelBody>
                    }.into_any()
                }}
            </Panel>

            // Search results — only non-monitored artists, shown below collection
            <Suspense fallback=move || view! {
                <Show when=move || !query.get().trim().is_empty()>
                    <div class="flex items-center gap-2 px-4 py-3 mb-5 text-sm text-zinc-500 dark:text-zinc-400">
                        <span class="inline-block size-4 border-2 border-zinc-300 dark:border-zinc-600 border-t-blue-500 dark:border-t-blue-400 rounded-full animate-spin"></span>
                        "Searching\u{2026}"
                    </div>
                </Show>
            }>
                {move || {
                    let current_query = query.get();
                    search_result.get().map(|result| {
                        match result {
                            Err(e) => view! {
                                <ErrorPanel
                                    message="Search failed. Please try again."
                                    details=e.to_string()
                                />
                            }.into_any(),
                            Ok(sr) => {
                                let has_error = sr.error.is_some();
                                let error_view = sr.error.map(|msg| view! {
                                    <div class="px-4 py-3 mb-6 rounded-[10px] text-[13px] border border-red-500/30 bg-red-500/[.08] text-red-600">
                                        {msg}
                                    </div>
                                });

                                let has_query = !current_query.trim().is_empty();
                                // Filter out artists already in the collection
                                let names = monitored_names.clone();
                                let new_results: Vec<_> = sr.results.into_iter()
                                    .filter(|a| !names.contains(&a.name.to_lowercase()))
                                    .collect();

                                let results_view = if !new_results.is_empty() {
                                    Some(view! {
                                        <Panel>
                                            <PanelHeader>
                                                <PanelTitle>"Add from Search"</PanelTitle>
                                            </PanelHeader>
                                            <div>
                                                <For
                                                    each=move || new_results.clone()
                                                    key=|artist| provider_result_key(&artist.provider, &artist.external_id)
                                                    let:artist
                                                >
                                                    <SearchResultRow artist=artist set_query=set_query />
                                                </For>
                                            </div>
                                        </Panel>
                                    }.into_any())
                                } else if has_query && !has_error {
                                    Some(view! {
                                        <div class=EMPTY>
                                            {format!("No new artists found for \u{201c}{current_query}\u{201d}")}
                                        </div>
                                    }.into_any())
                                } else {
                                    None
                                };

                                view! {
                                    <div>
                                        {error_view}
                                        {results_view}
                                    </div>
                                }.into_any()
                            }
                        }
                    })
                }}
            </Suspense>
        </div>
    }
}

/// A single search result row with an "Add" button.
#[component]
fn SearchResultRow(artist: SearchArtistResult, set_query: WriteSignal<String>) -> impl IntoView {
    let navigate = leptos_router::hooks::use_navigate();
    let image_url = artist.image_url.clone();
    let profile_url = artist.url.clone().unwrap_or_default();
    let fallback_initial = fallback_initial(&artist.name);
    let artist_external_id = artist.external_id.clone();
    let artist_name = artist.name.clone();
    let artist_image_url = artist.image_url.clone();
    let artist_url = artist.url.clone();
    let artist_provider = artist.provider.clone();
    let provider_display = provider_display_name(&artist.provider);
    let has_profile = !profile_url.is_empty();

    // Build the metadata subtitle fragments.
    let disambiguation = artist.disambiguation.clone();
    let artist_type = artist.artist_type.clone();
    let country = artist.country.clone();
    let tags = artist.tags.clone();
    let popularity = artist.popularity;

    // "Group from United Kingdom" or "Person" or "Group" etc.
    let type_country: Option<String> = match (&artist_type, &country) {
        (Some(t), Some(c)) => Some(format!("{t} from {c}")),
        (Some(t), None) => Some(t.clone()),
        (None, Some(c)) => Some(format!("from {c}")),
        (None, None) => None,
    };

    // Build subtitle: disambiguation or type/country, with popularity appended if available.
    let subtitle: Option<String> = {
        let base = disambiguation.or(type_country);
        match (base, popularity) {
            (Some(b), Some(p)) => Some(format!("{b} \u{00b7} {p}% popularity")),
            (Some(b), None) => Some(b),
            (None, Some(p)) => Some(format!("{p}% popularity")),
            (None, None) => None,
        }
    };

    let adding = RwSignal::new(false);

    view! {
        <div class=SEARCH_RESULT>
            {match image_url {
                Some(url) => view! {
                    <img class=ARTIST_AVATAR src=url alt="" />
                }.into_any(),
                None => view! {
                    <div class=ARTIST_FALLBACK>{fallback_initial}</div>
                }.into_any(),
            }}
            <div class="flex-1 min-w-0">
                <div class="flex items-center gap-2 flex-wrap">
                    <span class="text-[15px] font-semibold text-zinc-900 dark:text-zinc-100">{artist.name}</span>
                    {if has_profile {
                        let profile_url_clone = profile_url.clone();
                        let display = provider_display.clone();
                        view! {
                            <Badge
                                variant=BadgeVariant::Info
                                surface=BadgeSurface::Outline
                                size=BadgeSize::Sm
                                href=profile_url_clone
                                new_tab=true
                            >
                                {display}
                            </Badge>
                        }.into_any()
                    } else {
                        let display = provider_display.clone();
                        view! {
                            <Badge surface=BadgeSurface::Outline size=BadgeSize::Sm>
                                {display}
                            </Badge>
                        }.into_any()
                    }}
                </div>
                // Subtitle: disambiguation/type/country + popularity
                {subtitle.map(|s| view! {
                    <div class="text-[12px] text-zinc-500 dark:text-zinc-400 mt-0.5 leading-snug">{s}</div>
                })}
                // Tags as small inline pills
                {(!tags.is_empty()).then(|| view! {
                    <div class="flex flex-wrap gap-1 mt-1">
                        {tags.into_iter().map(|tag| view! {
                            <Badge surface=BadgeSurface::Outline>{tag}</Badge>
                        }).collect_view()}
                    </div>
                })}
            </div>
            <Button variant=ButtonVariant::Primary loading=adding
                on:click=move |_| {
                    adding.set(true);
                    let name = artist_name.clone();
                    let provider = artist_provider.clone();
                    let ext_id = artist_external_id.clone();
                    let img_url = artist_image_url.clone();
                    let ext_url = artist_url.clone();
                    let navigate = navigate.clone();
                    let toaster = expect_toaster();
                    leptos::task::spawn_local(async move {
                        match dispatch_action(ServerAction::AddArtist {
                            name: name.clone(),
                            provider,
                            external_id: ext_id,
                            image_url: img_url,
                            external_url: ext_url,
                        }).await {
                            Ok(()) => {
                                toaster.toast(
                                    ToastBuilder::new("Artist added")
                                        .with_level(ToastLevel::Success)
                                        .with_position(ToastPosition::BottomRight)
                                        .with_expiry(Some(4_000)),
                                );
                                set_query.set(String::new());
                                navigate("/library", Default::default());
                            }
                            Err(e) => {
                                toaster.toast(
                                    ToastBuilder::new(format!("Error: {e}"))
                                        .with_level(ToastLevel::Error)
                                        .with_position(ToastPosition::BottomRight)
                                        .with_expiry(Some(8_000)),
                                );
                                adding.set(false);
                            }
                        }
                    });
                }>
                {move || if adding.get() { "Adding\u{2026}" } else { "+ Add" }}
            </Button>
        </div>
    }
}

/// A monitored artist card linking to the detail page.
#[component]
fn ArtistCard(artist: MonitoredArtist, albums: Vec<MonitoredAlbum>) -> impl IntoView {
    let album_count = albums.len();
    let wanted = albums.iter().filter(|a| a.wanted).count();
    let acquired = albums.iter().filter(|a| a.acquired).count();
    let album_count_text =
        format!("{album_count} albums \u{00b7} {acquired} acquired \u{00b7} {wanted} wanted");
    let fallback_initial = fallback_initial(&artist.name);
    let artist_img = artist.image_url.clone();
    let detail_href = format!("/artists/{}", artist.id);
    let is_monitored = artist.monitored;
    let artist_id = artist.id;

    view! {
        <div class=ARTIST_CARD>
            <a href=detail_href class="flex items-center gap-3.5 flex-1 min-w-0 no-underline">
                {match artist_img {
                    Some(url) => view! {
                        <img class=ARTIST_AVATAR src=url alt="" />
                    }.into_any(),
                    None => view! {
                        <div class=ARTIST_FALLBACK>{fallback_initial}</div>
                    }.into_any(),
                }}
                <div class="flex-1 min-w-0">
                    <div class="flex items-center gap-2">
                        <div class="text-[15px] font-bold text-zinc-900 dark:text-zinc-100 whitespace-nowrap overflow-hidden text-ellipsis">{artist.name}</div>
                        {if is_monitored {
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
                    <div class="text-xs text-zinc-500 dark:text-zinc-400">{album_count_text}</div>
                </div>
            </a>
            {if !is_monitored {
                view! {
                    <button
                        type="button"
                        class="inline-flex items-center justify-center px-2 py-1 text-[11px] font-medium rounded-md border border-amber-500/30 bg-amber-500/[.08] text-amber-700 dark:text-amber-300 hover:bg-amber-500/[.14] transition-colors duration-150"
                        on:click=move |ev| {
                            ev.stop_propagation();
                            dispatch_with_toast(
                                ServerAction::ToggleArtistMonitor { artist_id, monitored: true },
                                "Artist promoted to monitored",
                            );
                        }
                    >
                        "Monitor"
                    </button>
                }.into_any()
            } else {
                view! { <span></span> }.into_any()
            }}
        </div>
    }
}
