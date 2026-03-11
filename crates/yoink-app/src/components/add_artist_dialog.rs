use leptos::portal::Portal;
use leptos::prelude::*;
use lucide_leptos::X;

use yoink_shared::{SearchArtistResult, ServerAction, provider_display_name};

use crate::actions::dispatch_action;
use crate::hooks::use_debounced_signal;
use crate::pages::provider_icon_svg;
use crate::search_result_keys::provider_result_key;
use crate::styles::SEARCH_INPUT;

use leptoaster::{ToastBuilder, ToastLevel, ToastPosition, expect_toaster};

use super::{
    ArtistAvatar, Badge, BadgeSurface, Button, ButtonSize, ButtonVariant, DialogResultRow,
    DialogShell, DialogSize, dialog_shell::DIALOG_BACKDROP_CLASS,
};

#[server(ListProvidersForAddArtist, "/leptos")]
async fn list_providers_for_add_artist() -> Result<Vec<String>, ServerFnError> {
    let ctx = crate::actions::require_ctx()?;
    Ok((ctx.list_providers)())
}

#[server(SearchProvidersForAddArtist, "/leptos")]
async fn search_providers_for_add_artist(
    query: String,
) -> Result<Vec<SearchArtistResult>, ServerFnError> {
    let ctx = crate::actions::require_ctx()?;

    let trimmed = query.trim().to_string();
    if trimmed.is_empty() {
        return Ok(vec![]);
    }

    (ctx.search_artists)(trimmed)
        .await
        .map_err(ServerFnError::new)
}

