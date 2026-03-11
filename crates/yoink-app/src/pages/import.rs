use leptos::prelude::*;
use lucide_leptos::{Check, FolderOpen, Music, Search};

use yoink_shared::{
    ImportAlbumCandidate, ImportConfirmation, ImportMatchStatus, ImportPreviewItem,
    ImportResultSummary,
};

use crate::components::{
    Badge, BadgeSize, BadgeVariant, Button, ButtonSize, ButtonVariant, ErrorPanel, PageHeader,
    PageShell, Panel, PanelHeader, PanelTitle,
};
use crate::hooks::{set_page_title, use_sse_version};
use crate::styles::EMPTY;

#[cfg(feature = "hydrate")]
use leptoaster::{ToastBuilder, ToastLevel, ToastPosition, expect_toaster};

// ── Page-specific Tailwind class constants ──────────────────

const IMPORT_ROW: &str = "flex items-center gap-3.5 px-5 py-3.5 border-b border-black/[.04] dark:border-white/[.04] transition-[background] duration-[120ms] last:border-b-0 hover:bg-blue-500/[.03] dark:hover:bg-blue-500/[.05]";
const IMPORT_ROW_SELECTED: &str = "flex items-center gap-3.5 px-5 py-3.5 border-b border-black/[.04] dark:border-white/[.04] transition-[background] duration-[120ms] last:border-b-0 bg-blue-500/[.04] dark:bg-blue-500/[.06]";
const IMPORT_ROW_IMPORTED: &str = "flex items-center gap-3.5 px-5 py-3.5 border-b border-black/[.04] dark:border-white/[.04] transition-[background] duration-[120ms] last:border-b-0 opacity-50";
const THUMB: &str = "size-12 rounded-lg object-cover shrink-0 bg-zinc-200 dark:bg-zinc-800 shadow-[0_2px_8px_rgba(0,0,0,.08)] dark:shadow-[0_2px_8px_rgba(0,0,0,.3)]";
const THUMB_FALLBACK: &str = "size-12 rounded-lg inline-flex items-center justify-center bg-zinc-200 dark:bg-zinc-800 text-zinc-400 dark:text-zinc-600 shrink-0";
const STAT_MINI: &str = "text-center px-3 py-2";
const STAT_MINI_LABEL: &str =
    "text-[11px] uppercase tracking-wide text-zinc-500 dark:text-zinc-400";
const STAT_MINI_VALUE: &str = "text-lg font-bold text-zinc-900 dark:text-zinc-100";

fn import_status_badge_variant(status: &ImportMatchStatus) -> BadgeVariant {
    match status {
        ImportMatchStatus::Matched => BadgeVariant::Success,
        ImportMatchStatus::Partial => BadgeVariant::Accent,
        ImportMatchStatus::Unmatched => BadgeVariant::Danger,
    }
}

// ── Server functions ────────────────────────────────────────

#[server(PreviewImportLibrary, "/leptos")]
pub async fn preview_import_library() -> Result<Vec<ImportPreviewItem>, ServerFnError> {
    let ctx = crate::actions::require_ctx()?;
    (ctx.preview_import)().await.map_err(ServerFnError::new)
}

#[server(
    name = ConfirmImportAction,
    prefix = "/leptos",
    input = server_fn::codec::Json,
    output = server_fn::codec::Json
)]
pub async fn confirm_import_action(
    items: Vec<ImportConfirmation>,
) -> Result<ImportResultSummary, ServerFnError> {
    let ctx = crate::actions::require_ctx()?;
    (ctx.confirm_import)(items)
        .await
        .map_err(ServerFnError::new)
}

// ── Page component ──────────────────────────────────────────

#[component]
pub fn ImportPage() -> impl IntoView {
    set_page_title("Import");
    let version = use_sse_version();
    let preview_data = Resource::new(move || version.get(), |_| preview_import_library());

    view! {
        <PageShell active="import">
                <Transition fallback=move || view! {
                    <div>
                        <PageHeader title="Manual Import"></PageHeader>
                        <div class="p-6 max-md:p-4">
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
                        </div>
                    </div>
                }>
                    {move || {
                        preview_data.get().map(|result| match result {
                            Err(e) => view! {
                                <div class="p-6 max-md:p-4">
                                    <ErrorPanel
                                        message="Failed to scan library."
                                        details=e.to_string()
                                        retry_href="/import"
                                    />
                                </div>
                            }.into_any(),
                            Ok(items) => {
                                view! { <ImportContent items=items /> }.into_any()
                            }
                        })
                    }}
                </Transition>
        </PageShell>
    }
}

