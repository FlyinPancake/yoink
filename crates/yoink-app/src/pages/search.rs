use crate::cls;
use leptos::prelude::*;
use std::collections::HashSet;

use yoink_shared::{SearchAlbumResult, SearchArtistResult, SearchTrackResult, ServerAction};

use crate::components::toast::dispatch_with_toast_loading;
use crate::components::{
    Badge, BadgeVariant, Breadcrumb, BreadcrumbItem, Button, ButtonVariant, PageShell, Panel,
    PanelBody, PanelHeader, PanelTitle,
};
use crate::hooks::set_page_title;
use crate::search_result_keys::provider_result_key;
use crate::styles::{EMPTY, MUTED, SEARCH_INPUT};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SearchAllResult {
    pub artists: Vec<SearchArtistResult>,
    pub albums: Vec<SearchAlbumResult>,
    pub tracks: Vec<SearchTrackResult>,
}

#[server(SearchAll, "/leptos")]
pub async fn search_all(query: String) -> Result<SearchAllResult, ServerFnError> {
    let ctx = crate::actions::require_ctx()?;

    let q = query.trim().to_string();
    if q.is_empty() {
        return Ok(SearchAllResult {
            artists: vec![],
            albums: vec![],
            tracks: vec![],
        });
    }

    let artists = (ctx.search_artists)(q.clone())
        .await
        .map_err(|e| ServerFnError::new(format!("failed to search artists: {e}")))?;
    let albums = (ctx.search_albums)(q.clone())
        .await
        .map_err(|e| ServerFnError::new(format!("failed to search albums: {e}")))?;
    let tracks = (ctx.search_tracks)(q)
        .await
        .map_err(|e| ServerFnError::new(format!("failed to search tracks: {e}")))?;

    Ok(SearchAllResult {
        artists,
        albums,
        tracks,
    })
}

#[component]
pub fn SearchPage() -> impl IntoView {
    set_page_title("Search");

    let (query, set_query) = signal(String::new());
    let (dedupe, set_dedupe) = signal(true);
    let result = Resource::new(move || query.get(), |q| async move { search_all(q).await });

    view! {
        <PageShell active="search">
                <Breadcrumb items=vec![
                    BreadcrumbItem::current("Search"),
                ] />

                <div class="p-6 max-md:p-4 space-y-5">
                    <Panel>
                        <PanelBody>
                            <div class="flex flex-wrap items-center gap-3">
                                <input
                                    type="text"
                                    class={cls!(SEARCH_INPUT, "max-w-[420px]")}
                                    placeholder="Search artists, albums, and tracks..."
                                    prop:value=move || query.get()
                                    on:input=move |ev| set_query.set(event_target_value(&ev))
                                />
                                <label class="inline-flex items-center gap-1.5 text-xs text-zinc-600 dark:text-zinc-300 select-none cursor-pointer">
                                    <input
                                        type="checkbox"
                                        prop:checked=move || dedupe.get()
                                        on:change=move |ev| set_dedupe.set(event_target_checked(&ev))
                                    />
                                    "Deduplicate"
                                </label>
                            </div>
                        </PanelBody>
                    </Panel>

                    <Transition fallback=move || view! { <div class=EMPTY>"Searching..."</div> }>
                        {move || {
                            let q = query.get();
                            if q.trim().is_empty() {
                                return Some(view! { <div class=EMPTY>"Type to search across all providers."</div> }.into_any());
                            }

                            result.get().map(|r| match r {
                                Err(e) => view! { <div class="text-sm text-red-500">{e.to_string()}</div> }.into_any(),
                                Ok(r) => {
                                    view! {
                                        <div class="space-y-5">
                                            <SearchArtistsSection items=r.artists dedupe=dedupe.get() />
                                            <SearchAlbumsSection items=r.albums dedupe=dedupe.get() />
                                            <SearchTracksSection items=r.tracks dedupe=dedupe.get() />
                                        </div>
                                    }.into_any()
                                }
                            })
                        }}
                    </Transition>
                </div>
        </PageShell>
    }
}

#[component]
fn SearchArtistsSection(items: Vec<SearchArtistResult>, dedupe: bool) -> impl IntoView {
    let items = if dedupe {
        let mut seen = HashSet::new();
        items
            .into_iter()
            .filter(|r| seen.insert(r.name.trim().to_lowercase()))
            .collect::<Vec<_>>()
    } else {
        items
    };
    let count = items.len();

    view! {
        <Panel>
            <PanelHeader>
                <PanelTitle>{format!("Artists ({count})")}</PanelTitle>
            </PanelHeader>
            <PanelBody class="p-0!">
                {if items.is_empty() {
                    view! { <div class=EMPTY>"No artists found"</div> }.into_any()
                } else {
                    view! {
                        <div class="divide-y divide-black/[.04] dark:divide-white/[.04]">
                            <For
                                each=move || items.clone()
                                key=|result| provider_result_key(&result.provider, &result.external_id)
                                let:result
                            >
                                <SearchArtistRow result=result />
                            </For>
                        </div>
                    }.into_any()
                }}
            </PanelBody>
        </Panel>
    }
}

