use leptos::prelude::*;

use super::ErrorPanel;

const CARD: &str = "w-full max-w-md rounded-[28px] border border-black/[.08] dark:border-white/[.08] bg-white/78 dark:bg-zinc-900/72 backdrop-blur-[20px] shadow-[0_20px_80px_rgba(15,23,42,.12)] dark:shadow-[0_24px_90px_rgba(0,0,0,.45)]";
const INPUT: &str = "w-full rounded-xl border border-black/[.08] dark:border-white/[.1] bg-white/80 dark:bg-zinc-950/65 px-3.5 py-3 text-sm text-zinc-900 dark:text-zinc-100 outline-none transition-[border-color,box-shadow,background] duration-150 focus:border-blue-500/50 focus:shadow-[0_0_0_3px_rgba(59,130,246,.14)]";
const LABEL: &str = "block text-[11px] font-semibold uppercase tracking-[0.18em] text-zinc-500 dark:text-zinc-400 mb-2";
const SUBMIT: &str = "inline-flex w-full items-center justify-center rounded-xl border border-blue-500 bg-blue-500 px-4 py-3 text-sm font-semibold text-white transition-all duration-150 hover:bg-blue-400 hover:border-blue-400 shadow-[0_12px_30px_rgba(59,130,246,.28)] cursor-pointer";
const SUCCESS: &str = "mb-5 rounded-xl border border-emerald-500/20 bg-emerald-500/[.08] px-4 py-3 text-sm text-emerald-700 dark:text-emerald-300";

#[component]
pub fn AuthCredentialsForm(
    #[prop(into)] title: String,
    #[prop(into)] intro: String,
    #[prop(into)] action: String,
    #[prop(into)] submit_label: String,
    #[prop(optional)] show_current_password: bool,
    #[prop(into, optional)] error: String,
    #[prop(into, optional)] success: String,
) -> impl IntoView {
    let error_visible = error.clone();
    let error_text = error;
    let success_visible = success.clone();
    let success_text = success;

    view! {
        <section class=CARD>
            <div class="px-6 pt-6 pb-5 border-b border-black/[.06] dark:border-white/[.06]">
                <p class="text-[11px] uppercase tracking-[0.24em] text-blue-600 dark:text-blue-400 font-semibold m-0 mb-2">
                    "Authentication"
                </p>
                <h1 class="text-[28px] leading-none font-bold text-zinc-900 dark:text-zinc-100 m-0 mb-3">
                    {title}
                </h1>
                <p class="text-sm leading-6 text-zinc-600 dark:text-zinc-300 m-0">
                    {intro}
                </p>
            </div>

            <div class="p-6">
                <Show when=move || !error_visible.is_empty()>
                    <ErrorPanel message=error_text.clone() />
                </Show>
                <Show when=move || !success_visible.is_empty()>
                    <div class=SUCCESS>{success_text.clone()}</div>
                </Show>

                <form method="post" action=action class="space-y-4">
                    <div>
                        <label class=LABEL for="auth-username">
                            "Username"
                        </label>
                        <input
                            id="auth-username"
                            class=INPUT
                            type="text"
                            name="username"
                            autocomplete="username"
                            required=true
                        />
                    </div>

                    <Show when=move || show_current_password>
                        <div>
                            <label class=LABEL for="auth-current-password">
                                "Current Password"
                            </label>
                            <input
                                id="auth-current-password"
                                class=INPUT
                                type="password"
                                name="current_password"
                                autocomplete="current-password"
                                required=show_current_password
                            />
                        </div>
                    </Show>

                    <div>
                        <label class=LABEL for="auth-new-password">
                            "New Password"
                        </label>
                        <input
                            id="auth-new-password"
                            class=INPUT
                            type="password"
                            name="new_password"
                            autocomplete="new-password"
                            required=true
                        />
                    </div>

                    <div>
                        <label class=LABEL for="auth-confirm-password">
                            "Confirm Password"
                        </label>
                        <input
                            id="auth-confirm-password"
                            class=INPUT
                            type="password"
                            name="confirm_password"
                            autocomplete="new-password"
                            required=true
                        />
                    </div>

                    <button type="submit" class=SUBMIT>
                        {submit_label}
                    </button>
                </form>
            </div>
        </section>
    }
}
