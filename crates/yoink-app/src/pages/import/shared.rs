use std::collections::HashMap;

use leptos::prelude::*;
use lucide_leptos::{Check, Music, Search};

use yoink_shared::{ImportAlbumCandidate, ImportPreviewItem, ImportResultSummary};

use crate::components::{AddArtistDialog, Badge, BadgeSize, BadgeVariant, Button, ButtonSize};

pub(super) fn preview_counts(items: &[ImportPreviewItem]) -> (usize, usize, usize, usize, usize) {
    let total = items.len();
    let matched = items
        .iter()
        .filter(|i| {
            i.match_status == yoink_shared::ImportMatchStatus::Matched && !i.already_imported
        })
        .count();
    let partial = items
        .iter()
        .filter(|i| i.match_status == yoink_shared::ImportMatchStatus::Partial)
        .count();
    let unmatched = items
        .iter()
        .filter(|i| i.match_status == yoink_shared::ImportMatchStatus::Unmatched)
        .count();
    let already = items.iter().filter(|i| i.already_imported).count();

    (total, matched, partial, unmatched, already)
}

#[component]
pub(super) fn ImportStats(
    matched: usize,
    partial: usize,
    unmatched: usize,
    already: usize,
) -> impl IntoView {
    view! {
        <div class="flex items-center gap-1 mb-4 bg-white/70 dark:bg-zinc-800/60 backdrop-blur-[12px] border border-black/[.06] dark:border-white/[.08] rounded-xl overflow-hidden">
            <div class="text-center px-3 py-2">
                <div class="text-lg font-bold text-zinc-900 dark:text-zinc-100">
                    <span class="text-green-600 dark:text-green-400">{matched}</span>
                </div>
                <div class="text-[11px] uppercase tracking-wide text-zinc-500 dark:text-zinc-400">"Matched"</div>
            </div>
            <div class="w-px h-8 bg-black/[.06] dark:bg-white/[.08]"></div>
            <div class="text-center px-3 py-2">
                <div class="text-lg font-bold text-zinc-900 dark:text-zinc-100">
                    <span class="text-amber-600 dark:text-amber-400">{partial}</span>
                </div>
                <div class="text-[11px] uppercase tracking-wide text-zinc-500 dark:text-zinc-400">"Partial"</div>
            </div>
            <div class="w-px h-8 bg-black/[.06] dark:bg-white/[.08]"></div>
            <div class="text-center px-3 py-2">
                <div class="text-lg font-bold text-zinc-900 dark:text-zinc-100">
                    <span class="text-red-600 dark:text-red-400">{unmatched}</span>
                </div>
                <div class="text-[11px] uppercase tracking-wide text-zinc-500 dark:text-zinc-400">"Unmatched"</div>
            </div>
            <div class="w-px h-8 bg-black/[.06] dark:bg-white/[.08]"></div>
            <div class="text-center px-3 py-2">
                <div class="text-lg font-bold text-zinc-900 dark:text-zinc-100">
                    <span class="text-zinc-400">{already}</span>
                </div>
                <div class="text-[11px] uppercase tracking-wide text-zinc-500 dark:text-zinc-400">"Already Imported"</div>
            </div>
        </div>
    }
}

#[component]
pub(super) fn ImportResultBanner(result: RwSignal<Option<ImportResultSummary>>) -> impl IntoView {
    view! {
        <Show when=move || result.get().is_some()>
            {move || {
                result.get().map(|summary| {
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
                                        {summary.errors.iter().map(|e| view! { <li>{e.clone()}</li> }).collect_view()}
                                    </ul>
                                }
                                    .into_any()
                            } else {
                                view! { <span></span> }.into_any()
                            }}
                        </div>
                    }
                })
            }}
        </Show>
    }
}

