use leptos::portal::Portal;
use leptos::prelude::*;
use lucide_leptos::{Check, ChevronDown};

// ── Style constants ────────────────────────────────────────

const TRIGGER: &str = "\
    flex w-fit items-center justify-between gap-2 rounded-lg \
    border border-black/[.06] dark:border-white/[.08] \
    bg-white/40 dark:bg-zinc-800/40 \
    px-2.5 py-1.5 text-xs whitespace-nowrap \
    text-zinc-900 dark:text-zinc-100 \
    cursor-pointer select-none outline-none \
    transition-[color,border-color,box-shadow] duration-150 \
    hover:border-blue-500/30 dark:hover:border-blue-500/40 \
    focus-visible:border-blue-500 focus-visible:ring-[3px] focus-visible:ring-blue-500/15 \
    disabled:cursor-not-allowed disabled:opacity-50";

/// The dropdown starts invisible (`opacity:0`) and is revealed by JS
/// once positioning is resolved, preventing the single-frame flash at
/// the wrong position.
const CONTENT: &str = "\
    fixed z-[9999] w-max \
    rounded-lg \
    border border-black/[.06] dark:border-white/[.08] \
    bg-white dark:bg-zinc-800 \
    text-zinc-900 dark:text-zinc-100 \
    shadow-lg dark:shadow-[0_8px_32px_rgba(0,0,0,.4)] \
    p-1";

const ITEM: &str = "\
    relative flex w-full cursor-pointer items-center \
    rounded-md py-1.5 pr-8 pl-2 text-xs whitespace-nowrap \
    outline-none select-none border-none bg-transparent \
    text-zinc-900 dark:text-zinc-100 \
    transition-colors duration-100 \
    hover:bg-blue-500/[.08] dark:hover:bg-blue-500/[.12] \
    hover:text-blue-600 dark:hover:text-blue-400";

const ITEM_ACTIVE: &str = "\
    relative flex w-full cursor-pointer items-center \
    rounded-md py-1.5 pr-8 pl-2 text-xs whitespace-nowrap \
    outline-none select-none border-none bg-transparent \
    transition-colors duration-100 \
    bg-blue-500/[.06] dark:bg-blue-500/[.10] \
    text-zinc-900 dark:text-zinc-100 \
    hover:bg-blue-500/[.08] dark:hover:bg-blue-500/[.12] \
    hover:text-blue-600 dark:hover:text-blue-400";

const INDICATOR: &str =
    "absolute right-2 flex size-3.5 items-center justify-center text-blue-500 dark:text-blue-400";

// ── Viewport-aware positioning (hydrate-only) ──────────────

/// Padding (px) kept between the dropdown edge and the viewport edge.
#[cfg(feature = "hydrate")]
const VIEWPORT_PADDING: f64 = 8.0;

/// Gap (px) between trigger and dropdown.
#[cfg(feature = "hydrate")]
const TRIGGER_GAP: f64 = 4.0;

/// Measure the trigger rect and the dropdown rect, then position the
/// dropdown so it stays fully inside the viewport.  Tries below-left
/// first, then shifts horizontally / flips vertically as needed.
#[cfg(feature = "hydrate")]
fn position_dropdown(trigger: &web_sys::Element, dropdown: &web_sys::HtmlElement) -> String {
    let tr = trigger.get_bounding_client_rect();
    let dr = dropdown.get_bounding_client_rect();
    let vw = web_sys::window()
        .and_then(|w| w.inner_width().ok())
        .and_then(|v| v.as_f64())
        .unwrap_or(1024.0);
    let vh = web_sys::window()
        .and_then(|w| w.inner_height().ok())
        .and_then(|v| v.as_f64())
        .unwrap_or(768.0);

    let dw = dr.width();
    let dh = dr.height();

    // ── Horizontal: prefer left-aligned with trigger ───────
    let mut left = tr.left();
    // If it overflows the right edge, shift left.
    if left + dw + VIEWPORT_PADDING > vw {
        left = vw - dw - VIEWPORT_PADDING;
    }
    // Never go past the left edge.
    if left < VIEWPORT_PADDING {
        left = VIEWPORT_PADDING;
    }

    // ── Vertical: prefer below trigger ─────────────────────
    let below = tr.bottom() + TRIGGER_GAP;
    let above = tr.top() - TRIGGER_GAP - dh;

    let top = if below + dh + VIEWPORT_PADDING <= vh {
        // Fits below.
        below
    } else if above >= VIEWPORT_PADDING {
        // Flip above.
        above
    } else {
        // Neither fits perfectly — pick the side with more room.
        if (vh - tr.bottom()) >= tr.top() {
            below
        } else {
            above.max(VIEWPORT_PADDING)
        }
    };

    format!("top:{top:.0}px;left:{left:.0}px;")
}

// ── Types ──────────────────────────────────────────────────

/// A single option inside a [`Select`] dropdown.
#[derive(Clone)]
pub struct SelectOption<T: 'static> {
    /// The value emitted via `on_change` when this option is picked.
    pub value: T,
    /// Human-readable label displayed in the trigger and dropdown.
    pub label: String,
}

/// Describes an optional group separator between items.
///
/// `Select` can render a thin horizontal rule between the "default"
/// slot and the main options list when `separator_after` is set.
#[derive(Clone)]
pub struct SelectGroup<T: 'static> {
    pub options: Vec<SelectOption<T>>,
    /// When `true`, a visual separator is rendered after this group.
    pub separator_after: bool,
}

// ── Component ──────────────────────────────────────────────

