use leptos::prelude::*;

use crate::{
    components::{Card, CardContent, CardDescription, CardHeader, CardTitle, ErrorPanel},
    hooks::set_page_title,
};

const WRAP: &str = "min-h-screen flex items-center justify-center bg-[radial-gradient(circle_at_top,_rgba(59,130,246,.12),_transparent_34%),linear-gradient(180deg,rgba(255,255,255,.96),rgba(244,244,245,.92))] dark:bg-[radial-gradient(circle_at_top,_rgba(59,130,246,.18),_transparent_28%),linear-gradient(180deg,rgba(9,9,11,.98),rgba(17,24,39,.96))] px-4 py-10";
const INPUT: &str = "w-full rounded-lg border border-black/[.08] dark:border-white/[.1] bg-white/80 dark:bg-zinc-950/65 px-3.5 py-2.5 text-sm text-zinc-900 dark:text-zinc-100 placeholder:text-zinc-400 dark:placeholder:text-zinc-500 outline-none transition-[border-color,box-shadow] duration-150 focus:border-blue-500/50 focus:shadow-[0_0_0_3px_rgba(59,130,246,.14)]";
const LABEL: &str = "block text-sm font-medium text-zinc-700 dark:text-zinc-300 mb-1.5";
const SUBMIT: &str = "inline-flex w-full items-center justify-center rounded-lg border border-blue-500 bg-blue-500 px-4 py-2.5 text-sm font-semibold text-white transition-all duration-150 hover:bg-blue-400 hover:border-blue-400 shadow-[0_2px_12px_rgba(59,130,246,.25)] hover:shadow-[0_4px_20px_rgba(59,130,246,.35)] cursor-pointer";

#[component]
pub fn LoginPage() -> impl IntoView {
    set_page_title("Login");
    let query = leptos_router::hooks::use_query_map();
    let error = query.read_untracked().get("error");
    let next = query
        .read_untracked()
        .get("next")
        .filter(|value| value.starts_with('/') && !value.starts_with("//"))
        .unwrap_or_else(|| "/".to_string());

    view! {
        <div class=WRAP>
            <div class="flex flex-col gap-6 w-full max-w-md">
                <Card>
                    <CardHeader>
                        <img
                            src="/yoink.svg"
                            alt="yoink"
                            class="size-10 rounded-xl shadow-[0_8px_20px_rgba(59,130,246,.12)]"
                        />
                        <CardTitle>"Sign in to yoink"</CardTitle>
                        <CardDescription>
                            "Enter your credentials to access your library"
                        </CardDescription>
                    </CardHeader>
                    <CardContent>
                        <form method="post" action="/auth/login" class="flex flex-col gap-4">
                            <input type="hidden" name="next" value=next />
                            {error.map(|message| view! { <ErrorPanel message=message /> })}

                            <div class="flex flex-col gap-1.5">
                                <label class=LABEL for="login-username">"Username"</label>
                                <input
                                    id="login-username"
                                    class=INPUT
                                    type="text"
                                    name="username"
                                    autocomplete="username"
                                    required=true
                                />
                            </div>

                            <div class="flex flex-col gap-1.5">
                                <label class=LABEL for="login-password">"Password"</label>
                                <input
                                    id="login-password"
                                    class=INPUT
                                    type="password"
                                    name="password"
                                    autocomplete="current-password"
                                    required=true
                                />
                            </div>

                            <button type="submit" class=SUBMIT>"Sign In"</button>
                        </form>
                    </CardContent>
                </Card>
            </div>
        </div>
    }
}