/// Inner content rendered once preview data is loaded.
#[component]
fn ImportContent(items: Vec<ImportPreviewItem>) -> impl IntoView {
    let total = items.len();
    let matched_count = items
        .iter()
        .filter(|i| i.match_status == ImportMatchStatus::Matched && !i.already_imported)
        .count();
    let partial_count = items
        .iter()
        .filter(|i| i.match_status == ImportMatchStatus::Partial)
        .count();
    let unmatched_count = items
        .iter()
        .filter(|i| i.match_status == ImportMatchStatus::Unmatched)
        .count();
    let already_count = items.iter().filter(|i| i.already_imported).count();

    // Create reactive signals for each item's selection state
    let item_signals: Vec<(ImportPreviewItem, RwSignal<Option<usize>>)> = items
        .into_iter()
        .map(|item| {
            let selected = RwSignal::new(item.selected_candidate);
            (item, selected)
        })
        .collect();

    let item_signals = StoredValue::new(item_signals);

    let importing = RwSignal::new(false);
    let import_result: RwSignal<Option<ImportResultSummary>> = RwSignal::new(None);

    // Select/deselect all matched
    let select_all_matched = move |_: leptos::ev::MouseEvent| {
        item_signals.with_value(|items| {
            for (item, sig) in items {
                if !item.already_imported && !item.candidates.is_empty() {
                    sig.set(Some(0));
                }
            }
        });
    };

    let deselect_all = move |_: leptos::ev::MouseEvent| {
        item_signals.with_value(|items| {
            for (item, sig) in items {
                if !item.already_imported {
                    sig.set(None);
                }
            }
        });
    };

    let do_import = move |_: leptos::ev::MouseEvent| {
        importing.set(true);
        import_result.set(None);

        let confirmations: Vec<ImportConfirmation> = item_signals.with_value(|items| {
            items
                .iter()
                .filter_map(|(item, sig)| {
                    let selected_idx = sig.get_untracked()?;
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
                Err(e) => {
                    #[cfg(feature = "hydrate")]
                    {
                        let toaster = expect_toaster();
                        toaster.toast(
                            ToastBuilder::new(format!("Import error: {e}"))
                                .with_level(ToastLevel::Error)
                                .with_position(ToastPosition::BottomRight)
                                .with_expiry(Some(8_000)),
                        );
                    }
                    let _ = e;
                }
            }
            importing.set(false);
        });
    };

    view! {
        // Header bar
        <PageHeader title="Manual Import" subtitle=format!("{total} folders found")></PageHeader>

        // Content
        <div class="p-6 max-md:p-4">
            // Summary stats
            <div class="flex items-center gap-1 mb-4 bg-white/70 dark:bg-zinc-800/60 backdrop-blur-[12px] border border-black/[.06] dark:border-white/[.08] rounded-xl overflow-hidden">
                <div class=STAT_MINI>
                    <div class=STAT_MINI_VALUE>
                        <span class="text-green-600 dark:text-green-400">{matched_count}</span>
                    </div>
                    <div class=STAT_MINI_LABEL>"Matched"</div>
                </div>
                <div class="w-px h-8 bg-black/[.06] dark:bg-white/[.08]"></div>
                <div class=STAT_MINI>
                    <div class=STAT_MINI_VALUE>
                        <span class="text-amber-600 dark:text-amber-400">{partial_count}</span>
                    </div>
                    <div class=STAT_MINI_LABEL>"Partial"</div>
                </div>
                <div class="w-px h-8 bg-black/[.06] dark:bg-white/[.08]"></div>
                <div class=STAT_MINI>
                    <div class=STAT_MINI_VALUE>
                        <span class="text-red-600 dark:text-red-400">{unmatched_count}</span>
                    </div>
                    <div class=STAT_MINI_LABEL>"Unmatched"</div>
                </div>
                <div class="w-px h-8 bg-black/[.06] dark:bg-white/[.08]"></div>
                <div class=STAT_MINI>
                    <div class=STAT_MINI_VALUE>
                        <span class="text-zinc-400">{already_count}</span>
                    </div>
                    <div class=STAT_MINI_LABEL>"Already Imported"</div>
                </div>
            </div>

            // Import result banner
            <Show when=move || import_result.get().is_some()>
                {move || {
                    import_result.get().map(|summary| {
                        let is_success = summary.failed == 0;
                        let banner_class = if is_success {
                            "mb-4 px-5 py-3 rounded-xl border bg-green-500/[.06] border-green-500/20 text-green-700 dark:text-green-300 text-sm"
                        } else {
                            "mb-4 px-5 py-3 rounded-xl border bg-amber-500/[.06] border-amber-500/20 text-amber-700 dark:text-amber-300 text-sm"
                        };
                        view! {
                            <div class=banner_class>
                                <strong>
                                    {format!("Imported {} of {} albums", summary.imported, summary.total_selected)}
                                </strong>
                                {if summary.artists_added > 0 {
                                    format!(" ({} new artists added)", summary.artists_added)
                                } else {
                                    String::new()
                                }}
                                {if !summary.errors.is_empty() {
                                    view! {
                                        <ul class="mt-2 ml-4 text-xs list-disc">
                                            {summary.errors.iter().map(|e| view! {
                                                <li>{e.clone()}</li>
                                            }).collect_view()}
                                        </ul>
                                    }.into_any()
                                } else {
                                    view! { <span></span> }.into_any()
                                }}
                            </div>
                        }
                    })
                }}
            </Show>

            // Main table
            <Panel>
                <PanelHeader>
                    <PanelTitle>"Discovered Albums"</PanelTitle>
                    <div class="flex flex-wrap items-center gap-2">
                        <Button on:click=select_all_matched>
                            "Select All"
                        </Button>
                        <Button on:click=deselect_all>
                            "Deselect All"
                        </Button>
                        <Button variant=ButtonVariant::Primary size=ButtonSize::Md loading=importing
                            on:click=do_import>
                            {move || if importing.get() {
                                "Importing\u{2026}".to_string()
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
                    }.into_any()
                } else {
                    view! {
                        <div>
                            {item_signals.with_value(|items| {
                                items.iter().map(|(item, selected_sig)| {
                                    view! {
                                        <ImportRow
                                            item=item.clone()
                                            selected=*selected_sig
                                        />
                                    }
                                }).collect_view()
                            })}
                        </div>
                    }.into_any()
                }}
            </Panel>
        </div>
    }
}