/// A generic, portal-based select dropdown styled after shadcn/ui.
///
/// Works with any `T: Clone + Copy + PartialEq + Send + Sync + 'static`.
/// The dropdown is rendered via `<Portal>` into `<body>` so it escapes
/// any ancestor `overflow-hidden` / stacking contexts.  Positioning is
/// viewport-aware: the dropdown shifts horizontally and flips vertically
/// to stay fully visible.
#[component]
pub fn Select<T>(
    /// The currently selected value.
    selected: T,
    /// Display text shown on the trigger button.
    display_text: String,
    /// Groups of options to display. Groups are separated by a thin rule
    /// when `separator_after` is `true`.
    groups: Vec<SelectGroup<T>>,
    /// Called when the user picks a different option.
    on_change: Callback<T>,
    /// Optional leading icon rendered before the display text in the
    /// trigger button (e.g. `<ArrowUpDown size=14 />`).
    #[prop(optional)]
    icon: Option<Children>,
) -> impl IntoView
where
    T: Clone + Copy + PartialEq + Send + Sync + 'static,
{
    let open = RwSignal::new(false);
    let pos_style = RwSignal::new(String::new());
    let trigger_ref = NodeRef::<leptos::html::Button>::new();
    let dropdown_ref = NodeRef::<leptos::html::Div>::new();

    // Store groups in a StoredValue so they can be read from Fn closures
    // without Send+Sync issues from pre-built view trees.
    let groups = StoredValue::new(groups);

    let do_open = move || {
        // We cannot measure the dropdown before it is in the DOM, so we
        // set a temporary off-screen position, flip `open` to `true`,
        // and schedule the real measurement for the next animation frame.
        pos_style.set("top:-9999px;left:-9999px;".into());
        open.set(true);

        #[cfg(feature = "hydrate")]
        {
            use wasm_bindgen::prelude::*;

            let trigger_ref = trigger_ref;
            let dropdown_ref = dropdown_ref;
            let pos_style = pos_style;

            // requestAnimationFrame so the browser has laid out the dropdown.
            let cb = Closure::once_into_js(move || {
                if let (Some(trigger_el), Some(dd_el)) = (trigger_ref.get(), dropdown_ref.get()) {
                    let trigger: &web_sys::Element = trigger_el.as_ref();
                    let dropdown: &web_sys::HtmlElement = dd_el.as_ref();
                    let style = position_dropdown(trigger, dropdown);
                    pos_style.set(style);
                }
            });
            if let Some(w) = web_sys::window() {
                let _ = w.request_animation_frame(cb.as_ref().unchecked_ref());
            }
        }
    };

    view! {
        // ── Trigger ────────────────────────────────
        <button
            type="button"
            class=TRIGGER
            node_ref=trigger_ref
            aria-haspopup="listbox"
            aria-expanded=move || if open.get() { "true" } else { "false" }
            on:click=move |_| {
                if open.get_untracked() { open.set(false) } else { do_open() }
            }
            on:keydown=move |ev| {
                let key = ev.key();
                match key.as_str() {
                    "Escape" => open.set(false),
                    "Enter" | " " | "ArrowDown" => {
                        ev.prevent_default();
                        if !open.get_untracked() { do_open(); }
                    }
                    _ => {}
                }
            }
        >
            {icon.map(|children| view! {
                <span class="shrink-0 opacity-50 flex items-center [&_svg]:size-3.5">
                    {children()}
                </span>
            })}
            <span class="line-clamp-1">{display_text}</span>
            <span
                class="shrink-0 opacity-50 transition-transform duration-150 flex items-center"
                style=move || if open.get() { "transform:rotate(180deg)" } else { "" }
            >
                <ChevronDown size=14 />
            </span>
        </button>

        // ── Portal: backdrop + dropdown rendered at <body> level ──
        <Portal>
            <Show when=move || open.get()>
                // Backdrop
                <div
                    class="fixed inset-0 z-[9998]"
                    on:click=move |_| open.set(false)
                ></div>
                // Dropdown — views are built inside the closure from stored data.
                <div
                    node_ref=dropdown_ref
                    class=CONTENT
                    role="listbox"
                    style=move || {
                        let pos = pos_style.get();
                        format!("{pos}animation:select-in 120ms ease-out")
                    }
                >
                    {groups.with_value(|gs| {
                        gs.iter().enumerate().map(|(gi, group)| {
                            let separator = group.separator_after;
                            let items = group.options.iter().map(|opt| {
                                let is_selected = opt.value == selected;
                                let value = opt.value;
                                let label = opt.label.clone();
                                view! {
                                    <button
                                        type="button"
                                        role="option"
                                        aria-selected=if is_selected { "true" } else { "false" }
                                        class=move || if is_selected { ITEM_ACTIVE } else { ITEM }
                                        on:click=move |_| {
                                            on_change.run(value);
                                            open.set(false);
                                        }
                                        on:keydown=move |ev| {
                                            if ev.key() == "Escape" { open.set(false); }
                                        }
                                    >
                                        {label}
                                        {if is_selected {
                                            view! {
                                                <span class=INDICATOR>
                                                    <Check size=14 />
                                                </span>
                                            }.into_any()
                                        } else {
                                            view! { <span></span> }.into_any()
                                        }}
                                    </button>
                                }
                            }).collect_view();

                            let sep = if separator {
                                view! {
                                    <div class="my-1 -mx-1 h-px bg-black/[.06] dark:bg-white/[.06]"></div>
                                }.into_any()
                            } else {
                                let _ = gi;
                                view! { <span></span> }.into_any()
                            };

                            view! {
                                {items}
                                {sep}
                            }
                        }).collect_view()
                    })}
                </div>
            </Show>
        </Portal>
    }
}
