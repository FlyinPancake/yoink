use leptos::prelude::*;
use lucide_leptos::{ChevronDown, ChevronUp, RefreshCw, TriangleAlert};

use super::Button;

/// A user-friendly error panel with optional details toggle and retry.
///
/// `message` is the friendly text shown to the user.
/// `details` is the raw error string, hidden behind a "Details" toggle.
/// If `retry_href` is provided, a "Retry" link is shown that navigates there.
#[component]
pub fn ErrorPanel(
    /// Friendly message shown to the user.
    #[prop(into)]
    message: String,
    /// Raw error details (shown behind a toggle).
    #[prop(into, optional)]
    details: Option<String>,
    /// If provided, show a "Retry" link pointing at this URL.
    #[prop(into, optional)]
    retry_href: Option<String>,
) -> impl IntoView {
    let (show_details, set_show_details) = signal(false);
    let has_details = details.is_some();
    let has_actions = has_details || retry_href.is_some();

    view! {
        <div class="rounded-xl border border-red-500/20 bg-red-500/[.06] dark:bg-red-500/[.08] p-5 mb-6">
            <div class="flex items-center gap-3">
                <div class="shrink-0 text-red-500 dark:text-red-400 [&_svg]:size-5" aria-hidden="true">
                    <TriangleAlert />
                </div>
                <div class="flex-1 min-w-0">
                    <p class="text-sm font-medium text-red-700 dark:text-red-300 m-0">{message}</p>
                    {if has_actions {
                        view! {
                            <div class="flex flex-wrap items-center gap-2 mt-2">
                                {retry_href.map(|href| view! {
                                    <Button href=href>
                                        <RefreshCw size=12 />
                                        "Retry"
                                    </Button>
                                })}
                                {if has_details {
                                    view! {
                                        <button type="button"
                                            class="inline-flex items-center gap-1 text-xs text-zinc-500 dark:text-zinc-400 hover:text-zinc-700 dark:hover:text-zinc-200 cursor-pointer bg-transparent border-none p-0 font-inherit [&_svg]:size-3.5"
                                            on:click=move |_| set_show_details.update(|v| *v = !*v)
                                            attr:aria-expanded=move || show_details.get().to_string()
                                        >
                                            {move || if show_details.get() { "Hide details" } else { "Show details" }}
                                            <Show when=move || show_details.get() fallback=|| view! { <ChevronDown /> }>
                                                <ChevronUp />
                                            </Show>
                                        </button>
                                    }.into_any()
                                } else {
                                    ().into_any()
                                }}
                            </div>
                        }.into_any()
                    } else {
                        ().into_any()
                    }}
                    {if has_details {
                        view! {
                            <Show when=move || show_details.get()>
                                <pre class="mt-3 p-3 rounded-lg bg-red-500/[.06] dark:bg-red-500/[.08] border border-red-500/10 text-[11px] text-red-600 dark:text-red-400 overflow-x-auto whitespace-pre-wrap break-words m-0 font-mono">
                                    {details.clone().unwrap_or_default()}
                                </pre>
                            </Show>
                        }.into_any()
                    } else {
                        ().into_any()
                    }}
                </div>
            </div>
        </div>
    }
}

#[cfg(all(test, feature = "ssr"))]
mod tests {
    use super::*;
    use leptos::prelude::{Owner, RenderHtml};

    fn render_message_only(message: &str) -> String {
        Owner::new().with(|| view! { <ErrorPanel message=message.to_string() /> }.to_html())
    }

    fn render_with_retry(message: &str, retry_href: &str) -> String {
        Owner::new().with(|| {
            view! { <ErrorPanel message=message.to_string() retry_href=retry_href.to_string() /> }
                .to_html()
        })
    }

    fn render_with_details(message: &str, details: &str) -> String {
        Owner::new().with(|| {
            view! { <ErrorPanel message=message.to_string() details=details.to_string() /> }
                .to_html()
        })
    }

    #[test]
    fn renders_message_and_retry_link_when_provided() {
        let html = render_with_retry("Something failed", "/retry");

        assert!(html.contains("Something failed"));
        assert!(html.contains("href=\"/retry\""));
        assert!(html.contains(">Retry<"));
    }

    #[test]
    fn renders_details_toggle_but_hides_details_by_default() {
        let html = render_with_details("Something failed", "stack trace line");

        assert!(html.contains("Show details"));
        assert!(html.contains("aria-expanded=\"false\""));
        assert!(!html.contains("stack trace line"));
    }

    #[test]
    fn does_not_render_toggle_when_details_missing() {
        let html = render_message_only("Something failed");

        assert!(!html.contains("Show details"));
        assert!(!html.contains("Hide details"));
        assert!(!html.contains("aria-expanded="));
    }
}