#[component]
pub(super) fn ImportRow(
    item: ImportPreviewItem,
    selections: RwSignal<HashMap<String, usize>>,
) -> impl IntoView {
    let item_id = item.id.clone();
    let match_status = item.match_status.clone();
    let is_already = item.already_imported;
    let candidates = item.candidates.clone();
    let has_candidates = !candidates.is_empty();
    let candidate_count = candidates.len();
    let candidates_stored = StoredValue::new(candidates);

    let row_class = move || {
        if is_already {
            "flex items-center gap-3.5 px-5 py-3.5 border-b border-black/[.04] dark:border-white/[.04] transition-[background] duration-[120ms] last:border-b-0 opacity-50"
        } else if selections.with(|selected| selected.contains_key(&item_id)) {
            "flex items-center gap-3.5 px-5 py-3.5 border-b border-black/[.04] dark:border-white/[.04] transition-[background] duration-[120ms] last:border-b-0 bg-blue-500/[.04] dark:bg-blue-500/[.06]"
        } else {
            "flex items-center gap-3.5 px-5 py-3.5 border-b border-black/[.04] dark:border-white/[.04] transition-[background] duration-[120ms] last:border-b-0 hover:bg-blue-500/[.03] dark:hover:bg-blue-500/[.05]"
        }
    };

    let status_dot_class = match &match_status {
        yoink_shared::ImportMatchStatus::Matched => "size-2.5 rounded-full bg-green-500 shrink-0",
        yoink_shared::ImportMatchStatus::Partial => "size-2.5 rounded-full bg-amber-500 shrink-0",
        yoink_shared::ImportMatchStatus::Unmatched => "size-2.5 rounded-full bg-red-500 shrink-0",
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
            <div class="flex items-center shrink-0">
                {if !is_already && has_candidates {
                    let class_item_id = item.id.clone();
                    let title_item_id = item.id.clone();
                    let click_item_id = item.id.clone();
                    view! {
                        <button
                            type="button"
                            class=move || {
                                if selections.with(|selected| selected.contains_key(&class_item_id)) {
                                    "inline-flex items-center justify-center size-7 rounded-lg bg-blue-500 border border-blue-500 text-white cursor-pointer transition-all duration-150 hover:bg-blue-600 [&_svg]:size-3.5"
                                } else {
                                    "inline-flex items-center justify-center size-7 rounded-lg bg-white/50 dark:bg-zinc-800/50 border border-black/[.08] dark:border-white/10 text-zinc-400 cursor-pointer transition-all duration-150 hover:bg-blue-500 hover:border-blue-500 hover:text-white [&_svg]:size-3.5"
                                }
                            }
                            title=move || if selections.with(|selected| selected.contains_key(&title_item_id)) { "Deselect" } else { "Select for import" }
                            on:click=move |_| {
                                if selections.with(|selected| selected.contains_key(&click_item_id)) {
                                    selections.update(|selected| {
                                        selected.remove(&click_item_id);
                                    });
                                } else if has_candidates {
                                    selections.update(|selected| {
                                        selected.insert(click_item_id.clone(), 0);
                                    });
                                }
                            }
                        >
                            <Check />
                        </button>
                    }
                        .into_any()
                } else {
                    view! { <div class="size-7"></div> }.into_any()
                }}
            </div>

            {match cover_url {
                Some(url) => view! {
                    <img class="size-12 rounded-lg object-cover shrink-0 bg-zinc-200 dark:bg-zinc-800 shadow-[0_2px_8px_rgba(0,0,0,.08)] dark:shadow-[0_2px_8px_rgba(0,0,0,.3)]" src=url alt="" />
                }.into_any(),
                None => view! {
                    <div class="size-12 rounded-lg inline-flex items-center justify-center bg-zinc-200 dark:bg-zinc-800 text-zinc-400 dark:text-zinc-600 shrink-0">
                        <Music attr:class="size-5" />
                    </div>
                }
                    .into_any(),
            }}

            <div class="flex-1 min-w-0">
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
                        <span class="text-xs text-zinc-400 dark:text-zinc-500">{format!("({})", y)}</span>
                    })}
                </div>

                <div class="flex items-center gap-3 text-[11px] text-zinc-400 dark:text-zinc-500 mb-2">
                    <span class="truncate max-w-[400px]" title=relative_path_title>
                        {relative_path}
                    </span>
                    <span class="shrink-0">{format!("{} tracks", audio_count)}</span>
                </div>

                {if has_candidates && !is_already {
                    view! {
                        <div class="mt-1">
                            <span class="text-[11px] uppercase tracking-wide text-zinc-500 dark:text-zinc-400 mb-1.5 block">
                                {if candidate_count == 1 { "Match:" } else { "Candidates:" }}
                            </span>
                            <div class="flex flex-col gap-1.5">
                                {candidates_stored.with_value(|candidates| {
                                    candidates
                                        .iter()
                                        .enumerate()
                                        .map(|(idx, cand)| {
                                            let cand = cand.clone();
                                            view! { <CandidateCard cand=cand idx=idx item_id=item.id.clone() selections=selections /> }
                                        })
                                        .collect_view()
                                })}
                            </div>
                        </div>
                    }
                        .into_any()
                } else if is_already {
                    view! {
                        <div class="text-[11px] text-green-600 dark:text-green-400 font-medium">
                            <Check attr:class="inline size-3 mr-1" />
                            "Already imported"
                        </div>
                    }
                        .into_any()
                } else {
                    let show_add_artist_dialog = RwSignal::new(false);
                    let dialog_artist_name = discovered_artist.clone();
                    view! {
                        <>
                            <div class="flex items-center gap-2">
                                <span class="text-[11px] text-red-500 dark:text-red-400">"No match found"</span>
                                <Button size=ButtonSize::Xs on:click=move |_| show_add_artist_dialog.set(true)>
                                    <Search attr:class="size-3" />
                                    "Add Artist"
                                </Button>
                            </div>
                            <AddArtistDialog open=show_add_artist_dialog artist_name=dialog_artist_name />
                        </>
                    }
                        .into_any()
                }}
            </div>

            <div class="flex items-center gap-2 shrink-0">
                <Badge size=BadgeSize::Pill variant=import_status_badge_variant(&match_status)>
                    {match_status.label()}
                </Badge>
            </div>
        </div>
    }
}

