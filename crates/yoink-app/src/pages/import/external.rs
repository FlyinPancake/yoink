use std::collections::HashMap;

use leptos::prelude::*;
use lucide_leptos::{ChevronRight, File, Folder, FolderOpen, Music, Search};

use yoink_shared::{
    BrowseEntry, ExternalImportConfirmation, ImportConfirmation, ImportPreviewItem,
    ImportResultSummary, ManualImportMode,
};

use crate::components::{
    Badge, BadgeSize, BadgeVariant, Button, ButtonSize, ButtonVariant, ErrorPanel, Panel,
    PanelBody, PanelHeader, PanelTitle,
};
use crate::hooks::use_sse_version;
use crate::styles::{EMPTY, SEARCH_INPUT};

#[cfg(feature = "hydrate")]
use leptoaster::{ToastBuilder, ToastLevel, ToastPosition, expect_toaster};

use super::server_fns::{
    browse_server_path, confirm_external_import_action, preview_external_import_action,
};
use super::shared::{ImportResultBanner, ImportRow, ImportStats, preview_counts};

#[derive(Clone, Copy, PartialEq, Eq)]
enum ExternalImportStep {
    Browse,
    Preview,
}

#[component]
pub(super) fn ExternalImportTab() -> impl IntoView {
    let step = RwSignal::new(ExternalImportStep::Browse);
    let current_path = RwSignal::new("/".to_string());
    let import_mode = RwSignal::new(ManualImportMode::Copy);
    let scanned_source = RwSignal::new(String::new());

    view! {
        {move || match step.get() {
            ExternalImportStep::Browse => view! {
                <PathBrowser
                    current_path=current_path
                    import_mode=import_mode
                    on_scan=move |path: String| {
                        scanned_source.set(path);
                        step.set(ExternalImportStep::Preview);
                    }
                />
            }
                .into_any(),
            ExternalImportStep::Preview => view! {
                <ExternalPreview
                    source_path=scanned_source.get_untracked()
                    import_mode=import_mode.get_untracked()
                    on_back=move || step.set(ExternalImportStep::Browse)
                />
            }
                .into_any(),
        }}
    }
}