#[component]
pub fn AddArtistDialog(open: RwSignal<bool>, #[prop(into)] artist_name: String) -> impl IntoView {
    let providers = Resource::new(|| (), |_| list_providers_for_add_artist());
    let filter_provider = RwSignal::<Option<String>>::new(None);
    let (query, debounced_query) = use_debounced_signal(String::new());

    let search_results: Resource<Result<Vec<SearchArtistResult>, ServerFnError>> = Resource::new(
        move || debounced_query.get(),
        |q| async move {
            if q.trim().is_empty() {
                return Ok(vec![]);
            }
            search_providers_for_add_artist(q).await
        },
    );

    let artist_name = StoredValue::new(artist_name);
    let session_added = RwSignal::new(Vec::<(String, String)>::new());

    Effect::new(move || {
        let is_open = open.get();
        if is_open {
            query.set(artist_name.with_value(|name| name.clone()));
            filter_provider.set(None);
            session_added.set(Vec::new());
        }
        #[cfg(feature = "hydrate")]
        {
            use crate::components::confirm_dialog::scroll_lock;
            if is_open {
                scroll_lock::acquire();
            } else {
                scroll_lock::release();
            }
        }
    });

    let close_on_escape = move |ev: leptos::ev::KeyboardEvent| {
        if ev.key() == "Escape" {
            open.set(false);
        }
    };

    view! {
        <Portal>
            <Show when=move || open.get()>
                <div class=DIALOG_BACKDROP_CLASS on:click=move |_: leptos::ev::MouseEvent| open.set(false) on:keydown=close_on_escape tabindex="-1">
                    <DialogShell open=open title="Add Artist" size=DialogSize::Lg class="max-h-[85vh] flex flex-col overflow-hidden">
                        <div class="px-5 py-3 border-b border-black/[.04] dark:border-white/[.04] flex flex-col gap-2.5 shrink-0">
                            <div class="relative">
                                <input
                                    type="text"
                                    class=SEARCH_INPUT
                                    placeholder="Search artist name..."
                                    autocomplete="off"
                                    aria-label="Search artist across providers"
                                    prop:value=move || query.get()
                                    on:input=move |ev| {
                                        query.set(event_target_value(&ev));
                                    }
                                />
                                <Show when=move || !query.get().is_empty()>
                                    <button
                                        type="button"
                                        class="absolute right-2 top-1/2 -translate-y-1/2 inline-flex items-center justify-center size-5 rounded-md text-zinc-400 hover:text-zinc-600 dark:hover:text-zinc-200 cursor-pointer bg-transparent border-none p-0 [&_svg]:size-3.5"
                                        on:click=move |_| query.set(String::new())
                                        title="Clear"
                                    >
                                        <X />
                                    </button>
                                </Show>
                            </div>

                            <Suspense fallback=|| ()>
                                {move || providers.get().map(|result| {
                                    match result {
                                        Err(_) => view! { <span></span> }.into_any(),
                                        Ok(all_providers) => {
                                            if all_providers.len() <= 1 {
                                                return view! { <span></span> }.into_any();
                                            }
                                            view! {
                                                <div class="flex flex-wrap gap-1.5">
                                                    <button
                                                        type="button"
                                                        class=move || {
                                                            if filter_provider.get().is_none() {
                                                                "inline-flex items-center gap-1.5 px-2.5 py-1 rounded-lg text-[11px] font-semibold cursor-pointer transition-all duration-150 border bg-blue-500/10 border-blue-500/30 text-blue-600 dark:text-blue-400 dark:bg-blue-500/15 dark:border-blue-500/40"
                                                            } else {
                                                                "inline-flex items-center gap-1.5 px-2.5 py-1 rounded-lg text-[11px] font-semibold cursor-pointer transition-all duration-150 border bg-white/40 dark:bg-zinc-800/40 border-black/[.06] dark:border-white/[.06] text-zinc-500 dark:text-zinc-400 hover:border-black/10 dark:hover:border-white/10"
                                                            }
                                                        }
                                                        on:click=move |_| filter_provider.set(None)
                                                    >
                                                        "All"
                                                    </button>
                                                    {all_providers.iter().map(|p| {
                                                        let provider_id = p.clone();
                                                        let provider_id2 = p.clone();
                                                        let display = provider_display_name(p);
                                                        let icon = provider_icon_svg(p);
                                                        view! {
                                                            <button
                                                                type="button"
                                                                class=move || {
                                                                    let is_active = filter_provider.get().as_deref() == Some(&provider_id);
                                                                    if is_active {
                                                                        "inline-flex items-center gap-1.5 px-2.5 py-1 rounded-lg text-[11px] font-semibold cursor-pointer transition-all duration-150 border bg-blue-500/10 border-blue-500/30 text-blue-600 dark:text-blue-400 dark:bg-blue-500/15 dark:border-blue-500/40"
                                                                    } else {
                                                                        "inline-flex items-center gap-1.5 px-2.5 py-1 rounded-lg text-[11px] font-semibold cursor-pointer transition-all duration-150 border bg-white/40 dark:bg-zinc-800/40 border-black/[.06] dark:border-white/[.06] text-zinc-500 dark:text-zinc-400 hover:border-black/10 dark:hover:border-white/10"
                                                                    }
                                                                }
                                                                on:click={
                                                                    let pid = provider_id2.clone();
                                                                    move |_| {
                                                                        if filter_provider.get().as_deref() == Some(&pid) {
                                                                            filter_provider.set(None);
                                                                        } else {
                                                                            filter_provider.set(Some(pid.clone()));
                                                                        }
                                                                    }
                                                                }
                                                            >
                                                                <span class="shrink-0 [&>svg]:size-3" inner_html=icon></span>
                                                                {display}
                                                            </button>
                                                        }
                                                    }).collect_view()}
                                                </div>
                                            }.into_any()
                                        }
                                    }
                                })}
                            </Suspense>
                        </div>

                        <div class="flex-1 overflow-y-auto min-h-0">
                            <Suspense fallback=move || view! {
                                <Show when=move || !query.get().trim().is_empty()>
                                    <div class="flex items-center gap-2 px-4 py-3 text-sm text-zinc-500 dark:text-zinc-400">
                                        <span class="inline-block size-4 border-2 border-zinc-300 dark:border-zinc-600 border-t-blue-500 dark:border-t-blue-400 rounded-full animate-spin"></span>
                                        "Searching..."
                                    </div>
                                </Show>
                            }>
                                {move || search_results.get().map(|result| match result {
                                    Err(e) => view! {
                                        <div class="px-4 py-3 text-sm text-red-500">{format!("Search failed: {e}")}</div>
                                    }.into_any(),
                                    Ok(results) => {
                                        let added_now = session_added.get();
                                        let active_filter = filter_provider.get();
                                        let visible: Vec<SearchArtistResult> = results
                                            .into_iter()
                                            .filter(|r| {
                                                !added_now.iter().any(|(p, e)| p == &r.provider && e == &r.external_id)
                                            })
                                            .filter(|r| active_filter.as_ref().is_none_or(|f| &r.provider == f))
                                            .collect();

                                        if visible.is_empty() && !query.get().trim().is_empty() {
                                            view! {
                                                <div class="text-center py-6 text-sm text-zinc-400 dark:text-zinc-600">"No results found"</div>
                                            }.into_any()
                                        } else {
                                            let visible = StoredValue::new(visible);
                                            view! {
                                                <div>
                                                    <For
                                                        each=move || visible.with_value(|results| results.clone())
                                                        key=|result| provider_result_key(&result.provider, &result.external_id)
                                                        let:result
                                                    >
                                                        <AddArtistResultRow
                                                            result=result
                                                            session_added=session_added
                                                            on_added=move || open.set(false)
                                                        />
                                                    </For>
                                                </div>
                                            }.into_any()
                                        }
                                    }
                                })}
                            </Suspense>
                        </div>

                        <div class="px-5 py-3 border-t border-black/[.06] dark:border-white/[.06] flex justify-end shrink-0">
                            <Button size=ButtonSize::Lg on:click=move |_| open.set(false)>
                                "Close"
                            </Button>
                        </div>
                    </DialogShell>
                </div>
            </Show>
        </Portal>
    }
}

