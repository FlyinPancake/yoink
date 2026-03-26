import { useState } from "react";
import { createFileRoute, Link } from "@tanstack/react-router";
import {
  ArrowLeftIcon,
  FolderInputIcon,
  HardDriveIcon,
  Loader2Icon,
  RefreshCwIcon,
  ScanSearchIcon,
} from "lucide-react";
import { $api } from "@/lib/api";
import { queryKeys } from "@/lib/api/queries";
import {
  useScanImport,
  useConfirmImport,
  usePreviewExternalImport,
  useConfirmExternalImport,
} from "@/lib/api/mutations";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { ImportPreviewList, buildConfirmations } from "@/components/import-preview-list";
import type { PreviewSelection } from "@/components/import-preview-list";
import { ImportResultCard } from "@/components/import-result-card";
import { FileBrowser } from "@/components/file-browser";
import type { components } from "@/lib/api/types.gen";

type ImportPreviewItem = components["schemas"]["ImportPreviewItem"];
type ImportResultSummary = components["schemas"]["ImportResultSummary"];
type ManualImportMode = "copy" | "hardlink";

export const Route = createFileRoute("/_app/import/")({
  component: ImportPage,
  staticData: {
    breadcrumb: "Import",
  },
});

// ── Page ───────────────────────────────────────────────────────

function ImportPage() {
  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-bold tracking-tight">Import</h1>
        <p className="text-muted-foreground">
          Import music from your library directory or an external path on the server.
        </p>
      </div>

      <Tabs defaultValue="library">
        <TabsList>
          <TabsTrigger value="library">
            <HardDriveIcon className="mr-1.5 size-3.5" />
            Library Scan
          </TabsTrigger>
          <TabsTrigger value="external">
            <FolderInputIcon className="mr-1.5 size-3.5" />
            External Import
          </TabsTrigger>
        </TabsList>

        <TabsContent value="library" className="mt-6">
          <LibraryScanTab />
        </TabsContent>

        <TabsContent value="external" className="mt-6">
          <ExternalImportTab />
        </TabsContent>
      </Tabs>
    </div>
  );
}

// ── Tab 1: Library Scan Import ─────────────────────────────────

type LibraryScanStep = "idle" | "scanning" | "preview" | "result";