#[component]
fn PathBrowser(
    current_path: RwSignal<String>,
    import_mode: RwSignal<ManualImportMode>,
    on_scan: impl Fn(String) + 'static + Clone + Send,
) -> impl IntoView {
    let path_input = RwSignal::new(current_path.get_untracked());
    let browsing = RwSignal::new(false);
    let browse_error: RwSignal<Option<String>> = RwSignal::new(None);
    let entries: RwSignal<Vec<BrowseEntry>> = RwSignal::new(Vec::new());

    let do_browse = move |path: String| {
        browsing.set(true);
        browse_error.set(None);
        let path_clone = path.clone();
        leptos::task::spawn_local(async move {
            match browse_server_path(path_clone.clone()).await {
                Ok(result) => {
                    entries.set(result);
                    current_path.set(path_clone.clone());
                    path_input.set(path_clone);
                }
                Err(e) => {
                    browse_error.set(Some(e.to_string()));
                    entries.set(Vec::new());
                }
            }
            browsing.set(false);
        });
    };

    do_browse(current_path.get_untracked());

    let on_scan_clone = on_scan.clone();
    let do_scan = move |_: leptos::ev::MouseEvent| {
        let path = current_path.get_untracked();
        on_scan_clone(path);
    };

    let navigate_to = move |path: String| {
        do_browse(path);
    };

    let on_path_submit = move |_: leptos::ev::MouseEvent| {
        do_browse(path_input.get_untracked());
    };

    let go_up = move |_: leptos::ev::MouseEvent| {
        let path = current_path.get_untracked();
        let parent = std::path::Path::new(&path)
            .parent()
            .map(|p| p.to_string_lossy().to_string())
            .unwrap_or_else(|| "/".to_string());
        do_browse(parent);
    };

    let audio_count = move || entries.get().iter().filter(|e| e.is_audio).count();
    let dir_count = move || entries.get().iter().filter(|e| e.is_dir).count();

    view! {
        <Panel>
            <PanelHeader>
                <PanelTitle>"Browse Server Path"</PanelTitle>
            </PanelHeader>
            <PanelBody>
                <div class="flex items-center gap-2 mb-4">
                    <input
                        type="text"
                        class=SEARCH_INPUT
                        prop:value=move || path_input.get()
                        on:input=move |ev| {
                            path_input.set(event_target_value(&ev));
                        }
                        on:keydown=move |ev: leptos::ev::KeyboardEvent| {
                            if ev.key() == "Enter" {
                                do_browse(path_input.get_untracked());
                            }
                        }
                        placeholder="Enter a server path, e.g. /mnt/music"
                    />
                    <Button on:click=on_path_submit>"Go"</Button>
                    <Button on:click=go_up>".."</Button>
                </div>

                <div class="flex items-center gap-3 mb-4">
                    <span class="text-xs font-medium text-zinc-500 dark:text-zinc-400">"Import mode:"</span>
                    <button
                        type="button"
                        class=move || {
                            if import_mode.get() == ManualImportMode::Copy {
                                "px-4 py-2 text-sm font-medium rounded-lg bg-blue-500/10 text-blue-600 dark:text-blue-400 border border-blue-500/20 cursor-pointer transition-all duration-150"
                            } else {
                                "px-4 py-2 text-sm font-medium rounded-lg text-zinc-500 dark:text-zinc-400 border border-transparent cursor-pointer transition-all duration-150 hover:text-zinc-700 dark:hover:text-zinc-300 hover:bg-zinc-100/50 dark:hover:bg-zinc-800/50"
                            }
                        }
                        on:click=move |_| import_mode.set(ManualImportMode::Copy)
                    >
                        "Copy"
                    </button>
                    <button
                        type="button"
                        class=move || {
                            if import_mode.get() == ManualImportMode::Hardlink {
                                "px-4 py-2 text-sm font-medium rounded-lg bg-blue-500/10 text-blue-600 dark:text-blue-400 border border-blue-500/20 cursor-pointer transition-all duration-150"
                            } else {
                                "px-4 py-2 text-sm font-medium rounded-lg text-zinc-500 dark:text-zinc-400 border border-transparent cursor-pointer transition-all duration-150 hover:text-zinc-700 dark:hover:text-zinc-300 hover:bg-zinc-100/50 dark:hover:bg-zinc-800/50"
                            }
                        }
                        on:click=move |_| import_mode.set(ManualImportMode::Hardlink)
                    >
                        "Hardlink"
                    </button>
                    <span class="text-[11px] text-zinc-400 dark:text-zinc-500">
                        {move || match import_mode.get() {
                            ManualImportMode::Copy => "Creates independent copies of each file.",
                            ManualImportMode::Hardlink => "Zero extra disk space. Falls back to copy if cross-device.",
                        }}
                    </span>
                </div>

                <Show when=move || browse_error.get().is_some()>
                    {move || browse_error.get().map(|err| view! {
                        <div class="mb-3 px-4 py-2.5 rounded-lg border bg-red-500/[.06] border-red-500/20 text-red-600 dark:text-red-400 text-sm">
                            {err}
                        </div>
                    })}
                </Show>

                <div class="flex items-center gap-1 mb-3 text-xs text-zinc-500 dark:text-zinc-400">
                    <Folder attr:class="size-3.5 shrink-0" />
                    <span class="font-mono truncate">{move || current_path.get()}</span>
                    <span class="ml-2 text-zinc-400 dark:text-zinc-500">
                        {move || format!("{} dirs, {} audio files", dir_count(), audio_count())}
                    </span>
                </div>
            </PanelBody>

            <Show when=move || browsing.get()>
                <div class="px-5 py-6 text-center text-sm text-zinc-400">"Loading..."</div>
            </Show>

            <Show when=move || !browsing.get()>
                {move || {
                    let items = entries.get();
                    if items.is_empty() {
                        view! { <div class=EMPTY>"Empty directory"</div> }.into_any()
                    } else {
                        view! {
                            <div class="max-h-[400px] overflow-y-auto">
                                {items.into_iter().map(|entry| {
                                    let entry_name = entry.name.clone();
                                    let entry_path = entry.path.clone();
                                    let is_dir = entry.is_dir;
                                    let is_audio = entry.is_audio;

                                    if is_dir {
                                        view! {
                                            <div
                                                class="flex items-center gap-2.5 px-4 py-2.5 border-b border-black/[.04] dark:border-white/[.04] cursor-pointer transition-[background] duration-[120ms] last:border-b-0 hover:bg-blue-500/[.03] dark:hover:bg-blue-500/[.05] text-sm"
                                                on:click=move |_| navigate_to(entry_path.clone())
                                            >
                                                <Folder attr:class="size-4 text-blue-500 dark:text-blue-400 shrink-0" />
                                                <span class="text-zinc-900 dark:text-zinc-100 font-medium truncate">{entry_name}</span>
                                                <ChevronRight attr:class="size-3.5 text-zinc-300 dark:text-zinc-600 ml-auto shrink-0" />
                                            </div>
                                        }
                                            .into_any()
                                    } else {
                                        let icon_class = if is_audio {
                                            "size-4 text-green-500 dark:text-green-400 shrink-0"
                                        } else {
                                            "size-4 text-zinc-300 dark:text-zinc-600 shrink-0"
                                        };
                                        view! {
                                            <div class="flex items-center gap-2.5 px-4 py-2 border-b border-black/[.04] dark:border-white/[.04] last:border-b-0 text-sm text-zinc-400 dark:text-zinc-500">
                                                <File attr:class=icon_class />
                                                <span class="truncate">{entry_name}</span>
                                                {if is_audio {
                                                    view! {
                                                        <Music attr:class="size-3 text-green-500/50 ml-auto shrink-0" />
                                                    }
                                                        .into_any()
                                                } else {
                                                    view! { <span></span> }.into_any()
                                                }}
                                            </div>
                                        }
                                            .into_any()
                                    }
                                }).collect_view()}
                            </div>
                        }
                            .into_any()
                    }
                }}
            </Show>

            <PanelBody>
                <div class="flex items-center justify-end gap-3 pt-2">
                    <Button variant=ButtonVariant::Primary size=ButtonSize::Md on:click=do_scan>
                        <Search attr:class="size-4" />
                        "Scan This Folder"
                    </Button>
                </div>
            </PanelBody>
        </Panel>
    }
}

