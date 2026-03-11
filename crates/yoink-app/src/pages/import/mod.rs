use leptos::prelude::*;
use lucide_leptos::{FolderOpen, HardDriveDownload};

use crate::components::{PageHeader, PageShell};
use crate::hooks::set_page_title;

mod external;
mod library_scan;
mod server_fns;
mod shared;

use external::ExternalImportTab;
use library_scan::LibraryScanTab;

#[derive(Clone, Copy, PartialEq, Eq)]
enum ImportTab {
    LibraryScan,
    ExternalImport,
}

#[component]
pub fn ImportPage() -> impl IntoView {
    set_page_title("Import");
    let active_tab = RwSignal::new(ImportTab::LibraryScan);

    view! {
        <PageShell active="import">
            <PageHeader title="Import"></PageHeader>
            <div class="p-6 max-md:p-4">
                <div class="flex items-center gap-2 mb-5">
                    <button
                        type="button"
                        class=move || {
                            if active_tab.get() == ImportTab::LibraryScan {
                                "px-4 py-2 text-sm font-medium rounded-lg bg-blue-500/10 text-blue-600 dark:text-blue-400 border border-blue-500/20 cursor-pointer transition-all duration-150"
                            } else {
                                "px-4 py-2 text-sm font-medium rounded-lg text-zinc-500 dark:text-zinc-400 border border-transparent cursor-pointer transition-all duration-150 hover:text-zinc-700 dark:hover:text-zinc-300 hover:bg-zinc-100/50 dark:hover:bg-zinc-800/50"
                            }
                        }
                        on:click=move |_| active_tab.set(ImportTab::LibraryScan)
                    >
                        <span class="flex items-center gap-1.5">
                            <FolderOpen attr:class="size-4" />
                            "Library Scan"
                        </span>
                    </button>
                    <button
                        type="button"
                        class=move || {
                            if active_tab.get() == ImportTab::ExternalImport {
                                "px-4 py-2 text-sm font-medium rounded-lg bg-blue-500/10 text-blue-600 dark:text-blue-400 border border-blue-500/20 cursor-pointer transition-all duration-150"
                            } else {
                                "px-4 py-2 text-sm font-medium rounded-lg text-zinc-500 dark:text-zinc-400 border border-transparent cursor-pointer transition-all duration-150 hover:text-zinc-700 dark:hover:text-zinc-300 hover:bg-zinc-100/50 dark:hover:bg-zinc-800/50"
                            }
                        }
                        on:click=move |_| active_tab.set(ImportTab::ExternalImport)
                    >
                        <span class="flex items-center gap-1.5">
                            <HardDriveDownload attr:class="size-4" />
                            "External Import"
                        </span>
                    </button>
                </div>

                {move || match active_tab.get() {
                    ImportTab::LibraryScan => view! { <LibraryScanTab /> }.into_any(),
                    ImportTab::ExternalImport => view! { <ExternalImportTab /> }.into_any(),
                }}
            </div>
        </PageShell>
    }
}