/// A single import preview row.
#[component]
fn ImportRow(item: ImportPreviewItem, selected: RwSignal<Option<usize>>) -> impl IntoView {
    let match_status = item.match_status.clone();
    let is_already = item.already_imported;
    let candidates = item.candidates.clone();
    let has_candidates = !candidates.is_empty();
    let candidate_count = candidates.len();
    let candidates_stored = StoredValue::new(candidates);

    let row_class = move || {
        if is_already {
            IMPORT_ROW_IMPORTED
        } else if selected.get().is_some() {
            IMPORT_ROW_SELECTED
        } else {
            IMPORT_ROW
        }
    };

    // Status indicator color
    let status_dot_class = match &match_status {
        ImportMatchStatus::Matched => "size-2.5 rounded-full bg-green-500 shrink-0",
        ImportMatchStatus::Partial => "size-2.5 rounded-full bg-amber-500 shrink-0",
        ImportMatchStatus::Unmatched => "size-2.5 rounded-full bg-red-500 shrink-0",
    };

    let cover_url =
        candidates_stored.with_value(|c| c.first().and_then(|cand| cand.cover_url.clone()));

    let audio_count = item.audio_file_count;
    let discovered_artist = item.discovered_artist.clone();
    let discovered_album = item.discovered_album.clone();
    let discovered_year = item.discovered_year.clone();
    let relative_path = item.relative_path.clone();
    let relative_path_title = relative_path.clone();

    view! {
        <div class=row_class>
            // Selection checkbox (leading position)
            <div class="flex items-center shrink-0">
                {if !is_already && has_candidates {
                    let is_selected = move || selected.get().is_some();
                    view! {
                        <button type="button"
                            class=move || {
                                if is_selected() {
                                    "inline-flex items-center justify-center size-7 rounded-lg bg-blue-500 border border-blue-500 text-white cursor-pointer transition-all duration-150 hover:bg-blue-600 [&_svg]:size-3.5"
                                } else {
                                    "inline-flex items-center justify-center size-7 rounded-lg bg-white/50 dark:bg-zinc-800/50 border border-black/[.08] dark:border-white/10 text-zinc-400 cursor-pointer transition-all duration-150 hover:bg-blue-500 hover:border-blue-500 hover:text-white [&_svg]:size-3.5"
                                }
                            }
                            title=move || if is_selected() { "Deselect" } else { "Select for import" }
                            on:click=move |_| {
                                if selected.get().is_some() {
                                    selected.set(None);
                                } else if has_candidates {
                                    selected.set(Some(0));
                                }
                            }
                        >
                            <Check />
                        </button>
                    }.into_any()
                } else {
                    // Empty placeholder to keep alignment
                    view! { <div class="size-7"></div> }.into_any()
                }}
            </div>

            // Cover thumbnail
            {match cover_url {
                Some(url) => view! {
                    <img class=THUMB src=url alt="" />
                }.into_any(),
                None => view! {
                    <div class=THUMB_FALLBACK>
                        <Music attr:class="size-5" />
                    </div>
                }.into_any(),
            }}

            // Main info section
            <div class="flex-1 min-w-0">
                // Discovered folder info
                <div class="flex items-center gap-2 mb-1">
                    <div class=status_dot_class></div>
                    <span class="text-sm font-semibold text-zinc-900 dark:text-zinc-100 truncate">
                        {discovered_artist.clone()}
                    </span>
                    <span class="text-zinc-400 dark:text-zinc-500">" / "</span>
                    <span class="text-sm font-medium text-zinc-700 dark:text-zinc-300 truncate">
                        {discovered_album.clone()}
                    </span>
                    {discovered_year.map(|y| view! {
                        <span class="text-xs text-zinc-400 dark:text-zinc-500">
                            {format!("({})", y)}
                        </span>
                    })}
                </div>

                // Path and file info
                <div class="flex items-center gap-3 text-[11px] text-zinc-400 dark:text-zinc-500 mb-2">
                    <span class="truncate max-w-[400px]" title=relative_path_title>
                        {relative_path}
                    </span>
                    <span class="shrink-0">
                        {format!("{} tracks", audio_count)}
                    </span>
                </div>

                // Candidate selector (if any candidates exist and not already imported)
                {if has_candidates && !is_already {
                    view! {
                        <div class="mt-1">
                            <span class="text-[11px] uppercase tracking-wide text-zinc-500 dark:text-zinc-400 mb-1.5 block">
                                {if candidate_count == 1 { "Match:" } else { "Candidates:" }}
                            </span>
                            <div class="flex flex-col gap-1.5">
                                {candidates_stored.with_value(|candidates| {
                                    candidates.iter().enumerate().map(|(idx, cand)| {
                                        let cand = cand.clone();
                                        view! { <CandidateCard cand=cand idx=idx selected=selected /> }
                                    }).collect_view()
                                })}
                            </div>
                        </div>
                    }.into_any()
                } else if is_already {
                    view! {
                        <div class="text-[11px] text-green-600 dark:text-green-400 font-medium">
                            <Check attr:class="inline size-3 mr-1" />
                            "Already imported"
                        </div>
                    }.into_any()
                } else {
                    let search_name = discovered_artist.clone();
                    let search_url = format!(
                        "/library?q={}",
                        js_encode_uri_component(&search_name)
                    );
                    view! {
                        <div class="flex items-center gap-2">
                            <span class="text-[11px] text-red-500 dark:text-red-400">
                                "No match found"
                            </span>
                            <Button size=ButtonSize::Xs href=search_url>
                                <Search attr:class="size-3" />
                                "Add Artist"
                            </Button>
                        </div>
                    }.into_any()
                }}
            </div>

            // Right side: status pill
            <div class="flex items-center gap-2 shrink-0">
                <Badge size=BadgeSize::Pill variant=import_status_badge_variant(&match_status)>
                    {match_status.label()}
                </Badge>
            </div>
        </div>
    }
}

