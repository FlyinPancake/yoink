use leptos::prelude::*;
use lucide_leptos::{
    FolderDown, Headphones, Heart, House, KeyRound, LogOut, Menu, Search, SunMoon, X,
};

use crate::hooks::use_auth_enabled;

// ── Mobile menu context ─────────────────────────────────────

#[derive(Clone, Copy)]
struct MobileMenuOpen(RwSignal<bool>);

/// Context provider for the sidebar's mobile-menu state.
///
/// Wrap this around any tree that contains both `<Sidebar/>` and
/// `<MobileMenuButton/>` so they share the same `RwSignal<bool>`.
/// Typically placed once in `<App/>`.
#[component]
pub fn SidebarProvider(children: Children) -> impl IntoView {
    provide_context(MobileMenuOpen(RwSignal::new(false)));
    children()
}

/// Button that opens the mobile sidebar drawer.
/// Place this inside header bars. Hidden on desktop (md+).
#[component]
pub fn MobileMenuButton() -> impl IntoView {
    let open = expect_context::<MobileMenuOpen>().0;

    view! {
        <button type="button"
            class="md:hidden inline-flex items-center justify-center size-9 rounded-lg bg-white/60 dark:bg-zinc-800/60 backdrop-blur-[12px] border border-black/[.08] dark:border-white/10 text-zinc-600 dark:text-zinc-300 cursor-pointer transition-all duration-150 hover:bg-white/85 dark:hover:bg-zinc-800/85 hover:text-zinc-900 dark:hover:text-white [&_svg]:size-5 shrink-0 mr-1"
            on:click=move |_| open.set(true)
            aria-label="Open menu"
        >
            <Menu />
        </button>
    }
}

// ── Tailwind class constants ────────────────────────────────

const NAV_BASE: &str = "flex items-center gap-3 py-2.5 px-4 text-zinc-400/90 no-underline text-sm font-medium border-l-3 border-transparent transition-[background,color,border-color] duration-150 hover:bg-white/[.04] hover:text-zinc-200 [&_svg]:size-[18px] [&_svg]:shrink-0";
const NAV_ACTIVE: &str = "flex items-center gap-3 py-2.5 px-4 text-zinc-100 no-underline text-sm font-medium border-l-3 border-blue-500 bg-blue-500/[.08] transition-[background,color,border-color] duration-150 hover:bg-white/[.04] hover:text-zinc-200 [&_svg]:size-[18px] [&_svg]:shrink-0";
const NAV_CHILD_BASE: &str = "flex items-center gap-2 py-1.5 pr-4 pl-11 text-zinc-400/90 no-underline text-xs font-medium border-l-3 border-transparent transition-[background,color,border-color] duration-150 hover:bg-white/[.03] hover:text-zinc-200";
const NAV_CHILD_ACTIVE: &str = "flex items-center gap-2 py-1.5 pr-4 pl-11 text-zinc-100 no-underline text-xs font-semibold border-l-3 border-blue-500 bg-blue-500/[.06] transition-[background,color,border-color] duration-150";

// ── Theme toggle helpers (client-only) ──────────────────────

/// Read the current dark-mode state from the DOM.
/// On the server this always returns `true` (the bootstrap script defaults to dark).
fn read_is_dark() -> bool {
    #[cfg(feature = "hydrate")]
    {
        leptos::prelude::document()
            .document_element()
            .map(|el| el.class_list().contains("dark"))
            .unwrap_or(true)
    }
    #[cfg(not(feature = "hydrate"))]
    {
        true
    }
}

/// Toggle dark mode: flip the `dark` class on `<html>` and persist to localStorage.
#[cfg(feature = "hydrate")]
fn toggle_dark_mode() -> bool {
    let doc_el = leptos::prelude::document()
        .document_element()
        .expect("missing <html>");
    let class_list = doc_el.class_list();
    let _ = class_list.toggle("dark");
    let is_dark = class_list.contains("dark");

    if let Ok(Some(storage)) = leptos::prelude::window().local_storage() {
        let _ = storage.set_item("theme", if is_dark { "dark" } else { "light" });
    }
    is_dark
}