#[component]
fn ExternalPreview(
    source_path: String,
    import_mode: ManualImportMode,
    on_back: impl Fn() + 'static + Send,
) -> impl IntoView {
    let version = use_sse_version();
    let source_path_stored = StoredValue::new(source_path.clone());
    let mode_stored = StoredValue::new(import_mode);
    let preview_data = Resource::new(
        move || version.get(),
        move |_| {
            let path = source_path_stored.get_value();
            preview_external_import_action(path)
        },
    );

    view! {
        <div class="mb-4">
            <Button on:click=move |_| on_back()>"Back to Browser"</Button>
        </div>

        <Transition fallback=move || view! {
            <Panel>
                <PanelHeader>
                    <div class="h-4 w-32 bg-zinc-200 dark:bg-zinc-700 rounded animate-pulse"></div>
                </PanelHeader>
                {(0..4).map(|_| view! {
                    <div class="flex items-center gap-3.5 px-5 py-3.5 border-b border-black/[.04] dark:border-white/[.04] animate-pulse">
                        <div class="size-12 rounded-lg bg-zinc-200 dark:bg-zinc-700 shrink-0"></div>
                        <div class="flex-1 min-w-0">
                            <div class="h-3.5 w-48 bg-zinc-200 dark:bg-zinc-700 rounded mb-2"></div>
                            <div class="h-3 w-32 bg-zinc-200 dark:bg-zinc-700 rounded"></div>
                        </div>
                    </div>
                }).collect_view()}
            </Panel>
        }>
            {move || {
                preview_data.get().map(|result| match result {
                    Err(e) => view! {
                        <ErrorPanel
                            message="Failed to scan external path."
                            details=e.to_string()
                            retry_href="/import"
                        />
                    }
                        .into_any(),
                    Ok(items) => {
                        let source = source_path_stored.get_value();
                        let mode = mode_stored.get_value();
                        view! {
                            <ExternalImportContent items=items source_path=source import_mode=mode />
                        }
                            .into_any()
                    }
                })
            }}
        </Transition>
    }
}

#[component]
fn ExternalImportContent(
    items: Vec<ImportPreviewItem>,
    source_path: String,
    import_mode: ManualImportMode,
) -> impl IntoView {
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
    let source_stored = StoredValue::new(source_path.clone());
    let mode_stored = StoredValue::new(import_mode);

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

        let confirmation = ExternalImportConfirmation {
            source_path: source_stored.get_value(),
            mode: mode_stored.get_value(),
            items: confirmations,
        };

        leptos::task::spawn_local(async move {
            match confirm_external_import_action(confirmation).await {
                Ok(summary) => {
                    #[cfg(feature = "hydrate")]
                    {
                        let toaster = expect_toaster();
                        if summary.failed == 0 {
                            toaster.toast(
                                ToastBuilder::new(format!(
                                    "Imported {} albums via {}",
                                    summary.imported,
                                    mode_stored.get_value().label().to_lowercase()
                                ))
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
        <div class="flex items-center gap-2 mb-4 px-4 py-2.5 rounded-xl border bg-zinc-100/50 dark:bg-zinc-800/40 border-black/[.04] dark:border-white/[.06] text-sm">
            <Folder attr:class="size-4 text-zinc-400 shrink-0" />
            <span class="font-mono text-zinc-600 dark:text-zinc-300 truncate">{source_path}</span>
            <Badge size=BadgeSize::Pill variant=BadgeVariant::Accent>
                {import_mode.label()}
            </Badge>
        </div>

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
                            <span>"No album folders found at this path."</span>
                        </div>
                    </div>
                }
                    .into_any()
            } else {
                view! {
                    <div>
                        {items.with_value(|items| {
                            items
                                .iter()
                                .map(|item| {
                                    view! { <ImportRow item=item.clone() selections=selections /> }
                                })
                                .collect_view()
                        })}
                    </div>
                }
                    .into_any()
            }}
        </Panel>
    }
}