#[component]
fn SearchAlbumsSection(items: Vec<SearchAlbumResult>, dedupe: bool) -> impl IntoView {
    let items = if dedupe {
        let mut seen = HashSet::new();
        items
            .into_iter()
            .filter(|r| {
                let key = format!(
                    "{}::{}::{}",
                    r.artist_name.trim().to_lowercase(),
                    r.title.trim().to_lowercase(),
                    r.release_date.clone().unwrap_or_default()
                );
                seen.insert(key)
            })
            .collect::<Vec<_>>()
    } else {
        items
    };
    let count = items.len();

    view! {
        <Panel>
            <PanelHeader>
                <PanelTitle>{format!("Albums ({count})")}</PanelTitle>
            </PanelHeader>
            <PanelBody class="p-0!">
                {if items.is_empty() {
                    view! { <div class=EMPTY>"No albums found"</div> }.into_any()
                } else {
                    view! {
                        <div class="divide-y divide-black/[.04] dark:divide-white/[.04]">
                            <For
                                each=move || items.clone()
                                key=|result| provider_result_key(&result.provider, &result.external_id)
                                let:result
                            >
                                <SearchAlbumRow result=result />
                            </For>
                        </div>
                    }.into_any()
                }}
            </PanelBody>
        </Panel>
    }
}

#[component]
fn SearchTracksSection(items: Vec<SearchTrackResult>, dedupe: bool) -> impl IntoView {
    let items = if dedupe {
        let mut seen = HashSet::new();
        items
            .into_iter()
            .filter(|r| {
                let key = format!(
                    "{}::{}::{}::{}",
                    r.artist_name.trim().to_lowercase(),
                    r.album_title.trim().to_lowercase(),
                    r.title.trim().to_lowercase(),
                    r.duration_secs
                );
                seen.insert(key)
            })
            .collect::<Vec<_>>()
    } else {
        items
    };
    let count = items.len();

    view! {
        <Panel>
            <PanelHeader>
                <PanelTitle>{format!("Tracks ({count})")}</PanelTitle>
            </PanelHeader>
            <PanelBody class="p-0!">
                {if items.is_empty() {
                    view! { <div class=EMPTY>"No tracks found"</div> }.into_any()
                } else {
                    view! {
                        <div class="divide-y divide-black/[.04] dark:divide-white/[.04]">
                            <For
                                each=move || items.clone()
                                key=|result| provider_result_key(&result.provider, &result.external_id)
                                let:result
                            >
                                <SearchTrackRow result=result />
                            </For>
                        </div>
                    }.into_any()
                }}
            </PanelBody>
        </Panel>
    }
}

