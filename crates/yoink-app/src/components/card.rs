use leptos::prelude::*;

const CARD: &str = "rounded-2xl border border-black/[.08] dark:border-white/[.08] bg-white/78 dark:bg-zinc-900/72 backdrop-blur-[20px] shadow-[0_20px_80px_rgba(15,23,42,.12)] dark:shadow-[0_24px_90px_rgba(0,0,0,.45)] overflow-hidden";
const CARD_HEADER: &str = "flex flex-col items-center text-center gap-2 px-6 pt-6 pb-4";
const CARD_TITLE: &str = "text-2xl font-bold text-zinc-900 dark:text-zinc-100 m-0";
const CARD_DESCRIPTION: &str = "text-sm text-zinc-500 dark:text-zinc-400 m-0";
const CARD_CONTENT: &str = "px-6 pb-6";

/// A standalone card container with heavy shadow and backdrop blur.
///
/// Designed for prominent standalone surfaces (login, setup pages).
/// For in-app content panels, prefer [`Panel`](super::Panel) instead.
#[component]
pub fn Card(#[prop(optional, into)] class: String, children: Children) -> impl IntoView {
    view! {
        <div class=move || crate::cls!(CARD, class.as_str())>{children()}</div>
    }
}

#[component]
pub fn CardHeader(#[prop(optional, into)] class: String, children: Children) -> impl IntoView {
    view! {
        <div class=move || crate::cls!(CARD_HEADER, class.as_str())>{children()}</div>
    }
}

#[component]
pub fn CardTitle(#[prop(optional, into)] class: String, children: Children) -> impl IntoView {
    view! {
        <h1 class=move || crate::cls!(CARD_TITLE, class.as_str())>{children()}</h1>
    }
}

#[component]
pub fn CardDescription(#[prop(optional, into)] class: String, children: Children) -> impl IntoView {
    view! {
        <p class=move || crate::cls!(CARD_DESCRIPTION, class.as_str())>{children()}</p>
    }
}

#[component]
pub fn CardContent(#[prop(optional, into)] class: String, children: Children) -> impl IntoView {
    view! {
        <div class=move || crate::cls!(CARD_CONTENT, class.as_str())>{children()}</div>
    }
}
