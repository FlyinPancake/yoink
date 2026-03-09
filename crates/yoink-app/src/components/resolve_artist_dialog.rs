use leptos::portal::Portal;
use leptos::prelude::*;
use lucide_leptos::{Link, Search, UserPlus, X};

use yoink_shared::{MonitoredArtist, SearchArtistResult, ServerAction, provider_display_name};

use crate::actions::dispatch_action;
use crate::pages::provider_icon_svg;
use crate::search_result_keys::{monitored_artist_key, provider_result_key};
use crate::styles::SEARCH_INPUT;
use leptoaster::{ToastBuilder, ToastLevel, ToastPosition, expect_toaster};

use super::link_provider_dialog::search_all_providers;
use super::{
    ArtistAvatar, Button, ButtonSize, ButtonVariant, DialogResultRow, DialogSectionLabel,
    DialogShell, DialogSize, dialog_shell::DIALOG_BACKDROP_CLASS,
};

// ── Tailwind class constants ────────────────────────────────

// ── Server function: get monitored artists ──────────────────

#[server(GetMonitoredArtists, "/leptos")]
pub async fn get_monitored_artists() -> Result<Vec<MonitoredArtist>, ServerFnError> {
    let ctx = use_context::<yoink_shared::ServerContext>()
        .ok_or_else(|| ServerFnError::new("ServerContext not available"))?;
    Ok(ctx.monitored_artists.read().await.clone())
}

// ── Component ───────────────────────────────────────────────

