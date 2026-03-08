use leptos::prelude::*;

use super::ErrorPanel;

const INPUT: &str = "w-full rounded-lg border border-black/[.08] dark:border-white/[.1] bg-white/80 dark:bg-zinc-950/65 px-3.5 py-2.5 text-sm text-zinc-900 dark:text-zinc-100 placeholder:text-zinc-400 dark:placeholder:text-zinc-500 outline-none transition-[border-color,box-shadow,background] duration-150 focus:border-blue-500/50 focus:shadow-[0_0_0_3px_rgba(59,130,246,.14)]";
const LABEL: &str = "block text-sm font-medium text-zinc-700 dark:text-zinc-300 mb-1.5";
const SUBMIT: &str = "inline-flex w-full items-center justify-center rounded-lg border border-blue-500 bg-blue-500 px-4 py-2.5 text-sm font-semibold text-white transition-all duration-150 hover:bg-blue-400 hover:border-blue-400 shadow-[0_2px_12px_rgba(59,130,246,.25)] hover:shadow-[0_4px_20px_rgba(59,130,246,.35)] cursor-pointer";
const SUCCESS: &str = "mb-4 rounded-lg border border-emerald-500/20 bg-emerald-500/[.08] px-4 py-3 text-sm text-emerald-700 dark:text-emerald-300";

/// The inner form for updating admin credentials.
///
/// Renders error/success banners, form fields, and the submit button.
/// Does **not** render any card or panel wrapper — callers are responsible
/// for providing the surrounding chrome (e.g. [`Card`](super::Card) on
/// standalone pages, [`Panel`](super::Panel) inside the app shell).
#[component]
pub fn AuthCredentialsForm(
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
        <Show when=move || !error_visible.is_empty()>
            <ErrorPanel message=error_text.clone() />
        </Show>
        <Show when=move || !success_visible.is_empty()>
            <div class=SUCCESS>{success_text.clone()}</div>
        </Show>

        <form method="post" action=action class="flex flex-col gap-4">
            <div class="flex flex-col gap-1.5">
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
                <div class="flex flex-col gap-1.5">
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

            <div class="flex flex-col gap-1.5">
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

            <div class="flex flex-col gap-1.5">
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
    }
}