// ── Candidate card constants ────────────────────────────────

const CAND_BASE: &str = "flex items-center gap-2.5 px-3 py-2 rounded-lg border cursor-pointer transition-all duration-150";
const CAND_SELECTED: &str =
    "bg-blue-500/[.06] border-blue-500/30 dark:bg-blue-500/[.08] dark:border-blue-500/40";
const CAND_UNSELECTED: &str = "bg-white/40 border-black/[.06] dark:bg-zinc-800/40 dark:border-white/[.06] hover:border-blue-500/20 dark:hover:border-blue-500/30";
const CAND_THUMB: &str = "size-8 rounded object-cover shrink-0 bg-zinc-200 dark:bg-zinc-700";
const CAND_THUMB_FALLBACK: &str = "size-8 rounded inline-flex items-center justify-center bg-zinc-200 dark:bg-zinc-700 text-zinc-400 dark:text-zinc-500 shrink-0";

/// A rich candidate card showing cover art, album details, and confidence.
#[component]
fn CandidateCard(
    cand: ImportAlbumCandidate,
    idx: usize,
    selected: RwSignal<Option<usize>>,
) -> impl IntoView {
    let is_selected = move || selected.get() == Some(idx);

    let card_class = move || {
        if is_selected() {
            format!("{CAND_BASE} {CAND_SELECTED}")
        } else {
            format!("{CAND_BASE} {CAND_UNSELECTED}")
        }
    };

    let conf = cand.confidence;
    let conf_class = if conf >= 85 {
        "text-[11px] font-bold text-green-600 dark:text-green-400"
    } else if conf >= 60 {
        "text-[11px] font-bold text-amber-600 dark:text-amber-400"
    } else {
        "text-[11px] font-bold text-red-600 dark:text-red-400"
    };

    let album_type_label = cand
        .album_type
        .as_deref()
        .map(|t| yoink_shared::album_type_label(Some(t), &cand.album_title))
        .unwrap_or("Album");

    let cover = cand.cover_url.clone();
    let title = cand.album_title.clone();
    let artist = cand.artist_name.clone();
    let date = cand.release_date.clone();
    let is_explicit = cand.explicit;
    let is_new = cand.album_id.is_none();
    let is_acquired = cand.acquired;
    let is_monitored = cand.monitored;

    view! {
        <div
            class=card_class
            on:click=move |_| {
                if is_selected() {
                    selected.set(None);
                } else {
                    selected.set(Some(idx));
                }
            }
        >
            // Mini cover art
            {match cover {
                Some(url) => view! {
                    <img class=CAND_THUMB src=url alt="" />
                }.into_any(),
                None => view! {
                    <div class=CAND_THUMB_FALLBACK>
                        <Music attr:class="size-3.5" />
                    </div>
                }.into_any(),
            }}

            // Album info
            <div class="flex-1 min-w-0">
                <div class="flex items-center gap-1.5 flex-wrap">
                    <span class="text-xs font-semibold text-zinc-900 dark:text-zinc-100 truncate">
                        {title}
                    </span>
                    {date.map(|d| view! {
                        <span class="text-[11px] text-zinc-400 dark:text-zinc-500">{d}</span>
                    })}
                </div>
                <div class="flex items-center gap-1.5 mt-0.5 flex-wrap">
                    <span class="text-[10px] px-1.5 py-0.5 rounded bg-zinc-100 dark:bg-zinc-700/60 text-zinc-500 dark:text-zinc-400 font-medium">
                        {album_type_label}
                    </span>
                    {if is_explicit {
                        view! {
                            <span class="text-[10px] px-1 py-0.5 rounded bg-zinc-100 dark:bg-zinc-700/60 text-zinc-500 dark:text-zinc-400 font-medium">
                                "E"
                            </span>
                        }.into_any()
                    } else {
                        view! { <span></span> }.into_any()
                    }}
                    <span class="text-[10px] text-zinc-400 dark:text-zinc-500">{artist}</span>
                    {if is_new {
                        view! {
                            <span class="text-[10px] px-1.5 py-0.5 rounded bg-blue-500/10 text-blue-600 dark:text-blue-400 font-medium">
                                "New"
                            </span>
                        }.into_any()
                    } else if is_acquired {
                        view! {
                            <span class="text-[10px] px-1.5 py-0.5 rounded bg-green-500/10 text-green-600 dark:text-green-400 font-medium">
                                "Acquired"
                            </span>
                        }.into_any()
                    } else if is_monitored {
                        view! {
                            <span class="text-[10px] px-1.5 py-0.5 rounded bg-amber-500/10 text-amber-600 dark:text-amber-400 font-medium">
                                "Monitored"
                            </span>
                        }.into_any()
                    } else {
                        view! { <span></span> }.into_any()
                    }}
                </div>
            </div>

            // Confidence + selection indicator
            <div class="flex items-center gap-2 shrink-0">
                <span class=conf_class>{format!("{}%", conf)}</span>
                <div class=move || {
                    if is_selected() {
                        "size-4 rounded-full border-2 border-blue-500 bg-blue-500"
                    } else {
                        "size-4 rounded-full border-2 border-zinc-300 dark:border-zinc-600"
                    }
                }></div>
            </div>
        </div>
    }
}

/// Percent-encode a string for use in URL query parameters.
fn js_encode_uri_component(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            _ => {
                out.push_str(&format!("%{:02X}", b));
            }
        }
    }
    out
}