/// A dialog for resolving an unlinked album artist credit.
///
/// Presents two options:
/// 1. **Add as new artist** — creates a new monitored artist from the provider data
/// 2. **Link to existing artist** — search existing monitored artists or providers
///
/// # Props
/// - `open` — controls visibility
/// - `album_id` — the album to associate the artist with
/// - `credit_name` — display name from the provider
/// - `credit_provider` — provider ID (e.g. "tidal")
/// - `credit_external_id` — external artist ID in that provider
#[component]
pub fn ResolveArtistDialog(
    open: RwSignal<bool>,
    album_id: yoink_shared::Uuid,
    #[prop(into)] credit_name: String,
    #[prop(into, optional)] credit_provider: String,
    #[prop(into, optional)] credit_external_id: String,
) -> impl IntoView {
    let credit_name_stored = StoredValue::new(credit_name.clone());
    let credit_provider_stored = StoredValue::new(if credit_provider.is_empty() {
        None
    } else {
        Some(credit_provider)
    });
    let credit_external_id_stored = StoredValue::new(if credit_external_id.is_empty() {
        None
    } else {
        Some(credit_external_id)
    });
    let album_id_stored = StoredValue::new(album_id);

    let adding_new = RwSignal::new(false);
    let mode = RwSignal::new("choose".to_string()); // "choose", "link_existing", "search_new"

    // Search query for linking to existing
    let query = RwSignal::new(String::new());
    let debounced_query = RwSignal::new(String::new());

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
                let timer_id_inner = timer_id.clone();
                let dq = debounced_query;
                let cb = wasm_bindgen::closure::Closure::once_into_js(move || {
                    dq.set(val);
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
            debounced_query.set(query.get());
        });
    }

    // Monitored artists for "link to existing"
    let monitored_artists = Resource::new(
        move || mode.get(),
        |m| async move {
            if m == "link_existing" {
                get_monitored_artists().await.unwrap_or_default()
            } else {
                Vec::new()
            }
        },
    );

    // Provider search results for "search providers"
    let search_results: Resource<Result<Vec<SearchArtistResult>, ServerFnError>> = Resource::new(
        move || debounced_query.get(),
        |q| async move {
            if q.trim().is_empty() {
                return Ok(vec![]);
            }
            search_all_providers(q).await
        },
    );

    // Reset state when dialog opens
    Effect::new(move || {
        let is_open = open.get();
        if is_open {
            mode.set("choose".to_string());
            adding_new.set(false);
            query.set(credit_name_stored.with_value(|n| n.clone()));
            debounced_query.set(credit_name_stored.with_value(|n| n.clone()));
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
                    <DialogShell
                        open=open
                        title=credit_name_stored.with_value(|n| n.clone())
                        subtitle="This artist isn\u{2019}t linked to a local artist yet."
                        size=DialogSize::Lg
                        class="max-h-[85vh] flex flex-col overflow-hidden"
                    >
                        // Body — mode-dependent content
                        <div class="flex-1 overflow-y-auto min-h-0">
                            // ── Choose mode ─────────────────────
                            <Show when=move || mode.get() == "choose">
                                <div class="p-5 flex flex-col gap-3">
                                    // "Add as new artist" button — only if we have full source identity
                                    {move || {
                                        let add_source = credit_provider_stored
                                            .with_value(|p| p.clone())
                                            .zip(credit_external_id_stored.with_value(|e| e.clone()));
                                        let provider_display = add_source
                                            .as_ref()
                                            .map(|(provider, _)| provider_display_name(provider))
                                            .unwrap_or_default();
                                        let name = credit_name_stored.with_value(|n| n.clone());
                                        if let Some((provider, external_id)) = add_source {
                                            view! {
                                                <button type="button"
                                                    class="flex items-start gap-3 w-full text-left p-4 rounded-xl border border-black/[.06] dark:border-white/[.08] bg-white/50 dark:bg-zinc-800/50 transition-all duration-150 hover:border-blue-500/30 hover:bg-blue-500/[.04] dark:hover:bg-blue-500/[.06] cursor-pointer group"
                                                    disabled=move || adding_new.get()
                                                    on:click={
                                                        let provider = provider.clone();
                                                        let external_id = external_id.clone();
                                                        move |_| {
                                                            adding_new.set(true);
                                                            let name = credit_name_stored.with_value(|n| n.clone());
                                                            let provider = provider.clone();
                                                            let external_id = external_id.clone();
                                                            let toaster = expect_toaster();
                                                            leptos::task::spawn_local(async move {
                                                                // Add the artist (re-sync will associate with album)
                                                                let add_result = dispatch_action(ServerAction::AddArtist {
                                                                    name: name.clone(),
                                                                    provider,
                                                                    external_id,
                                                                    image_url: None,
                                                                    external_url: None,
                                                                }).await;
                                                                match add_result {
                                                                    Ok(()) => {
                                                                        toaster.toast(
                                                                            ToastBuilder::new(format!("{name} added and linked"))
                                                                                .with_level(ToastLevel::Success)
                                                                                .with_position(ToastPosition::BottomRight)
                                                                                .with_expiry(Some(4_000)),
                                                                        );
                                                                        open.set(false);
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
                                                                adding_new.set(false);
                                                            });
                                                        }
                                                    }
                                                >
                                                    <div class="size-10 rounded-full bg-blue-500/10 dark:bg-blue-500/15 flex items-center justify-center shrink-0 group-hover:bg-blue-500/20">
                                                        <span class="text-blue-500 [&>svg]:size-[18px]"><UserPlus /></span>
                                                    </div>
                                                    <div>
                                                        <div class="text-sm font-semibold text-zinc-900 dark:text-zinc-100">
                                                            {move || if adding_new.get() { "Adding\u{2026}".to_string() } else { "Add as new artist".to_string() }}
                                                        </div>
                                                        <div class="text-[12px] text-zinc-500 dark:text-zinc-400 mt-0.5">
                                                            {format!("Create \u{201c}{name}\u{201d} as a monitored artist from {provider_display}")}
                                                        </div>
                                                    </div>
                                                </button>
                                            }.into_any()
                                        } else {
                                            view! { <span></span> }.into_any()
                                        }
                                    }}

                                    // "Link to existing artist"
                                    <button type="button"
                                        class="flex items-start gap-3 w-full text-left p-4 rounded-xl border border-black/[.06] dark:border-white/[.08] bg-white/50 dark:bg-zinc-800/50 transition-all duration-150 hover:border-blue-500/30 hover:bg-blue-500/[.04] dark:hover:bg-blue-500/[.06] cursor-pointer group"
                                        on:click=move |_| mode.set("link_existing".to_string())
                                    >
                                        <div class="size-10 rounded-full bg-zinc-100 dark:bg-zinc-800 flex items-center justify-center shrink-0 group-hover:bg-zinc-200 dark:group-hover:bg-zinc-700">
                                            <span class="text-zinc-500 dark:text-zinc-400 [&>svg]:size-[18px]"><Link /></span>
                                        </div>
                                        <div>
                                            <div class="text-sm font-semibold text-zinc-900 dark:text-zinc-100">"Link to existing artist"</div>
                                            <div class="text-[12px] text-zinc-500 dark:text-zinc-400 mt-0.5">
                                                "Choose from your already-monitored artists"
                                            </div>
                                        </div>
                                    </button>

                                    // "Search providers"
                                    <button type="button"
                                        class="flex items-start gap-3 w-full text-left p-4 rounded-xl border border-black/[.06] dark:border-white/[.08] bg-white/50 dark:bg-zinc-800/50 transition-all duration-150 hover:border-blue-500/30 hover:bg-blue-500/[.04] dark:hover:bg-blue-500/[.06] cursor-pointer group"
                                        on:click=move |_| mode.set("search_new".to_string())
                                    >
                                        <div class="size-10 rounded-full bg-zinc-100 dark:bg-zinc-800 flex items-center justify-center shrink-0 group-hover:bg-zinc-200 dark:group-hover:bg-zinc-700">
                                            <span class="text-zinc-500 dark:text-zinc-400 [&>svg]:size-[18px]"><Search /></span>
                                        </div>
                                        <div>
                                            <div class="text-sm font-semibold text-zinc-900 dark:text-zinc-100">"Search providers"</div>
                                            <div class="text-[12px] text-zinc-500 dark:text-zinc-400 mt-0.5">
                                                "Find and add the artist from a metadata provider"
                                            </div>
                                        </div>
                                    </button>
                                </div>
                            </Show>

                            // ── Link to existing artist ─────────
                            <Show when=move || mode.get() == "link_existing">
                                <DialogSectionLabel>
                                    <button type="button"
                                        class="text-blue-500 dark:text-blue-400 hover:text-blue-400 dark:hover:text-blue-300 bg-transparent border-none cursor-pointer p-0 font-inherit text-[11px] uppercase tracking-wider font-semibold"
                                        on:click=move |_| mode.set("choose".to_string())
                                    >
                                        "\u{2190} Back"
                                    </button>
                                    " \u{00b7} Link to existing artist"
                                </DialogSectionLabel>
                                <Suspense fallback=move || view! {
                                    <div class="p-4 text-sm text-zinc-500 dark:text-zinc-400">"Loading artists\u{2026}"</div>
                                }>
                                    {move || monitored_artists.get().map(|artists| {
                                        let aid = album_id_stored.with_value(|id| *id);
                                        let credit_name = credit_name_stored.with_value(|n| n.clone());
                                        let credit_provider = credit_provider_stored.with_value(|p| p.clone());
                                        let credit_external_id = credit_external_id_stored.with_value(|e| e.clone());
                                        if artists.is_empty() {
                                            view! {
                                                <div class="text-center py-6 text-sm text-zinc-400 dark:text-zinc-600">
                                                    "No monitored artists yet."
                                                </div>
                                            }.into_any()
                                        } else {
                                            // Sort: name-match first, then alphabetical
                                            let needle = credit_name.to_lowercase();
                                            let mut sorted = artists;
                                            sorted.sort_by(|a, b| {
                                                let a_match = a.name.to_lowercase().contains(&needle);
                                                let b_match = b.name.to_lowercase().contains(&needle);
                                                b_match.cmp(&a_match)
                                                    .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
                                            });
                                            let sorted = StoredValue::new(sorted);
                                            view! {
                                                <div>
                                                    <For
                                                        each=move || sorted.with_value(|artists| artists.clone())
                                                        key=|artist| monitored_artist_key(artist)
                                                        let:artist
                                                    >
                                                        <ExistingArtistRow
                                                            artist=artist
                                                            album_id=aid
                                                            credit_provider=credit_provider.clone()
                                                            credit_external_id=credit_external_id.clone()
                                                            open=open
                                                        />
                                                    </For>
                                                </div>
                                            }.into_any()
                                        }
                                    })}
                                </Suspense>
                            </Show>

                            // ── Search providers ────────────────
                            <Show when=move || mode.get() == "search_new">
                                <DialogSectionLabel>
                                    <button type="button"
                                        class="text-blue-500 dark:text-blue-400 hover:text-blue-400 dark:hover:text-blue-300 bg-transparent border-none cursor-pointer p-0 font-inherit text-[11px] uppercase tracking-wider font-semibold"
                                        on:click=move |_| mode.set("choose".to_string())
                                    >
                                        "\u{2190} Back"
                                    </button>
                                    " \u{00b7} Search providers"
                                </DialogSectionLabel>
                                <div class="px-4 py-3 border-b border-black/[.04] dark:border-white/[.04] shrink-0">
                                    <div class="relative">
                                        <input
                                            type="text"
                                            class=SEARCH_INPUT
                                            placeholder="Search artist name..."
                                            autocomplete="off"
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
                                </div>
                                <Suspense fallback=move || view! {
                                    <Show when=move || !query.get().trim().is_empty()>
                                        <div class="flex items-center gap-2 px-4 py-3 text-sm text-zinc-500 dark:text-zinc-400">
                                            <span class="inline-block size-4 border-2 border-zinc-300 dark:border-zinc-600 border-t-blue-500 dark:border-t-blue-400 rounded-full animate-spin"></span>
                                            "Searching\u{2026}"
                                        </div>
                                    </Show>
                                }>
                                    {move || search_results.get().map(|result| {
                                        let aid = album_id_stored.with_value(|id| *id);
                                        match result {
                                            Err(e) => view! {
                                                <div class="px-4 py-3 text-sm text-red-500">{format!("Search failed: {e}")}</div>
                                            }.into_any(),
                                            Ok(results) => {
                                                if results.is_empty() && !query.get().trim().is_empty() {
                                                    view! {
                                                        <div class="text-center py-6 text-sm text-zinc-400 dark:text-zinc-600">"No results found"</div>
                                                    }.into_any()
                                                } else {
                                                    let results = StoredValue::new(results);
                                                view! {
                                                    <div>
                                                        <For
                                                            each=move || results.with_value(|results| results.clone())
                                                            key=|result| provider_result_key(&result.provider, &result.external_id)
                                                            let:result
                                                        >
                                                            <ProviderSearchRow result=result album_id=aid open=open />
                                                        </For>
                                                    </div>
                                                }.into_any()
                                                }
                                            }
                                        }
                                    })}
                                </Suspense>
                            </Show>
                        </div>

                        // Footer
                        <div class="px-5 py-3 border-t border-black/[.06] dark:border-white/[.06] flex justify-end shrink-0">
                            <Button size=ButtonSize::Lg on:click=move |_| open.set(false)>
                                "Cancel"
                            </Button>
                        </div>
                    </DialogShell>
                </div>
            </Show>
        </Portal>
    }
}

// ── Row: existing monitored artist ──────────────────────────

#[component]
fn ExistingArtistRow(
    artist: MonitoredArtist,
    album_id: yoink_shared::Uuid,
    credit_provider: Option<String>,
    credit_external_id: Option<String>,
    open: RwSignal<bool>,
) -> impl IntoView {
    let linking = RwSignal::new(false);
    let artist_name = artist.name.clone();
    let artist_id = artist.id;

    view! {
        <DialogResultRow>
            <ArtistAvatar name=artist_name.clone() image_url=artist.image_url.clone() />
            <div class="flex-1 min-w-0">
                <span class="text-sm font-semibold text-zinc-900 dark:text-zinc-100 truncate">{artist_name.clone()}</span>
            </div>
            <Button
                loading=linking
                on:click={
                    let credit_provider = credit_provider.clone();
                    let credit_external_id = credit_external_id.clone();
                    let artist_name = artist_name.clone();
                    move |_| {
                        linking.set(true);
                        let credit_provider = credit_provider.clone();
                        let credit_external_id = credit_external_id.clone();
                        let artist_name = artist_name.clone();
                        let toaster = expect_toaster();
                        leptos::task::spawn_local(async move {
                            // 1. Link the provider to the existing artist (if we have provider info)
                            if let (Some(provider), Some(external_id)) = (credit_provider, credit_external_id)
                                && let Err(e) = dispatch_action(ServerAction::LinkArtistProvider {
                                    artist_id,
                                    provider,
                                    external_id,
                                    external_url: None,
                                    external_name: Some(artist_name.clone()),
                                    image_ref: None,
                                }).await
                            {
                                toaster.toast(
                                    ToastBuilder::new(format!("Error linking provider: {e}"))
                                        .with_level(ToastLevel::Error)
                                        .with_position(ToastPosition::BottomRight)
                                        .with_expiry(Some(8_000)),
                                );
                                linking.set(false);
                                return;
                            }
                            // 2. Add artist to the album
                            match dispatch_action(ServerAction::AddAlbumArtist {
                                album_id,
                                artist_id,
                            }).await {
                                Ok(()) => {
                                    toaster.toast(
                                        ToastBuilder::new(format!("{artist_name} linked to album"))
                                            .with_level(ToastLevel::Success)
                                            .with_position(ToastPosition::BottomRight)
                                            .with_expiry(Some(4_000)),
                                    );
                                    open.set(false);
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
                            linking.set(false);
                        });
                    }
                }
            >
                {move || if linking.get() { "Linking\u{2026}" } else { "Link" }}
            </Button>
        </DialogResultRow>
    }
}

// ── Row: provider search result ─────────────────────────────

#[component]
fn ProviderSearchRow(
    result: SearchArtistResult,
    #[allow(unused)] album_id: yoink_shared::Uuid,
    open: RwSignal<bool>,
) -> impl IntoView {
    let adding = RwSignal::new(false);

    let provider_display = provider_display_name(&result.provider);
    let provider_icon = provider_icon_svg(&result.provider);

    let disambiguation = result.disambiguation.clone();
    let artist_type = result.artist_type.clone();
    let country = result.country.clone();

    let subtitle: Option<String> = match (&disambiguation, &artist_type, &country) {
        (Some(d), _, _) => Some(d.clone()),
        (None, Some(t), Some(c)) => Some(format!("{t} from {c}")),
        (None, Some(t), None) => Some(t.clone()),
        (None, None, Some(c)) => Some(format!("from {c}")),
        (None, None, None) => None,
    };

    let provider = result.provider.clone();
    let external_id = result.external_id.clone();
    let external_url = result.url.clone();
    let name = result.name.clone();
    let label = "Add & Link";

    view! {
        <DialogResultRow>
            <ArtistAvatar name=result.name.clone() image_url=result.image_url.clone() />
            <div class="flex-1 min-w-0">
                <div class="flex items-center gap-1.5">
                    <span class="text-sm font-semibold text-zinc-900 dark:text-zinc-100 truncate">{result.name.clone()}</span>
                    <span class="inline-flex items-center gap-1 px-1.5 py-0.5 rounded text-[10px] font-medium bg-zinc-100 dark:bg-zinc-700/60 text-zinc-500 dark:text-zinc-400 shrink-0">
                        <span class="shrink-0 [&>svg]:size-2.5" inner_html=provider_icon></span>
                        {provider_display}
                    </span>

                </div>
                {subtitle.map(|s| view! {
                    <div class="text-[11px] text-zinc-500 dark:text-zinc-400 mt-0.5 truncate">{s}</div>
                })}
            </div>
            <Button
                variant=ButtonVariant::Primary
                size=ButtonSize::Md
                loading=adding
                on:click={
                    let provider = provider.clone();
                    let external_id = external_id.clone();
                    let external_url = external_url.clone();
                    let name = name.clone();
                    move |_| {
                        adding.set(true);
                        let provider = provider.clone();
                        let external_id = external_id.clone();
                        let external_url = external_url.clone();
                        let name = name.clone();
                        let toaster = expect_toaster();
                        leptos::task::spawn_local(async move {
                            // Add artist (creates new or finds existing)
                            let add_result = dispatch_action(ServerAction::AddArtist {
                                name: name.clone(),
                                provider,
                                external_id,
                                image_url: None,
                                external_url,
                            }).await;
                            match add_result {
                                Ok(()) => {
                                    toaster.toast(
                                        ToastBuilder::new(format!("{name} added"))
                                            .with_level(ToastLevel::Success)
                                            .with_position(ToastPosition::BottomRight)
                                            .with_expiry(Some(4_000)),
                                    );
                                    open.set(false);
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
                            adding.set(false);
                        });
                    }
                }
            >
                {move || if adding.get() { "Adding\u{2026}" } else { label }}
            </Button>
        </DialogResultRow>
    }
}