#[component]
fn SearchArtistRow(result: SearchArtistResult) -> impl IntoView {
    let loading = RwSignal::new(false);
    let provider = result.provider.clone();
    let external_id = result.external_id.clone();
    let name = result.name.clone();
    let image_url = result.image_url.clone();
    let external_url = result.url.clone();
    let tags = result.tags.clone();
    let type_country = match (&result.artist_type, &result.country) {
        (Some(t), Some(c)) if !t.is_empty() && !c.is_empty() => format!("{t} · {c}"),
        (Some(t), _) if !t.is_empty() => t.clone(),
        (_, Some(c)) if !c.is_empty() => c.clone(),
        _ => String::new(),
    };

    view! {
        <div class="px-4 py-3 flex items-center gap-3">
            <div class="size-10 rounded-md overflow-hidden bg-zinc-200 dark:bg-zinc-800 shrink-0">
                {match result.image_url.clone() {
                    Some(url) => view! { <img src=url class="w-full h-full object-cover" alt="" /> }.into_any(),
                    None => view! { <div class="w-full h-full grid place-items-center text-zinc-400 text-xs">"?"</div> }.into_any(),
                }}
            </div>
            <div class="flex-1 min-w-0">
                <div class="text-sm font-medium text-zinc-900 dark:text-zinc-100 truncate">{result.name.clone()}</div>
                <div class={cls!(MUTED, "text-xs truncate")}>
                    {if type_country.is_empty() {
                        result.provider.clone()
                    } else {
                        format!("{} · {}", result.provider.clone(), type_country)
                    }}
                </div>
                {if let Some(dis) = result.disambiguation.clone() {
                    if dis.is_empty() {
                        view! { <span></span> }.into_any()
                    } else {
                        view! { <div class={cls!(MUTED, "text-xs truncate")}>{dis}</div> }.into_any()
                    }
                } else {
                    view! { <span></span> }.into_any()
                }}
                <div class="flex items-center gap-1.5 mt-1 flex-wrap">
                    <Badge mono=true>{result.external_id.clone()}</Badge>
                    {if let Some(pop) = result.popularity {
                        view! {
                            <Badge variant=BadgeVariant::Info>{format!("pop {pop}")}</Badge>
                        }
                        .into_any()
                    } else {
                        view! { <span></span> }.into_any()
                    }}
                    {tags.into_iter().take(2).map(|tag| {
                        view! { <Badge variant=BadgeVariant::Warning>{tag}</Badge> }
                    }).collect_view()}
                </div>
            </div>
            {if let Some(url) = result.url.clone() {
                view! { <a href=url target="_blank" rel="noreferrer" class="text-xs text-blue-500 hover:text-blue-400 no-underline">"Open"</a> }.into_any()
            } else {
                view! { <span></span> }.into_any()
            }}
            <Button variant=ButtonVariant::Primary class="py-1" loading=loading
                on:click=move |_| {
                    dispatch_with_toast_loading(
                        ServerAction::AddArtist {
                            name: name.clone(),
                            provider: provider.clone(),
                            external_id: external_id.clone(),
                            image_url: image_url.clone(),
                            external_url: external_url.clone(),
                        },
                        "Artist added",
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
fn SearchAlbumRow(result: SearchAlbumResult) -> impl IntoView {
    let loading = RwSignal::new(false);
    let provider = result.provider.clone();
    let external_album_id = result.external_id.clone();
    let artist_external_id = result.artist_external_id.clone();
    let artist_name = result.artist_name.clone();

    view! {
        <div class="px-4 py-3 flex items-center gap-3">
            <div class="size-10 rounded-md overflow-hidden bg-zinc-200 dark:bg-zinc-800 shrink-0">
                {match result.cover_url.clone() {
                    Some(url) => view! { <img src=url class="w-full h-full object-cover" alt="" /> }.into_any(),
                    None => view! { <div class="w-full h-full grid place-items-center text-zinc-400 text-xs">"No"</div> }.into_any(),
                }}
            </div>
            <div class="flex-1 min-w-0">
                <div class="text-sm font-medium text-zinc-900 dark:text-zinc-100 truncate">{result.title.clone()}</div>
                <div class={cls!(MUTED, "text-xs truncate")}>{format!("{} · {}", result.artist_name.clone(), result.provider.clone())}</div>
                <div class="flex items-center gap-1.5 mt-1 flex-wrap">
                    {if let Some(rt) = result.album_type.clone() {
                        view! { <Badge>{rt}</Badge> }.into_any()
                    } else {
                        view! { <span></span> }.into_any()
                    }}
                    {if let Some(rd) = result.release_date.clone() {
                        view! { <Badge>{rd}</Badge> }.into_any()
                    } else {
                        view! { <span></span> }.into_any()
                    }}
                    {if result.explicit {
                        view! { <Badge variant=BadgeVariant::Explicit>"Explicit"</Badge> }.into_any()
                    } else {
                        view! { <span></span> }.into_any()
                    }}
                    <Badge mono=true>{result.external_id.clone()}</Badge>
                </div>
            </div>
            {if let Some(url) = result.url.clone() {
                view! { <a href=url target="_blank" rel="noreferrer" class="text-xs text-blue-500 hover:text-blue-400 no-underline">"Open"</a> }.into_any()
            } else {
                view! { <span></span> }.into_any()
            }}
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
fn SearchTrackRow(result: SearchTrackResult) -> impl IntoView {
    let loading = RwSignal::new(false);
    let provider = result.provider.clone();
    let external_track_id = result.external_id.clone();
    let external_album_id = result.album_external_id.clone();
    let artist_external_id = result.artist_external_id.clone();
    let artist_name = result.artist_name.clone();

    view! {
        <div class="px-4 py-3 flex items-center gap-3">
            <div class="size-10 rounded-md overflow-hidden bg-zinc-200 dark:bg-zinc-800 shrink-0">
                {match result.album_cover_url.clone() {
                    Some(url) => view! { <img src=url class="w-full h-full object-cover" alt="" /> }.into_any(),
                    None => view! { <div class="w-full h-full grid place-items-center text-zinc-400 text-xs">"♪"</div> }.into_any(),
                }}
            </div>
            <div class="flex-1 min-w-0">
                <div class="text-sm font-medium text-zinc-900 dark:text-zinc-100 truncate">{result.title.clone()}</div>
                <div class={cls!(MUTED, "text-xs truncate")}>{format!("{} · {}", result.artist_name.clone(), result.album_title.clone())}</div>
                <div class="flex items-center gap-1.5 mt-1 flex-wrap">
                    <Badge variant=BadgeVariant::Info>
                        {result.provider.clone()}
                    </Badge>
                    <Badge>{result.duration_display.clone()}</Badge>
                    {if let Some(v) = result.version.clone() {
                        if v.is_empty() {
                            view! { <span></span> }.into_any()
                        } else {
                            view! { <Badge>{v}</Badge> }.into_any()
                        }
                    } else {
                        view! { <span></span> }.into_any()
                    }}
                    {if let Some(isrc) = result.isrc.clone() {
                        view! { <Badge mono=true>{isrc}</Badge> }.into_any()
                    } else {
                        view! { <span></span> }.into_any()
                    }}
                    {if result.explicit {
                        view! { <Badge variant=BadgeVariant::Explicit>"Explicit"</Badge> }.into_any()
                    } else {
                        view! { <span></span> }.into_any()
                    }}
                </div>
            </div>
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