function LibraryScanTab() {
  const [step, setStep] = useState<LibraryScanStep>("idle");
  const [selection, setSelection] = useState<PreviewSelection>({
    selected: new Set(),
    candidates: new Map(),
  });
  const [result, setResult] = useState<ImportResultSummary | null>(null);

  const scanImport = useScanImport();
  const confirmImport = useConfirmImport();

  // Fetch preview data (enabled once scan has been triggered)
  const {
    data: previewItems,
    isLoading: previewLoading,
    isError: previewError,
    refetch: refetchPreview,
  } = $api.useQuery(
    "get",
    "/api/import/preview",
    {},
    {
      enabled: step === "scanning" || step === "preview",
      ...queryKeys.importPreview(),
    },
  );

  const handleScan = async () => {
    setStep("scanning");
    setResult(null);
    setSelection({ selected: new Set(), candidates: new Map() });

    try {
      await scanImport.mutateAsync({});
      // After triggering the scan, refetch preview
      await refetchPreview();
      setStep("preview");
    } catch {
      // If scan fails, still try to load existing preview
      await refetchPreview();
      setStep("preview");
    }
  };

  const handleImport = async () => {
    if (!previewItems) return;

    const confirmations = buildConfirmations(previewItems, selection);
    if (confirmations.length === 0) return;

    try {
      const res = await confirmImport.mutateAsync({
        body: confirmations,
      });
      setResult(res);
      setStep("result");
    } catch {
      // Mutation error handling is in onError
    }
  };

  // Auto-select matched items when preview loads
  const initializeSelection = (items: ImportPreviewItem[]) => {
    const selected = new Set<string>();
    for (const item of items) {
      if (!item.already_imported && item.match_status === "matched") {
        selected.add(item.id);
      }
    }
    setSelection({ selected, candidates: new Map() });
  };

  // When preview data arrives, initialize selection once
  if (step === "scanning" && previewItems && !previewLoading && selection.selected.size === 0) {
    initializeSelection(previewItems);
    setStep("preview");
  }

  // ── Idle state ──
  if (step === "idle") {
    return (
      <div className="space-y-6">
        <div className="flex flex-col items-center justify-center rounded-xl border border-dashed bg-muted/30 py-20">
          <ScanSearchIcon className="size-10 text-muted-foreground/40" />
          <p className="mt-4 text-sm text-muted-foreground">
            Scan your configured music library directory for albums that can be imported.
          </p>
          <Button
            className="mt-6"
            onClick={() => void handleScan()}
            disabled={scanImport.isPending}
          >
            <ScanSearchIcon className="mr-1.5 size-4" />
            {scanImport.isPending ? "Starting scan..." : "Scan Library"}
          </Button>
        </div>
      </div>
    );
  }

  // ── Scanning / loading preview ──
  if (step === "scanning" && previewLoading) {
    return (
      <div className="space-y-6">
        <div className="flex items-center gap-3">
          <Loader2Icon className="size-5 animate-spin text-muted-foreground" />
          <p className="text-sm text-muted-foreground">Scanning library and matching albums...</p>
        </div>
        <div className="space-y-2">
          {Array.from({ length: 4 }).map((_, i) => (
            <Skeleton key={i} className="h-20 rounded-xl" />
          ))}
        </div>
      </div>
    );
  }

  // ── Preview error ──
  if (previewError) {
    return (
      <div className="space-y-4">
        <div className="rounded-xl border border-red-200 bg-red-50 p-4 text-sm text-red-700 dark:border-red-900/50 dark:bg-red-950/50 dark:text-red-400">
          Failed to load import preview. The scan may still be in progress.
        </div>
        <div className="flex gap-2">
          <Button variant="outline" size="sm" onClick={() => void refetchPreview()}>
            <RefreshCwIcon className="mr-1.5 size-3.5" />
            Retry
          </Button>
          <Button variant="outline" size="sm" onClick={() => setStep("idle")}>
            Back
          </Button>
        </div>
      </div>
    );
  }

  // ── Preview state ──
  if (step === "preview" && previewItems) {
    return (
      <div className="space-y-4">
        <div className="flex items-center justify-between">
          <Button variant="ghost" size="sm" onClick={() => setStep("idle")}>
            <ArrowLeftIcon className="mr-1.5 size-3.5" />
            Back
          </Button>
          <Button
            variant="outline"
            size="sm"
            onClick={() => void handleScan()}
            disabled={scanImport.isPending}
          >
            <RefreshCwIcon className="mr-1.5 size-3.5" />
            Re-scan
          </Button>
        </div>

        <ImportPreviewList
          items={previewItems}
          selection={selection}
          onSelectionChange={setSelection}
          onImport={() => void handleImport()}
          isPending={confirmImport.isPending}
        />
      </div>
    );
  }

  // ── Result state ──
  if (step === "result" && result) {
    return (
      <div className="space-y-6">
        <ImportResultCard result={result} />

        <div className="flex gap-2">
          <Button asChild variant="outline" size="sm">
            <Link to="/library/albums">Go to Library</Link>
          </Button>
          <Button
            variant="outline"
            size="sm"
            onClick={() => {
              setStep("idle");
              setResult(null);
              setSelection({ selected: new Set(), candidates: new Map() });
            }}
          >
            <RefreshCwIcon className="mr-1.5 size-3.5" />
            Scan Again
          </Button>
        </div>
      </div>
    );
  }

  // Fallback loading
  return (
    <div className="flex items-center justify-center py-16">
      <Loader2Icon className="size-6 animate-spin text-muted-foreground" />
    </div>
  );
}

// ── Tab 2: External Import ─────────────────────────────────────

type ExternalStep = "browse" | "preview" | "configure" | "result";

