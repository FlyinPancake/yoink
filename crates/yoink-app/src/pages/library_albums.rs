use crate::cls;
use std::collections::HashMap;

use leptos::prelude::*;

use yoink_shared::{MonitoredAlbum, MonitoredArtist, SearchAlbumResult, ServerAction};

use crate::components::toast::dispatch_with_toast_loading;
use crate::components::{
    AlbumCard, Breadcrumb, BreadcrumbItem, Button, ButtonVariant, PageShell, Panel, PanelBody,
    PanelHeader, PanelTitle, SleeveBadge,
};
use crate::hooks::set_page_title;
use crate::search_result_keys::provider_result_key;
use crate::styles::{EMPTY, MUTED, SEARCH_INPUT, SELECT};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LibraryAlbumsData {
    pub artists: Vec<MonitoredArtist>,
    pub albums: Vec<MonitoredAlbum>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SearchAlbumsResult {
    pub results: Vec<SearchAlbumResult>,
    pub error: Option<String>,
}

#[server(GetLibraryAlbumsData, "/leptos")]
pub async fn get_library_albums_data() -> Result<LibraryAlbumsData, ServerFnError> {
    let ctx = use_context::<yoink_shared::ServerContext>()
        .ok_or_else(|| ServerFnError::new("ServerContext not available"))?;

    let artists = ctx.monitored_artists.read().await.clone();
    let all_albums = ctx.monitored_albums.read().await.clone();

    // Keep only albums that are at least partially monitored:
    // - fully monitored album, OR
    // - has any monitored track (even if already acquired, i.e. not partially_wanted).
    let mut albums = Vec::new();
    for album in all_albums {
        if album.monitored {
            albums.push(album);
            continue;
        }

        let tracks = (ctx.fetch_tracks)(album.id).await.map_err(|e| {
            ServerFnError::new(format!("failed to load tracks for album {}: {e}", album.id))
        })?;
        if tracks.iter().any(|t| t.monitored) {
            albums.push(album);
        }
    }

    Ok(LibraryAlbumsData { artists, albums })
}

#[server(SearchAlbums, "/leptos")]
pub async fn search_albums(query: String) -> Result<SearchAlbumsResult, ServerFnError> {
    let ctx = use_context::<yoink_shared::ServerContext>()
        .ok_or_else(|| ServerFnError::new("ServerContext not available"))?;

    let q = query.trim().to_string();
    if q.is_empty() {
        return Ok(SearchAlbumsResult {
            results: vec![],
            error: None,
        });
    }

    match (ctx.search_albums)(q).await {
        Ok(results) => Ok(SearchAlbumsResult {
            results,
            error: None,
        }),
        Err(err) => Ok(SearchAlbumsResult {
            results: vec![],
            error: Some(err.to_string()),
        }),
    }
}

#[component]
pub fn LibraryAlbumsTab() -> impl IntoView {
    let data = Resource::new(|| (), |_| get_library_albums_data());
    let (query, set_query) = signal(String::new());
    let (sort_key, set_sort_key) = signal("recent".to_string());
    let (filter_key, set_filter_key) = signal("all".to_string());

    let search_result: Resource<Result<SearchAlbumsResult, ServerFnError>> = Resource::new(
        move || query.get(),
        |q| async move { search_albums(q).await },
    );

    view! {
        <Transition fallback=move || view! { <div class="p-6 max-md:p-4"><div class=EMPTY>"Loading albums..."</div></div> }>
            {move || {
                data.get().map(|result| match result {
                    Err(e) => view! { <div class="p-6 text-sm text-red-500">{e.to_string()}</div> }.into_any(),
                    Ok(data) => {
                        let artist_names: HashMap<_, _> = data
                            .artists
                            .into_iter()
                            .map(|a| (a.id, a.name))
                            .collect();
                        let albums = StoredValue::new(data.albums);

                        view! {
                            <div class="p-6 max-md:p-4 space-y-5">
                                <Panel>
                                    <PanelBody>
                                        <div class="flex flex-wrap items-center gap-2">
                                            <input
                                                type="text"
                                                class={cls!(SEARCH_INPUT, "max-w-[360px]")}
                                                placeholder="Search albums (local + provider)..."
                                                prop:value=move || query.get()
                                                on:input=move |ev| set_query.set(event_target_value(&ev))
                                            />
                                            <select class=SELECT on:change=move |ev| set_filter_key.set(event_target_value(&ev))>
                                                <option value="all" selected=true>"All"</option>
                                                <option value="monitored">"Monitored"</option>
                                                <option value="wanted">"Wanted"</option>
                                                <option value="acquired">"Acquired"</option>
                                            </select>
                                            <select class=SELECT on:change=move |ev| set_sort_key.set(event_target_value(&ev))>
                                                <option value="az">"A-Z"</option>
                                                <option value="newest">"Newest"</option>
                                                <option value="oldest">"Oldest"</option>
                                                <option value="recent" selected=true>"Recently Added"</option>
                                                <option value="artist">"By Artist"</option>
                                            </select>
                                        </div>
                                    </PanelBody>
                                </Panel>

                                <Panel>
                                    <PanelHeader>
                                        <PanelTitle>"Albums"</PanelTitle>
                                    </PanelHeader>
                                    <PanelBody class="p-4">
                                        <div class="grid grid-cols-[repeat(auto-fill,minmax(180px,1fr))] gap-5 max-md:grid-cols-[repeat(auto-fill,minmax(140px,1fr))] max-md:gap-3">
                                            {move || {
                                                let q = query.get().trim().to_lowercase();
                                                let filter = filter_key.get();
                                                let sort = sort_key.get();
                                                albums.with_value(|all| {
                                                    let mut items: Vec<_> = all
                                                        .iter()
                                                        .filter(|a| {
                                                            if !q.is_empty() {
                                                                let artist = artist_names.get(&a.artist_id).cloned().unwrap_or_default();
                                                                if !a.title.to_lowercase().contains(&q)
                                                                    && !artist.to_lowercase().contains(&q)
                                                                {
                                                                    return false;
                                                                }
                                                            }
                                                            match filter.as_str() {
                                                                "monitored" => a.monitored,
                                                                "wanted" => a.wanted || a.partially_wanted,
                                                                "acquired" => a.acquired,
                                                                _ => true,
                                                            }
                                                        })
                                                        .collect();

                                                    match sort.as_str() {
                                                        "az" => items.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase())),
                                                        "newest" => items.sort_by(|a, b| b.release_date.cmp(&a.release_date)),
                                                        "oldest" => items.sort_by(|a, b| a.release_date.cmp(&b.release_date)),
                                                        "artist" => items.sort_by(|a, b| {
                                                            let an = artist_names.get(&a.artist_id).cloned().unwrap_or_default();
                                                            let bn = artist_names.get(&b.artist_id).cloned().unwrap_or_default();
                                                            an.to_lowercase().cmp(&bn.to_lowercase())
                                                        }),
                                                        _ => items.sort_by(|a, b| b.added_at.cmp(&a.added_at)),
                                                    }

                                                    if items.is_empty() {
                                                        return view! { <div class="col-span-full"><div class=EMPTY>"No matching albums"</div></div> }.into_any();
                                                    }

                                                    items
                                                        .into_iter()
                                                        .map(|album| {
                                                            let artist_name = artist_names
                                                                .get(&album.artist_id)
                                                                .cloned()
                                                                .unwrap_or_else(|| "Unknown Artist".to_string());
                                                            let href = format!("/artists/{}/albums/{}", album.artist_id, album.id);
                                                            let badge = if album.acquired {
                                                                SleeveBadge::Acquired
                                                            } else if album.wanted || album.partially_wanted {
                                                                SleeveBadge::Wanted
                                                            } else {
                                                                SleeveBadge::None
                                                            };
                                                            let badge_signal = Signal::derive(move || badge);
                                                            let is_explicit = album.explicit;
                                                            view! {
                                                                <AlbumCard
                                                                    album=album.clone()
                                                                    href=href
                                                                    cover_resolution=640
                                                                    sleeve_badge=badge_signal
                                                                    show_explicit=is_explicit
                                                                    subtitle=artist_name
                                                                />
                                                            }
                                                        })
                                                        .collect_view()
                                                        .into_any()
                                                })
                                            }}
                                        </div>
                                    </PanelBody>
                                </Panel>

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
                                                            <PanelTitle>"Add Albums From Providers"</PanelTitle>
                                                        </PanelHeader>
                                                        <PanelBody class="p-0!">
                                                            <div class="divide-y divide-black/[.04] dark:divide-white/[.04]">
                                                                <For
                                                                    each=move || results.with_value(|results| results.clone())
                                                                    key=|result| provider_result_key(&result.provider, &result.external_id)
                                                                    let:result
                                                                >
                                                                    <ProviderAlbumSearchRow result=result />
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
                        }
                        .into_any()
                    }
                })
            }}
        </Transition>
    }
}

