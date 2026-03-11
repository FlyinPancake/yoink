use leptos::portal::Portal;
use leptos::prelude::*;
use lucide_leptos::X;

use yoink_shared::{SearchArtistResult, ServerAction, provider_display_name};

use super::{
    ArtistAvatar, Badge, BadgeSurface, Button, ButtonSize, ButtonVariant, DialogResultRow,
    DialogShell, DialogSize, dialog_shell::DIALOG_BACKDROP_CLASS,
};
use crate::actions::dispatch_action;
use crate::hooks::use_debounced_signal;
use crate::pages::provider_icon_svg;
use crate::search_result_keys::provider_result_key;
use crate::styles::SEARCH_INPUT;
use leptoaster::{ToastBuilder, ToastLevel, ToastPosition, expect_toaster};

// ── Tailwind class constants ────────────────────────────────

const FILTER_ACTIVE: &str = "inline-flex items-center gap-1.5 px-2.5 py-1 rounded-lg text-[11px] font-semibold cursor-pointer transition-all duration-150 border bg-blue-500/10 border-blue-500/30 text-blue-600 dark:text-blue-400 dark:bg-blue-500/15 dark:border-blue-500/40";
const FILTER_INACTIVE: &str = "inline-flex items-center gap-1.5 px-2.5 py-1 rounded-lg text-[11px] font-semibold cursor-pointer transition-all duration-150 border bg-white/40 dark:bg-zinc-800/40 border-black/[.06] dark:border-white/[.06] text-zinc-500 dark:text-zinc-400 hover:border-black/10 dark:hover:border-white/10";

// ── Server functions ────────────────────────────────────────

#[server(ListProviders, "/leptos")]
pub async fn list_providers() -> Result<Vec<String>, ServerFnError> {
    let ctx = crate::actions::require_ctx()?;
    Ok((ctx.list_providers)())
}

#[server(SearchAllProviders, "/leptos")]
pub async fn search_all_providers(query: String) -> Result<Vec<SearchArtistResult>, ServerFnError> {
    let ctx = crate::actions::require_ctx()?;

    let trimmed = query.trim().to_string();
    if trimmed.is_empty() {
        return Ok(vec![]);
    }

    (ctx.search_artists)(trimmed)
        .await
        .map_err(ServerFnError::new)
}

// ── Component ───────────────────────────────────────────────

