import { useMemo } from "react";
import {
  CheckCircle2Icon,
  CircleDashedIcon,
  CircleDotIcon,
  FolderIcon,
  MusicIcon,
} from "lucide-react";
import { Checkbox } from "@/components/ui/checkbox";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import type { components } from "@/lib/api/types.gen";

type ImportPreviewItem = components["schemas"]["ImportPreviewItem"];
type ImportMatchStatus = components["schemas"]["ImportMatchStatus"];

// ── Match status display config ────────────────────────────────

const matchStatusConfig: Record<ImportMatchStatus, { label: string; color: string }> = {
  matched: { label: "Matched", color: "text-green-600 dark:text-green-400" },
  partial: { label: "Partial", color: "text-amber-600 dark:text-amber-400" },
  unmatched: { label: "Unmatched", color: "text-red-600 dark:text-red-400" },
};

const matchStatusIcon: Record<ImportMatchStatus, React.ComponentType<{ className?: string }>> = {
  matched: CheckCircle2Icon,
  partial: CircleDotIcon,
  unmatched: CircleDashedIcon,
};

// ── Selection state type ───────────────────────────────────────

export interface PreviewSelection {
  /** Set of preview item IDs that are checked for import. */
  selected: Set<string>;
  /** Map of preview item ID -> selected candidate index (or "new" for create new). */
  candidates: Map<string, string>;
}

// ── Component ──────────────────────────────────────────────────

export function ImportPreviewList({
  items,
  selection,
  onSelectionChange,
  onImport,
  isPending,
}: {
  items: ImportPreviewItem[];
  selection: PreviewSelection;
  onSelectionChange: (selection: PreviewSelection) => void;
  onImport: () => void;
  isPending: boolean;
}) {
  // Group items by match status
  const groups = useMemo(() => {
    const matched: ImportPreviewItem[] = [];
    const partial: ImportPreviewItem[] = [];
    const unmatched: ImportPreviewItem[] = [];

    for (const item of items) {
      if (item.already_imported) continue;
      if (item.match_status === "matched") matched.push(item);
      else if (item.match_status === "partial") partial.push(item);
      else unmatched.push(item);
    }

    const alreadyImported = items.filter((item) => item.already_imported);

    return { matched, partial, unmatched, alreadyImported };
  }, [items]);

  const importableItems = [...groups.matched, ...groups.partial, ...groups.unmatched];
  const selectedCount = importableItems.filter((item) => selection.selected.has(item.id)).length;

  const toggleAll = (checked: boolean) => {
    const next = new Set(selection.selected);
    for (const item of importableItems) {
      if (checked) {
        next.add(item.id);
      } else {
        next.delete(item.id);
      }
    }
    onSelectionChange({ ...selection, selected: next });
  };

  const toggleItem = (id: string, checked: boolean) => {
    const next = new Set(selection.selected);
    if (checked) {
      next.add(id);
    } else {
      next.delete(id);
    }
    onSelectionChange({ ...selection, selected: next });
  };

  const setCandidate = (itemId: string, value: string) => {
    const next = new Map(selection.candidates);
    next.set(itemId, value);
    onSelectionChange({ ...selection, candidates: next });
  };

  if (items.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center rounded-xl border border-dashed bg-muted/30 py-20">
        <FolderIcon className="size-10 text-muted-foreground/40" />
        <p className="mt-4 text-sm text-muted-foreground">No importable albums found.</p>
      </div>
    );
  }

  const allChecked = importableItems.length > 0 && selectedCount === importableItems.length;

  return (
    <div className="space-y-6">
      {/* Header toolbar */}
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-3">
          <Checkbox checked={allChecked} onCheckedChange={(checked) => toggleAll(!!checked)} />
          <span className="text-sm text-muted-foreground">
            {selectedCount} of {importableItems.length} selected
          </span>
        </div>
        <Button size="sm" disabled={isPending || selectedCount === 0} onClick={onImport}>
          {isPending
            ? "Importing..."
            : `Import ${selectedCount} Album${selectedCount !== 1 ? "s" : ""}`}
        </Button>
      </div>

      {/* Matched group */}
      {groups.matched.length > 0 && (
        <PreviewGroup
          title="Matched"
          status="matched"
          items={groups.matched}
          selection={selection}
          onToggle={toggleItem}
          onCandidateChange={setCandidate}
        />
      )}

      {/* Partial group */}
      {groups.partial.length > 0 && (
        <PreviewGroup
          title="Partial Match"
          status="partial"
          items={groups.partial}
          selection={selection}
          onToggle={toggleItem}
          onCandidateChange={setCandidate}
        />
      )}

      {/* Unmatched group */}
      {groups.unmatched.length > 0 && (
        <PreviewGroup
          title="Unmatched"
          status="unmatched"
          items={groups.unmatched}
          selection={selection}
          onToggle={toggleItem}
          onCandidateChange={setCandidate}
        />
      )}

      {/* Already imported */}
      {groups.alreadyImported.length > 0 && (
        <section className="space-y-3">
          <h2 className="text-sm font-semibold tracking-wider text-muted-foreground uppercase">
            Already Imported ({groups.alreadyImported.length})
          </h2>
          <div className="space-y-2 opacity-50">
            {groups.alreadyImported.map((item) => (
              <PreviewItemCard
                key={item.id}
                item={item}
                disabled
                checked={false}
                onToggle={() => {}}
                selectedCandidate={undefined}
                onCandidateChange={() => {}}
              />
            ))}
          </div>
        </section>
      )}
    </div>
  );
}

// ── Group section ──────────────────────────────────────────────