#[component]
fn ProviderAlbumSearchRow(result: SearchAlbumResult) -> impl IntoView {
    let loading = RwSignal::new(false);
    let provider = result.provider.clone();
    let external_album_id = result.external_id.clone();
    let artist_external_id = result.artist_external_id.clone();
    let artist_name = result.artist_name.clone();

    view! {
        <div class="px-4 py-3 flex items-center gap-3">
            <div class="flex-1 min-w-0">
                <div class="text-sm font-medium text-zinc-900 dark:text-zinc-100 truncate">{result.title.clone()}</div>
                <div class={cls!(MUTED, "text-xs truncate")}>{format!("{} · {}", result.artist_name.clone(), result.provider.clone())}</div>
            </div>
            <Button variant=ButtonVariant::Primary class="py-1" loading=loading
                on:click=move |_| {
                    dispatch_with_toast_loading(
                        ServerAction::AddAlbum {
                            provider: provider.clone(),
                            external_album_id: external_album_id.clone(),
                            artist_external_id: artist_external_id.clone(),
                            artist_name: artist_name.clone(),
                            monitor_all: false,
                        },
                        "Album added",
                        Some(loading),
                    );
                }
            >
                "Add"
            </Button>
        </div>
    }
}

#[component]
pub fn LibraryAlbumsPage() -> impl IntoView {
    set_page_title("Library - Albums");
    view! {
        <PageShell active="library-albums">
                <Breadcrumb items=vec![
                    BreadcrumbItem::link("Library", "/library/artists"),
                    BreadcrumbItem::current("Albums"),
                ] />
                <LibraryAlbumsTab />
        </PageShell>
    }
}