#[component]
fn AddArtistResultRow(
    result: SearchArtistResult,
    session_added: RwSignal<Vec<(String, String)>>,
    on_added: impl Fn() + Clone + Send + 'static,
) -> impl IntoView {
    let adding = RwSignal::new(false);

    let provider_display = provider_display_name(&result.provider);
    let provider_icon = provider_icon_svg(&result.provider);

    let disambiguation = result.disambiguation.clone();
    let artist_type = result.artist_type.clone();
    let country = result.country.clone();
    let tags = result.tags.clone();
    let popularity = result.popularity;

    let type_country: Option<String> = match (&artist_type, &country) {
        (Some(t), Some(c)) => Some(format!("{t} from {c}")),
        (Some(t), None) => Some(t.clone()),
        (None, Some(c)) => Some(format!("from {c}")),
        (None, None) => None,
    };

    let subtitle: Option<String> = {
        let base = disambiguation.or(type_country);
        match (base, popularity) {
            (Some(b), Some(p)) => Some(format!("{b} · {p}% popularity")),
            (Some(b), None) => Some(b),
            (None, Some(p)) => Some(format!("{p}% popularity")),
            (None, None) => None,
        }
    };

    let provider = result.provider.clone();
    let external_id = result.external_id.clone();
    let external_url = result.url.clone();
    let external_name = result.name.clone();
    let image_url = result.image_url.clone();

    view! {
        <DialogResultRow>
            <ArtistAvatar name=result.name.clone() image_url=result.image_url.clone() />
            <div class="flex-1 min-w-0">
                <div class="flex items-center gap-1.5">
                    <span class="text-sm font-semibold text-zinc-900 dark:text-zinc-100 truncate">{result.name.clone()}</span>
                    <Badge surface=BadgeSurface::Outline>
                        <span class="shrink-0 [&>svg]:size-2.5" inner_html=provider_icon></span>
                        {provider_display}
                    </Badge>
                </div>
                {subtitle.map(|s| view! {
                    <div class="text-[11px] text-zinc-500 dark:text-zinc-400 mt-0.5 leading-snug truncate">{s}</div>
                })}
                {(!tags.is_empty()).then(|| view! {
                    <div class="flex flex-wrap gap-1 mt-0.5">
                        {tags.into_iter().take(3).map(|tag| view! {
                            <Badge surface=BadgeSurface::Outline>{tag}</Badge>
                        }).collect_view()}
                    </div>
                })}
            </div>
            <Button
                variant=ButtonVariant::Primary
                size=ButtonSize::Sm
                loading=adding
                on:click={
                    let provider = provider.clone();
                    let external_id = external_id.clone();
                    let external_url = external_url.clone();
                    let external_name = external_name.clone();
                    let image_url = image_url.clone();
                    let on_added = on_added.clone();
                    move |_| {
                        adding.set(true);
                        let provider = provider.clone();
                        let external_id = external_id.clone();
                        let external_url = external_url.clone();
                        let external_name = external_name.clone();
                        let image_url = image_url.clone();
                        let on_added = on_added.clone();
                        let toaster = expect_toaster();
                        leptos::task::spawn_local(async move {
                            match dispatch_action(ServerAction::AddArtist {
                                name: external_name.clone(),
                                provider: provider.clone(),
                                external_id: external_id.clone(),
                                image_url,
                                external_url,
                            }).await {
                                Ok(()) => {
                                    toaster.toast(
                                        ToastBuilder::new("Artist added")
                                            .with_level(ToastLevel::Success)
                                            .with_position(ToastPosition::BottomRight)
                                            .with_expiry(Some(4_000)),
                                    );
                                    session_added.update(|v| v.push((provider, external_id)));
                                    on_added();
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
                    }
                }
            >
                {move || if adding.get() { "Adding..." } else { "Add" }}
            </Button>
        </DialogResultRow>
    }
}