function PreviewGroup({
  title,
  status,
  items,
  selection,
  onToggle,
  onCandidateChange,
}: {
  title: string;
  status: ImportMatchStatus;
  items: ImportPreviewItem[];
  selection: PreviewSelection;
  onToggle: (id: string, checked: boolean) => void;
  onCandidateChange: (itemId: string, value: string) => void;
}) {
  const cfg = matchStatusConfig[status];
  const StatusIcon = matchStatusIcon[status];

  return (
    <section className="space-y-3">
      <h2 className="flex items-center gap-2 text-sm font-semibold tracking-wider text-muted-foreground uppercase">
        <StatusIcon className={`size-4 ${cfg.color}`} />
        {title} ({items.length})
      </h2>
      <div className="space-y-2">
        {items.map((item) => (
          <PreviewItemCard
            key={item.id}
            item={item}
            checked={selection.selected.has(item.id)}
            onToggle={(checked) => onToggle(item.id, checked)}
            selectedCandidate={selection.candidates.get(item.id)}
            onCandidateChange={(value) => onCandidateChange(item.id, value)}
          />
        ))}
      </div>
    </section>
  );
}

// ── Individual preview item card ───────────────────────────────

function PreviewItemCard({
  item,
  checked,
  onToggle,
  disabled,
  selectedCandidate,
  onCandidateChange,
}: {
  item: ImportPreviewItem;
  checked: boolean;
  onToggle: (checked: boolean) => void;
  disabled?: boolean;
  selectedCandidate: string | undefined;
  onCandidateChange: (value: string) => void;
}) {
  const cfg = matchStatusConfig[item.match_status];

  // Resolve selected candidate value — default to server's pre-selection
  const candidateValue =
    selectedCandidate ??
    (item.selected_candidate != null ? String(item.selected_candidate) : "new");

  return (
    <div className="flex items-start gap-3 rounded-xl border bg-card p-4 shadow-sm">
      {/* Checkbox */}
      <div className="pt-0.5">
        <Checkbox checked={checked} onCheckedChange={(c) => onToggle(!!c)} disabled={disabled} />
      </div>

      {/* Cover thumbnail */}
      <div className="size-12 shrink-0 overflow-hidden rounded-lg bg-muted">
        {item.candidates.length > 0 &&
        candidateValue !== "new" &&
        item.candidates[Number(candidateValue)]?.cover_url ? (
          <img
            src={item.candidates[Number(candidateValue)].cover_url!}
            alt={item.discovered_album}
            className="size-full object-cover"
          />
        ) : (
          <div className="flex size-full items-center justify-center">
            <MusicIcon className="size-5 text-muted-foreground/40" />
          </div>
        )}
      </div>

      {/* Info */}
      <div className="min-w-0 flex-1 space-y-1.5">
        <div className="flex items-center gap-2">
          <p className="truncate font-semibold">{item.discovered_album}</p>
          <Badge variant="outline" className={`shrink-0 text-[10px] ${cfg.color}`}>
            {cfg.label}
          </Badge>
          {item.already_imported && (
            <Badge variant="secondary" className="shrink-0 text-[10px]">
              Imported
            </Badge>
          )}
        </div>
        <div className="flex flex-wrap items-center gap-1 text-xs text-muted-foreground">
          <span className="truncate">{item.discovered_artist}</span>
          {item.discovered_year && (
            <>
              <span>&middot;</span>
              <span>{item.discovered_year}</span>
            </>
          )}
          <span>&middot;</span>
          <span>
            {item.audio_file_count} track
            {item.audio_file_count !== 1 ? "s" : ""}
          </span>
        </div>
        <p className="truncate text-xs text-muted-foreground/60" title={item.relative_path}>
          {item.relative_path}
        </p>
      </div>

      {/* Candidate selection */}
      {!disabled && item.candidates.length > 0 && (
        <div className="shrink-0">
          <Select value={candidateValue} onValueChange={onCandidateChange}>
            <SelectTrigger size="sm" className="w-56">
              <SelectValue placeholder="Select match..." />
            </SelectTrigger>
            <SelectContent>
              {item.candidates.map((c, i) => (
                <SelectItem key={i} value={String(i)}>
                  <span className="truncate">
                    {c.artist_name} &mdash; {c.album_title}
                  </span>
                  {c.release_date && (
                    <span className="ml-1 text-muted-foreground">
                      ({c.release_date.slice(0, 4)})
                    </span>
                  )}
                  <span className="ml-1 text-[10px] text-muted-foreground">{c.confidence}%</span>
                </SelectItem>
              ))}
              <SelectItem value="new">Create new album</SelectItem>
            </SelectContent>
          </Select>
        </div>
      )}
    </div>
  );
}

// ── Helper: build ImportConfirmation array from selection ───────

export function buildConfirmations(items: ImportPreviewItem[], selection: PreviewSelection) {
  const confirmations: Array<{
    preview_id: string;
    artist_name: string;
    album_title: string;
    year: string | null;
    artist_id: string | null;
    album_id: string | null;
  }> = [];

  for (const item of items) {
    if (!selection.selected.has(item.id)) continue;

    const candidateValue =
      selection.candidates.get(item.id) ??
      (item.selected_candidate != null ? String(item.selected_candidate) : "new");

    if (candidateValue !== "new" && item.candidates[Number(candidateValue)]) {
      const c = item.candidates[Number(candidateValue)];
      confirmations.push({
        preview_id: item.id,
        artist_name: c.artist_name,
        album_title: c.album_title,
        year: c.release_date?.slice(0, 4) ?? null,
        artist_id: c.artist_id,
        album_id: c.album_id ?? null,
      });
    } else {
      confirmations.push({
        preview_id: item.id,
        artist_name: item.discovered_artist,
        album_title: item.discovered_album,
        year: item.discovered_year ?? null,
        artist_id: null,
        album_id: null,
      });
    }
  }

  return confirmations;
}