/// A dialog that lets the user search all metadata providers and link
/// results to an existing local artist. Results are mixed and sorted by
/// relevance; provider badge filters narrow the list.
///
/// # Props
/// - `open` - controls visibility
/// - `artist_id` - the local artist UUID to link to
/// - `artist_name` - used as default search query
/// - `already_linked` - (provider, external_id) pairs already linked in DB
#[component]
pub fn LinkProviderDialog(
    open: RwSignal<bool>,
    artist_id: yoink_shared::Uuid,
    #[prop(into)] artist_name: String,
    #[prop(into)] already_linked: Vec<(String, String)>,
) -> impl IntoView {
    // Fetch available providers once (for filter badges)
    let providers = Resource::new(|| (), |_| list_providers());

    // Active provider filter — None means "All"
    let filter_provider = RwSignal::<Option<String>>::new(None);

    // Search query — default to artist name
    let default_query = artist_name.clone();
    let (query, debounced_query) = use_debounced_signal(String::new());

    // Search all providers at once
    let search_results: Resource<Result<Vec<SearchArtistResult>, ServerFnError>> = Resource::new(
        move || debounced_query.get(),
        |q| async move {
            if q.trim().is_empty() {
                return Ok(vec![]);
            }
            search_all_providers(q).await
        },
    );

    // Track external IDs linked during this session
    let session_linked = RwSignal::new(Vec::<(String, String)>::new());

    // Reset state when dialog opens
    let default_query_stored = StoredValue::new(default_query);
    Effect::new(move || {
        let is_open = open.get();
        if is_open {
            query.set(default_query_stored.with_value(|q| q.clone()));
            filter_provider.set(None);
            session_linked.set(Vec::new());
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

    let artist_id = StoredValue::new(artist_id);
    let already_linked = StoredValue::new(already_linked);
    let close_on_escape = move |ev: leptos::ev::KeyboardEvent| {
        if ev.key() == "Escape" {
            open.set(false);
        }
    };

    view! {
        <Portal>
            <Show when=move || open.get()>
                <div class=DIALOG_BACKDROP_CLASS on:click=move |_: leptos::ev::MouseEvent| open.set(false) on:keydown=close_on_escape tabindex="-1">
                    <DialogShell open=open title="Link Provider" size=DialogSize::Lg class="max-h-[85vh] flex flex-col overflow-hidden">
                        // Search input + provider filter badges
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
                                    <button type="button"
                                        class="absolute right-2 top-1/2 -translate-y-1/2 inline-flex items-center justify-center size-5 rounded-md text-zinc-400 hover:text-zinc-600 dark:hover:text-zinc-200 cursor-pointer bg-transparent border-none p-0 [&_svg]:size-3.5"
                                        on:click=move |_| query.set(String::new())
                                        title="Clear"
                                    >
                                        <X />
                                    </button>
                                </Show>
                            </div>

                            // Provider filter badges
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
                                                    <button type="button"
                                                        class=move || if filter_provider.get().is_none() { FILTER_ACTIVE } else { FILTER_INACTIVE }
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
                                                            <button type="button"
                                                                class=move || {
                                                                    let is_active = filter_provider.get().as_deref() == Some(&provider_id);
                                                                    if is_active { FILTER_ACTIVE } else { FILTER_INACTIVE }
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

                        // Results
                        <div class="flex-1 overflow-y-auto min-h-0">
                            <Suspense fallback=move || view! {
                                <Show when=move || !query.get().trim().is_empty()>
                                    <div class="flex items-center gap-2 px-4 py-3 text-sm text-zinc-500 dark:text-zinc-400">
                                        <span class="inline-block size-4 border-2 border-zinc-300 dark:border-zinc-600 border-t-blue-500 dark:border-t-blue-400 rounded-full animate-spin"></span>
                                        "Searching\u{2026}"
                                    </div>
                                </Show>
                            }>
                                {move || search_results.get().map(|result| {
                                    match result {
                                        Err(e) => view! {
                                            <div class="px-4 py-3 text-sm text-red-500">{format!("Search failed: {e}")}</div>
                                        }.into_any(),
                                        Ok(results) => {
                                            // Filter out already-linked results
                                            let linked_in_db = already_linked.with_value(|l| l.clone());
                                            let linked_now = session_linked.get();
                                            let is_linked = |provider: &str, eid: &str| -> bool {
                                                linked_in_db.iter().any(|(p, e)| p == provider && e == eid)
                                                    || linked_now.iter().any(|(p, e)| p == provider && e == eid)
                                            };

                                            // Apply provider filter
                                            let active_filter = filter_provider.get();
                                            let visible: Vec<SearchArtistResult> = results
                                                .into_iter()
                                                .filter(|r| !is_linked(&r.provider, &r.external_id))
                                                .filter(|r| {
                                                    active_filter.as_ref().is_none_or(|f| &r.provider == f)
                                                })
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
                                                            <LinkResultRow
                                                                result=result
                                                                artist_id=artist_id.with_value(|id| *id)
                                                                session_linked=session_linked
                                                            />
                                                        </For>
                                                    </div>
                                                }.into_any()
                                            }
                                        }
                                    }
                                })}
                            </Suspense>
                        </div>

                        // Footer
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

/// A single search result row with provider badge and "Link" button.
#[component]
fn LinkResultRow(
    result: SearchArtistResult,
    artist_id: yoink_shared::Uuid,
    session_linked: RwSignal<Vec<(String, String)>>,
) -> impl IntoView {
    let linking = RwSignal::new(false);

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
            (Some(b), Some(p)) => Some(format!("{b} \u{00b7} {p}% popularity")),
            (Some(b), None) => Some(b),
            (None, Some(p)) => Some(format!("{p}% popularity")),
            (None, None) => None,
        }
    };

    let provider = result.provider.clone();
    let external_id = result.external_id.clone();
    let external_url = result.url.clone();
    let external_name = result.name.clone();
    let image_ref_val = result.image_url.clone();

    view! {
        <DialogResultRow>
            <ArtistAvatar name=result.name.clone() image_url=result.image_url.clone() />
            <div class="flex-1 min-w-0">
                <div class="flex items-center gap-1.5">
                    <span class="text-sm font-semibold text-zinc-900 dark:text-zinc-100 truncate">{result.name.clone()}</span>
                    // Provider badge
                    <Badge surface=BadgeSurface::Outline>
                        <span class="shrink-0 [&>svg]:size-2.5" inner_html=provider_icon></span>
                        {provider_display}
                    </Badge>
                </div>
                // Subtitle: disambiguation/type/country + popularity
                {subtitle.map(|s| view! {
                    <div class="text-[11px] text-zinc-500 dark:text-zinc-400 mt-0.5 leading-snug truncate">{s}</div>
                })}
                // Tags as small inline pills
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
                loading=linking
                on:click={
                    let provider = provider.clone();
                    let external_id = external_id.clone();
                    let external_url = external_url.clone();
                    let external_name = external_name.clone();
                    let image_ref_val = image_ref_val.clone();
                    move |_| {
                        linking.set(true);
                        let provider = provider.clone();
                        let external_id = external_id.clone();
                        let external_url = external_url.clone();
                        let external_name = external_name.clone();
                        let image_ref_val = image_ref_val.clone();
                        let toaster = expect_toaster();
                        leptos::task::spawn_local(async move {
                            let prov_for_track = provider.clone();
                            let eid_for_track = external_id.clone();
                            match dispatch_action(ServerAction::LinkArtistProvider {
                                artist_id,
                                provider,
                                external_id,
                                external_url,
                                external_name: Some(external_name),
                                image_ref: image_ref_val,
                            }).await {
                                Ok(()) => {
                                    toaster.toast(
                                        ToastBuilder::new("Provider linked")
                                            .with_level(ToastLevel::Success)
                                            .with_position(ToastPosition::BottomRight)
                                            .with_expiry(Some(4_000)),
                                    );
                                    session_linked.update(|v| {
                                        v.push((prov_for_track, eid_for_track));
                                    });
                                }
                                Err(e) => {
                                    toaster.toast(
                                        ToastBuilder::new(format!("Error: {e}"))
                                            .with_level(ToastLevel::Error)
                                            .with_position(ToastPosition::BottomRight)
                                            .with_expiry(Some(8_000)),
                                    );
                                    linking.set(false);
                                }
                            }
                        });
                    }
                }>
                {move || if linking.get() { "Linking\u{2026}" } else { "Link" }}
            </Button>
        </DialogResultRow>
    }
}
