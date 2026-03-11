use std::collections::HashMap;

use leptos::prelude::*;
use lucide_leptos::FolderOpen;

use yoink_shared::{ImportConfirmation, ImportPreviewItem, ImportResultSummary};

use crate::components::{
    Button, ButtonSize, ButtonVariant, ErrorPanel, Panel, PanelHeader, PanelTitle,
};
use crate::hooks::use_sse_version;
use crate::styles::EMPTY;

#[cfg(feature = "hydrate")]
use leptoaster::{ToastBuilder, ToastLevel, ToastPosition, expect_toaster};

use super::server_fns::{confirm_import_action, preview_import_library};
use super::shared::{ImportResultBanner, ImportRow, ImportStats, preview_counts};

#[component]
pub(super) fn LibraryScanTab() -> impl IntoView {
    let version = use_sse_version();
    let preview_data = Resource::new(move || version.get(), |_| preview_import_library());

    view! {
        <Transition fallback=move || view! {
            <Panel>
                <PanelHeader>
                    <div class="h-4 w-32 bg-zinc-200 dark:bg-zinc-700 rounded animate-pulse"></div>
                </PanelHeader>
                {(0..6).map(|_| view! {
                    <div class="flex items-center gap-3.5 px-5 py-3.5 border-b border-black/[.04] dark:border-white/[.04] animate-pulse">
                        <div class="size-12 rounded-lg bg-zinc-200 dark:bg-zinc-700 shrink-0"></div>
                        <div class="flex-1 min-w-0">
                            <div class="h-3.5 w-48 bg-zinc-200 dark:bg-zinc-700 rounded mb-2"></div>
                            <div class="h-3 w-32 bg-zinc-200 dark:bg-zinc-700 rounded"></div>
                        </div>
                        <div class="h-5 w-16 bg-zinc-200 dark:bg-zinc-700 rounded-full"></div>
                    </div>
                }).collect_view()}
            </Panel>
        }>
            {move || {
                preview_data.get().map(|result| match result {
                    Err(e) => view! {
                        <ErrorPanel
                            message="Failed to scan library."
                            details=e.to_string()
                            retry_href="/import"
                        />
                    }
                        .into_any(),
                    Ok(items) => view! { <ImportContent items=items /> }.into_any(),
                })
            }}
        </Transition>
    }
}

#[component]
fn ImportContent(items: Vec<ImportPreviewItem>) -> impl IntoView {
    let (total, matched_count, partial_count, unmatched_count, already_count) =
        preview_counts(&items);

    let items = StoredValue::new(items);
    let selections: RwSignal<HashMap<String, usize>> = RwSignal::new(items.with_value(|items| {
        items
            .iter()
            .filter_map(|item| item.selected_candidate.map(|idx| (item.id.clone(), idx)))
            .collect()
    }));
    let importing = RwSignal::new(false);
    let import_result: RwSignal<Option<ImportResultSummary>> = RwSignal::new(None);

    let select_all_matched = move |_: leptos::ev::MouseEvent| {
        selections.update(|selected| {
            items.with_value(|items| {
                for item in items {
                    if !item.already_imported && !item.candidates.is_empty() {
                        selected.insert(item.id.clone(), 0);
                    }
                }
            });
        });
    };

    let deselect_all = move |_: leptos::ev::MouseEvent| {
        selections.update(|selected| {
            items.with_value(|items| {
                for item in items {
                    if !item.already_imported && !item.candidates.is_empty() {
                        selected.remove(&item.id);
                    }
                }
            });
        });
    };

    let do_import = move |_: leptos::ev::MouseEvent| {
        importing.set(true);
        import_result.set(None);

        let confirmations: Vec<ImportConfirmation> = items.with_value(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let selected_idx =
                        selections.with(|selected| selected.get(&item.id).copied())?;
                    let candidate = item.candidates.get(selected_idx)?;
                    Some(ImportConfirmation {
                        preview_id: item.id.clone(),
                        artist_name: candidate.artist_name.clone(),
                        album_title: candidate.album_title.clone(),
                        year: candidate.release_date.clone(),
                        artist_id: Some(candidate.artist_id),
                        album_id: candidate.album_id,
                    })
                })
                .collect()
        });

        if confirmations.is_empty() {
            importing.set(false);
            return;
        }

        leptos::task::spawn_local(async move {
            match confirm_import_action(confirmations).await {
                Ok(summary) => {
                    #[cfg(feature = "hydrate")]
                    {
                        let toaster = expect_toaster();
                        if summary.failed == 0 {
                            toaster.toast(
                                ToastBuilder::new(format!("Imported {} albums", summary.imported))
                                    .with_level(ToastLevel::Success)
                                    .with_position(ToastPosition::BottomRight)
                                    .with_expiry(Some(4_000)),
                            );
                        } else {
                            toaster.toast(
                                ToastBuilder::new(format!(
                                    "Imported {}/{}. {} failed.",
                                    summary.imported, summary.total_selected, summary.failed
                                ))
                                .with_level(ToastLevel::Error)
                                .with_position(ToastPosition::BottomRight)
                                .with_expiry(Some(8_000)),
                            );
                        }
                    }
                    import_result.set(Some(summary));
                }
                Err(_e) => {
                    #[cfg(feature = "hydrate")]
                    {
                        let toaster = expect_toaster();
                        toaster.toast(
                            ToastBuilder::new(format!("Import error: {_e}"))
                                .with_level(ToastLevel::Error)
                                .with_position(ToastPosition::BottomRight)
                                .with_expiry(Some(8_000)),
                        );
                    }
                }
            }
            importing.set(false);
        });
    };

    view! {
        <ImportStats matched=matched_count partial=partial_count unmatched=unmatched_count already=already_count />
        <ImportResultBanner result=import_result />

        <Panel>
            <PanelHeader>
                <PanelTitle>{format!("{total} Discovered Albums")}</PanelTitle>
                <div class="flex flex-wrap items-center gap-2">
                    <Button on:click=select_all_matched>"Select All"</Button>
                    <Button on:click=deselect_all>"Deselect All"</Button>
                    <Button variant=ButtonVariant::Primary size=ButtonSize::Md loading=importing on:click=do_import>
                        {move || if importing.get() {
                            "Importing...".to_string()
                        } else {
                            "Import Selected".to_string()
                        }}
                    </Button>
                </div>
            </PanelHeader>

            {if total == 0 {
                view! {
                    <div class=EMPTY>
                        <div class="flex flex-col items-center gap-2">
                            <FolderOpen attr:class="size-8 text-zinc-300 dark:text-zinc-600" />
                            <span>"No album folders found in the music root directory."</span>
                        </div>
                    </div>
                }
                    .into_any()
            } else {
                view! {
                    <div>
                        {items.with_value(|items| {
                            items.iter().map(|item| {
                                view! { <ImportRow item=item.clone() selections=selections /> }
                            }).collect_view()
                        })}
                    </div>
                }
                    .into_any()
            }}
        </Panel>
    }
}