function ExternalImportTab() {
  const [step, setStep] = useState<ExternalStep>("browse");
  const [sourcePath, setSourcePath] = useState<string>("");
  const [previewItems, setPreviewItems] = useState<ImportPreviewItem[]>([]);
  const [selection, setSelection] = useState<PreviewSelection>({
    selected: new Set(),
    candidates: new Map(),
  });
  const [importMode, setImportMode] = useState<ManualImportMode>("copy");
  const [result, setResult] = useState<ImportResultSummary | null>(null);

  const previewExternal = usePreviewExternalImport();
  const confirmExternal = useConfirmExternalImport();

  // Handle folder selection from file browser
  const handleFolderSelect = async (path: string) => {
    setSourcePath(path);
    setSelection({ selected: new Set(), candidates: new Map() });

    try {
      const items = await previewExternal.mutateAsync({
        body: { source_path: path },
      });
      if (items) {
        setPreviewItems(items);

        // Auto-select matched items
        const selected = new Set<string>();
        for (const item of items) {
          if (!item.already_imported && item.match_status === "matched") {
            selected.add(item.id);
          }
        }
        setSelection({ selected, candidates: new Map() });
        setStep("preview");
      }
    } catch {
      // Error is handled by mutation state
    }
  };

  // Handle import confirmation (with mode selection)
  const handleImport = async () => {
    const confirmations = buildConfirmations(previewItems, selection);
    if (confirmations.length === 0) return;

    try {
      const res = await confirmExternal.mutateAsync({
        body: {
          source_path: sourcePath,
          mode: importMode,
          items: confirmations,
        },
      });
      setResult(res);
      setStep("result");
    } catch {
      // Error is handled by mutation state
    }
  };

  const resetFlow = () => {
    setStep("browse");
    setSourcePath("");
    setPreviewItems([]);
    setSelection({ selected: new Set(), candidates: new Map() });
    setResult(null);
  };

  // ── Browse step ──
  if (step === "browse") {
    return (
      <div className="space-y-4">
        <p className="text-sm text-muted-foreground">
          Browse the server filesystem to select a folder containing music to import.
        </p>

        {previewExternal.isError && (
          <div className="rounded-xl border border-red-200 bg-red-50 p-4 text-sm text-red-700 dark:border-red-900/50 dark:bg-red-950/50 dark:text-red-400">
            Failed to preview the selected path. Please try a different directory.
          </div>
        )}

        <FileBrowser onSelect={(path) => void handleFolderSelect(path)} />

        {previewExternal.isPending && (
          <div className="flex items-center gap-3">
            <Loader2Icon className="size-5 animate-spin text-muted-foreground" />
            <p className="text-sm text-muted-foreground">
              Scanning directory and matching albums...
            </p>
          </div>
        )}
      </div>
    );
  }

  // ── Preview step ──
  if (step === "preview") {
    return (
      <div className="space-y-4">
        <div className="flex items-center justify-between">
          <Button variant="ghost" size="sm" onClick={() => setStep("browse")}>
            <ArrowLeftIcon className="mr-1.5 size-3.5" />
            Back to browser
          </Button>
          <p className="font-mono text-xs text-muted-foreground">{sourcePath}</p>
        </div>

        <ImportPreviewList
          items={previewItems}
          selection={selection}
          onSelectionChange={setSelection}
          onImport={() => setStep("configure")}
          isPending={false}
        />
      </div>
    );
  }

  // ── Configure step (choose copy/hardlink) ──
  if (step === "configure") {
    const selectedCount = previewItems.filter((item) => selection.selected.has(item.id)).length;

    return (
      <div className="space-y-6">
        <Button variant="ghost" size="sm" onClick={() => setStep("preview")}>
          <ArrowLeftIcon className="mr-1.5 size-3.5" />
          Back to selection
        </Button>

        <div className="mx-auto max-w-lg space-y-6">
          <div className="space-y-2 text-center">
            <h2 className="text-lg font-semibold">Configure Import</h2>
            <p className="text-sm text-muted-foreground">
              {selectedCount} album{selectedCount !== 1 ? "s" : ""} will be imported from{" "}
              <span className="font-mono text-xs">{sourcePath}</span>
            </p>
          </div>

          {/* Import mode selection */}
          <div className="rounded-xl border bg-card p-5 shadow-sm">
            <label className="text-sm font-medium">Transfer mode</label>
            <p className="mt-1 mb-3 text-xs text-muted-foreground">
              Choose how files are moved into your library.
            </p>
            <Select value={importMode} onValueChange={(v) => setImportMode(v as ManualImportMode)}>
              <SelectTrigger className="w-full">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="copy">Copy &mdash; Full independent copy of files</SelectItem>
                <SelectItem value="hardlink">
                  Hardlink &mdash; No extra disk space (same filesystem only)
                </SelectItem>
              </SelectContent>
            </Select>
          </div>

          {/* Confirm button */}
          <Button
            className="w-full"
            disabled={confirmExternal.isPending}
            onClick={() => void handleImport()}
          >
            {confirmExternal.isPending
              ? "Importing..."
              : `Import ${selectedCount} Album${selectedCount !== 1 ? "s" : ""}`}
          </Button>

          {confirmExternal.isError && (
            <div className="rounded-xl border border-red-200 bg-red-50 p-4 text-sm text-red-700 dark:border-red-900/50 dark:bg-red-950/50 dark:text-red-400">
              Import failed. Please try again.
            </div>
          )}
        </div>
      </div>
    );
  }

  // ── Result step ──
  if (step === "result" && result) {
    return (
      <div className="space-y-6">
        <ImportResultCard result={result} />

        <div className="flex gap-2">
          <Button asChild variant="outline" size="sm">
            <Link to="/library/albums">Go to Library</Link>
          </Button>
          <Button variant="outline" size="sm" onClick={resetFlow}>
            <FolderInputIcon className="mr-1.5 size-3.5" />
            Import More
          </Button>
        </div>
      </div>
    );
  }

  // Fallback
  return (
    <div className="flex items-center justify-center py-16">
      <Loader2Icon className="size-6 animate-spin text-muted-foreground" />
    </div>
  );
}
