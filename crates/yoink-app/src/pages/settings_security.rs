use leptos::prelude::*;

use crate::{
    components::{AuthCredentialsForm, PageHeader, PageShell},
    hooks::set_page_title,
};

const WRAP: &str = "min-h-screen bg-[radial-gradient(circle_at_top,_rgba(59,130,246,.12),_transparent_34%),linear-gradient(180deg,rgba(255,255,255,.96),rgba(244,244,245,.92))] dark:bg-[radial-gradient(circle_at_top,_rgba(59,130,246,.18),_transparent_28%),linear-gradient(180deg,rgba(9,9,11,.98),rgba(17,24,39,.96))] px-4 py-10";

#[component]
pub fn SetupPasswordPage() -> impl IntoView {
    set_page_title("Set Password");
    let query = leptos_router::hooks::use_query_map();
    let error = query.read_untracked().get("error");

    view! {
        <div class=WRAP>
            <div class="mx-auto flex min-h-[calc(100vh-5rem)] max-w-5xl items-center justify-center">
                <AuthCredentialsForm
                    title="Replace Bootstrap Password"
                    intro="This temporary admin password is only for first access. Set the permanent username and password you want to keep using."
                    action="/auth/credentials"
                    submit_label="Save Credentials"
                    error=error.unwrap_or_default()
                />
            </div>
        </div>
    }
}

#[component]
pub fn SecuritySettingsPage() -> impl IntoView {
    set_page_title("Security");
    let query = leptos_router::hooks::use_query_map();
    let error = query.read_untracked().get("error");
    let success = query
        .read_untracked()
        .get("success")
        .filter(|value| value == "1")
        .map(|_| "Credentials updated.".to_string());

    view! {
        <PageShell active="settings-security">
            <PageHeader title="Security"></PageHeader>
            <div class="p-6 max-md:p-4">
                <AuthCredentialsForm
                    title="Rotate Admin Credentials"
                    intro="Update the single admin username and password. This revokes every existing session and reissues the current one."
                    action="/auth/credentials"
                    submit_label="Update Credentials"
                    show_current_password=true
                    error=error.unwrap_or_default()
                    success=success.unwrap_or_default()
                />
            </div>
        </PageShell>
    }
}
