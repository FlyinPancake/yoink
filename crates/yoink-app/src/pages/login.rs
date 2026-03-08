use leptos::prelude::*;

use crate::hooks::set_page_title;

const WRAP: &str = "min-h-screen bg-[radial-gradient(circle_at_top,_rgba(59,130,246,.12),_transparent_34%),linear-gradient(180deg,rgba(255,255,255,.96),rgba(244,244,245,.92))] dark:bg-[radial-gradient(circle_at_top,_rgba(59,130,246,.18),_transparent_28%),linear-gradient(180deg,rgba(9,9,11,.98),rgba(17,24,39,.96))] px-4 py-10";
const CARD: &str = "mx-auto w-full max-w-md rounded-[28px] border border-black/[.08] dark:border-white/[.08] bg-white/78 dark:bg-zinc-900/72 backdrop-blur-[22px] shadow-[0_24px_90px_rgba(15,23,42,.12)] dark:shadow-[0_28px_100px_rgba(0,0,0,.45)]";
const INPUT: &str = "w-full rounded-xl border border-black/[.08] dark:border-white/[.1] bg-white/80 dark:bg-zinc-950/65 px-3.5 py-3 text-sm text-zinc-900 dark:text-zinc-100 outline-none transition-[border-color,box-shadow] duration-150 focus:border-blue-500/50 focus:shadow-[0_0_0_3px_rgba(59,130,246,.14)]";
const LABEL: &str = "block text-[11px] font-semibold uppercase tracking-[0.18em] text-zinc-500 dark:text-zinc-400 mb-2";
const SUBMIT: &str = "inline-flex w-full items-center justify-center rounded-xl border border-blue-500 bg-blue-500 px-4 py-3 text-sm font-semibold text-white transition-all duration-150 hover:bg-blue-400 hover:border-blue-400 shadow-[0_12px_30px_rgba(59,130,246,.28)] cursor-pointer";

#[component]
pub fn LoginPage() -> impl IntoView {
    set_page_title("Login");
    let query = leptos_router::hooks::use_query_map();
    let error = query.read_untracked().get("error");
    let next = query
        .read_untracked()
        .get("next")
        .filter(|value| value.starts_with('/'))
        .unwrap_or_else(|| "/".to_string());

    view! {
        <div class=WRAP>
            <div class=CARD>
                <div class="px-6 pt-6 pb-5 border-b border-black/[.06] dark:border-white/[.06]">
                    <div class="flex items-center gap-3 mb-4">
                        <img src="/yoink.svg" alt="yoink" class="size-11 rounded-2xl shadow-[0_10px_24px_rgba(59,130,246,.15)]" />
                        <div>
                            <p class="text-[11px] uppercase tracking-[0.24em] text-blue-600 dark:text-blue-400 font-semibold m-0 mb-1">
                                "Single Admin"
                            </p>
                            <h1 class="text-[30px] leading-none font-bold text-zinc-900 dark:text-zinc-100 m-0">
                                "Sign In"
                            </h1>
                        </div>
                    </div>
                    <p class="text-sm leading-6 text-zinc-600 dark:text-zinc-300 m-0">
                        "Authenticate to access your library manager."
                    </p>
                </div>

                <div class="p-6">
                    {error.map(|message| view! { <crate::components::ErrorPanel message=message /> })}
                    <form method="post" action="/auth/login" class="space-y-4">
                        <input type="hidden" name="next" value=next />

                        <div>
                            <label class=LABEL for="login-username">"Username"</label>
                            <input id="login-username" class=INPUT type="text" name="username" autocomplete="username" required=true />
                        </div>

                        <div>
                            <label class=LABEL for="login-password">"Password"</label>
                            <input id="login-password" class=INPUT type="password" name="password" autocomplete="current-password" required=true />
                        </div>

                        <button type="submit" class=SUBMIT>"Sign In"</button>
                    </form>
                </div>
            </div>
        </div>
    }
}