/// Shared sidebar navigation content (used by both desktop and mobile).
#[component]
fn SidebarNav(
    dashboard_class: &'static str,
    library_class: &'static str,
    library_artists_class: &'static str,
    library_albums_class: &'static str,
    library_tracks_class: &'static str,
    search_class: &'static str,
    wanted_class: &'static str,
    import_class: &'static str,
    security_class: &'static str,
    auth_enabled: Signal<bool>,
    theme_label: Signal<&'static str>,
    on_toggle: impl Fn(leptos::ev::MouseEvent) + 'static + Clone + Send + Sync,
    #[prop(optional)] on_nav_click: Option<Box<dyn Fn() + Send + Sync>>,
) -> impl IntoView {
    let on_nav_click = StoredValue::new(on_nav_click);

    let nav_click = move |_: leptos::ev::MouseEvent| {
        on_nav_click.with_value(|cb| {
            if let Some(f) = cb {
                f();
            }
        });
    };

    view! {
        <div class="px-4 pt-5 pb-3 flex items-center gap-2.5 border-b border-white/[.06]">
            <img src="/yoink.svg" alt="yoink" class="size-8 shrink-0" />
            <span class="text-lg font-bold text-zinc-100 tracking-wide">"yoink"</span>
        </div>
        <nav class="flex-1 py-2" aria-label="Main navigation">
            <a href="/" class=dashboard_class on:click=nav_click>
                <House />
                "Dashboard"
            </a>
            <a href="/library/artists" class=library_class on:click=nav_click>
                <Headphones />
                "Library"
            </a>
            <a href="/library/artists" class=library_artists_class on:click=nav_click>
                "Artists"
            </a>
            <a href="/library/albums" class=library_albums_class on:click=nav_click>
                "Albums"
            </a>
            <a href="/library/tracks" class=library_tracks_class on:click=nav_click>
                "Tracks"
            </a>
            <a href="/search" class=search_class on:click=nav_click>
                <Search />
                "Search"
            </a>
            <a href="/wanted" class=wanted_class on:click=nav_click>
                <Heart />
                "Wanted"
            </a>
            <a href="/import" class=import_class on:click=nav_click>
                <FolderDown />
                "Import"
            </a>
        </nav>
        <div class="px-4 py-3 border-t border-white/[.06]">
            <span class="block text-[11px] uppercase tracking-wider font-semibold text-zinc-500 px-1 pb-1 mb-1">
                "Settings"
            </span>
            <Show when=move || auth_enabled.get()>
                <div class="mb-1">
                    <a href="/settings/security" class=security_class on:click=nav_click>
                        <KeyRound />
                        "Security"
                    </a>
                </div>
                <form method="post" action="/auth/logout">
                    <button type="submit"
                        class="flex items-center gap-2.5 w-full bg-transparent border-none text-red-400/90 font-inherit text-[13px] cursor-pointer py-2 px-1 rounded-md transition-[background,color] duration-150 hover:bg-red-500/[.08] hover:text-red-300 [&_svg]:size-4 [&_svg]:shrink-0"
                        on:click=nav_click
                    >
                        <LogOut />
                        "Logout"
                    </button>
                </form>
            </Show>
            <button type="button"
                class="flex items-center gap-2.5 w-full bg-transparent border-none text-zinc-400/90 font-inherit text-[13px] cursor-pointer py-2 px-1 rounded-md transition-[background,color] duration-150 hover:bg-white/[.04] hover:text-zinc-200 [&_svg]:size-4 [&_svg]:shrink-0"
                on:click=on_toggle
            >
                <SunMoon />
                <span>{theme_label}</span>
            </button>
        </div>
    }
}