#[component]
fn CandidateCard(
    cand: ImportAlbumCandidate,
    idx: usize,
    item_id: String,
    selections: RwSignal<HashMap<String, usize>>,
) -> impl IntoView {
    let class_item_id = item_id.clone();
    let click_item_id = item_id.clone();
    let indicator_item_id = item_id.clone();

    let card_class = move || {
        if selections.with(|selected| selected.get(&class_item_id).copied()) == Some(idx) {
            "flex items-center gap-2.5 px-3 py-2 rounded-lg border cursor-pointer transition-all duration-150 bg-blue-500/[.06] border-blue-500/30 dark:bg-blue-500/[.08] dark:border-blue-500/40"
                .to_string()
        } else {
            "flex items-center gap-2.5 px-3 py-2 rounded-lg border cursor-pointer transition-all duration-150 bg-white/40 border-black/[.06] dark:bg-zinc-800/40 dark:border-white/[.06] hover:border-blue-500/20 dark:hover:border-blue-500/30"
                .to_string()
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
                if selections.with(|selected| selected.get(&click_item_id).copied()) == Some(idx) {
                    selections.update(|selected| {
                        selected.remove(&click_item_id);
                    });
                } else {
                    selections.update(|selected| {
                        selected.insert(click_item_id.clone(), idx);
                    });
                }
            }
        >
            {match cover {
                Some(url) => view! {
                    <img class="size-8 rounded object-cover shrink-0 bg-zinc-200 dark:bg-zinc-700" src=url alt="" />
                }.into_any(),
                None => view! {
                    <div class="size-8 rounded inline-flex items-center justify-center bg-zinc-200 dark:bg-zinc-700 text-zinc-400 dark:text-zinc-500 shrink-0">
                        <Music attr:class="size-3.5" />
                    </div>
                }
                    .into_any(),
            }}

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
                        }
                            .into_any()
                    } else {
                        view! { <span></span> }.into_any()
                    }}
                    <span class="text-[10px] text-zinc-400 dark:text-zinc-500">{artist}</span>
                    {if is_new {
                        view! {
                            <span class="text-[10px] px-1.5 py-0.5 rounded bg-blue-500/10 text-blue-600 dark:text-blue-400 font-medium">
                                "New"
                            </span>
                        }
                            .into_any()
                    } else if is_acquired {
                        view! {
                            <span class="text-[10px] px-1.5 py-0.5 rounded bg-green-500/10 text-green-600 dark:text-green-400 font-medium">
                                "Acquired"
                            </span>
                        }
                            .into_any()
                    } else if is_monitored {
                        view! {
                            <span class="text-[10px] px-1.5 py-0.5 rounded bg-amber-500/10 text-amber-600 dark:text-amber-400 font-medium">
                                "Monitored"
                            </span>
                        }
                            .into_any()
                    } else {
                        view! { <span></span> }.into_any()
                    }}
                </div>
            </div>

            <div class="flex items-center gap-2 shrink-0">
                <span class=conf_class>{format!("{}%", conf)}</span>
                <div class=move || {
                    if selections.with(|selected| selected.get(&indicator_item_id).copied())
                        == Some(idx)
                    {
                        "size-4 rounded-full border-2 border-blue-500 bg-blue-500"
                    } else {
                        "size-4 rounded-full border-2 border-zinc-300 dark:border-zinc-600"
                    }
                }></div>
            </div>
        </div>
    }
}

fn import_status_badge_variant(status: &yoink_shared::ImportMatchStatus) -> BadgeVariant {
    match status {
        yoink_shared::ImportMatchStatus::Matched => BadgeVariant::Success,
        yoink_shared::ImportMatchStatus::Partial => BadgeVariant::Accent,
        yoink_shared::ImportMatchStatus::Unmatched => BadgeVariant::Danger,
    }
}
