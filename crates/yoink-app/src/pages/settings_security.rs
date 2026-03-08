use leptos::prelude::*;

use crate::{
    components::{
        AuthCredentialsForm, Card, CardContent, CardDescription, CardHeader, CardTitle, PageHeader,
        PageShell, Panel, PanelBody, PanelHeader, PanelTitle,
    },
    hooks::set_page_title,
    styles::MUTED,
};

const WRAP: &str = "min-h-screen flex items-center justify-center bg-[radial-gradient(circle_at_top,_rgba(59,130,246,.12),_transparent_34%),linear-gradient(180deg,rgba(255,255,255,.96),rgba(244,244,245,.92))] dark:bg-[radial-gradient(circle_at_top,_rgba(59,130,246,.18),_transparent_28%),linear-gradient(180deg,rgba(9,9,11,.98),rgba(17,24,39,.96))] px-4 py-10";

#[component]
pub fn SetupPasswordPage() -> impl IntoView {
    set_page_title("Set Password");
    let query = leptos_router::hooks::use_query_map();
    let error = query.read_untracked().get("error");

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
                        <CardTitle>"Replace Bootstrap Password"</CardTitle>
                        <CardDescription>
                            "This temporary password is only for first access. Set the permanent username and password you want to keep using."
                        </CardDescription>
                    </CardHeader>
                    <CardContent>
                        <AuthCredentialsForm
                            action="/auth/credentials"
                            submit_label="Save Credentials"
                            error=error.unwrap_or_default()
                        />
                    </CardContent>
                </Card>
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
                <Panel>
                    <PanelHeader>
                        <PanelTitle>"Rotate Admin Credentials"</PanelTitle>
                    </PanelHeader>
                    <PanelBody>
                        <p class=move || crate::cls!(MUTED, "text-sm mb-4 m-0")>
                            "Update the single admin username and password. This revokes every existing session and reissues the current one."
                        </p>
                        <AuthCredentialsForm
                            action="/auth/credentials"
                            submit_label="Update Credentials"
                            show_current_password=true
                            error=error.unwrap_or_default()
                            success=success.unwrap_or_default()
                        />
                    </PanelBody>
                </Panel>
            </div>
        </PageShell>
    }
}