/// Navigation sidebar — shared across all Leptos-rendered pages.
///
/// On desktop (md+) the sidebar is a fixed panel on the left.
/// On mobile (<md) a hamburger button opens a slide-in drawer overlay.
#[component]
pub fn Sidebar(#[prop(into)] active: String) -> impl IntoView {
    let dashboard_class = if active == "dashboard" {
        NAV_ACTIVE
    } else {
        NAV_BASE
    };
    let library_class = if active.starts_with("library") {
        NAV_ACTIVE
    } else {
        NAV_BASE
    };
    let library_artists_class = if active == "library-artists" {
        NAV_CHILD_ACTIVE
    } else {
        NAV_CHILD_BASE
    };
    let library_albums_class = if active == "library-albums" {
        NAV_CHILD_ACTIVE
    } else {
        NAV_CHILD_BASE
    };
    let library_tracks_class = if active == "library-tracks" {
        NAV_CHILD_ACTIVE
    } else {
        NAV_CHILD_BASE
    };
    let search_class = if active == "search" {
        NAV_ACTIVE
    } else {
        NAV_BASE
    };
    let wanted_class = if active == "wanted" {
        NAV_ACTIVE
    } else {
        NAV_BASE
    };
    let import_class = if active == "import" {
        NAV_ACTIVE
    } else {
        NAV_BASE
    };
    let security_class = if active == "settings-security" {
        NAV_ACTIVE
    } else {
        NAV_BASE
    };
    let auth_enabled = use_auth_enabled();

    // Theme state — initialised from DOM on the client (after hydration),
    // defaults to `true` (dark) during SSR to match the bootstrap script.
    #[cfg(not(feature = "hydrate"))]
    let (is_dark, _) = signal(read_is_dark());

    #[cfg(feature = "hydrate")]
    let (is_dark, set_is_dark) = signal(read_is_dark());

    // Sync on mount: re-read from DOM after hydration in case the bootstrap
    // script set a different state than the SSR default.
    // Also listen for OS theme changes and sync when no explicit override.
    #[cfg(feature = "hydrate")]
    {
        Effect::new(move |_| {
            set_is_dark.set(read_is_dark());
        });

        // OS theme sync: listen to prefers-color-scheme changes.
        // Only apply when the user hasn't explicitly set a theme in localStorage.
        {
            use wasm_bindgen::JsCast;
            use wasm_bindgen::prelude::*;

            let win = leptos::prelude::window();
            if let Ok(Some(mql)) = win.match_media("(prefers-color-scheme: dark)") {
                let handler = Closure::<dyn Fn(web_sys::Event)>::new(move |_ev: web_sys::Event| {
                    // If user has an explicit localStorage theme, don't override
                    let has_explicit = leptos::prelude::window()
                        .local_storage()
                        .ok()
                        .flatten()
                        .and_then(|s| s.get_item("theme").ok().flatten())
                        .is_some();
                    if has_explicit {
                        return;
                    }
                    // Sync dark class from OS preference
                    let prefers_dark = leptos::prelude::window()
                        .match_media("(prefers-color-scheme: dark)")
                        .ok()
                        .flatten()
                        .map(|m| m.matches())
                        .unwrap_or(false);
                    if let Some(doc_el) = leptos::prelude::document().document_element() {
                        let cl = doc_el.class_list();
                        if prefers_dark {
                            let _ = cl.add_1("dark");
                        } else {
                            let _ = cl.remove_1("dark");
                        }
                    }
                    set_is_dark.set(prefers_dark);
                });
                let _ = mql
                    .add_event_listener_with_callback("change", handler.as_ref().unchecked_ref());
                handler.forget(); // keep listener alive
            }
        }
    }

    let theme_label = Signal::derive(move || if is_dark.get() { "Dark" } else { "Light" });

    let on_toggle = move |_: leptos::ev::MouseEvent| {
        #[cfg(feature = "hydrate")]
        {
            let new_dark = toggle_dark_mode();
            set_is_dark.set(new_dark);
        }
    };

    // Mobile drawer state — read from SidebarProvider context
    let mobile_open = expect_context::<MobileMenuOpen>().0;

    let close_drawer = move || mobile_open.set(false);

    // Lock body scroll when mobile drawer is open (ref-counted with ConfirmDialog)
    Effect::new(move || {
        let is_open = mobile_open.get();
        #[cfg(feature = "hydrate")]
        {
            use crate::components::confirm_dialog::scroll_lock;
            if is_open {
                scroll_lock::acquire();
            } else {
                scroll_lock::release();
            }
        }
        let _ = is_open; // suppress unused warning on SSR
    });

    view! {
        // ── Desktop sidebar (hidden on mobile) ──────────────
        <aside class="fixed inset-y-0 left-0 w-[220px] bg-[rgba(10,10,15,.92)] backdrop-blur-[20px] border-r border-white/[.06] flex flex-col z-50 overflow-y-auto max-md:hidden" aria-label="Sidebar">
            <SidebarNav
                dashboard_class=dashboard_class
                library_class=library_class
                library_artists_class=library_artists_class
                library_albums_class=library_albums_class
                library_tracks_class=library_tracks_class
                search_class=search_class
                wanted_class=wanted_class
                import_class=import_class
                security_class=security_class
                auth_enabled=Signal::derive(move || auth_enabled.get())
                theme_label=theme_label
                on_toggle=on_toggle
            />
        </aside>



        // ── Mobile drawer overlay ───────────────────────────
        <Show when=move || mobile_open.get()>
            // Backdrop
            <div
                class="fixed inset-0 z-[70] bg-black/50 backdrop-blur-sm md:hidden transition-opacity duration-200"
                on:click=move |_| mobile_open.set(false)
            ></div>
            // Drawer
            <aside class="fixed inset-y-0 left-0 w-[260px] bg-[rgba(10,10,15,.96)] backdrop-blur-[20px] border-r border-white/[.06] flex flex-col z-[80] overflow-y-auto md:hidden animate-[slide-in-left_200ms_ease-out]"
                role="dialog" aria-modal="true" aria-label="Navigation menu"
            >
                // Close button inside drawer
                <div class="absolute top-3 right-3">
                    <button type="button"
                        class="inline-flex items-center justify-center size-8 rounded-lg bg-white/[.04] border border-white/[.08] text-zinc-400 cursor-pointer transition-all duration-150 hover:bg-white/[.08] hover:text-white [&_svg]:size-4"
                        on:click=move |_| mobile_open.set(false)
                        aria-label="Close menu"
                    >
                        <X />
                    </button>
                </div>
                <SidebarNav
                    dashboard_class=dashboard_class
                    library_class=library_class
                    library_artists_class=library_artists_class
                    library_albums_class=library_albums_class
                    library_tracks_class=library_tracks_class
                    search_class=search_class
                    wanted_class=wanted_class
                    import_class=import_class
                    security_class=security_class
                    auth_enabled=Signal::derive(move || auth_enabled.get())
                    theme_label=theme_label
                    on_toggle=on_toggle
                    on_nav_click=Box::new(close_drawer)
                />
            </aside>
        </Show>
    }
}
