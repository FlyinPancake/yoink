import { BookmarkIcon, ChevronDownIcon } from "lucide-react";
import { cn } from "@/lib/utils";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import type { components } from "@/lib/api/types.gen";

type Quality = components["schemas"]["Quality"];

const QUALITY_OPTIONS: Array<{ value: Quality; label: string }> = [
  { value: "HiRes", label: "Hi-Res" },
  { value: "Lossless", label: "Lossless" },
  { value: "High", label: "High" },
  { value: "Low", label: "Low" },
];

interface MonitorButtonProps {
  // Whether the item is currently monitored.
  monitored: boolean;
  // Called when the user clicks the main bookmark area to toggle monitor.
  onToggleMonitor: () => void;
  // The current quality override, or null for "use default".
  qualityOverride: Quality | null;
  // The system default quality (shown as the label for "default").
  defaultQuality: Quality;
  // Called when the user picks a quality option. null means "use default".
  onQualityChange: (quality: Quality | null) => void;
  // Whether a mutation is in-flight.
  pending?: boolean;
  // (Optional) Visual variant to use. "compact" hides the text labels
  variant?: "default" | "compact";
}

export function MonitorButton({
  monitored,
  onToggleMonitor,
  qualityOverride,
  defaultQuality,
  onQualityChange,
  pending,
  variant = "default",
}: MonitorButtonProps) {
  const compact = variant === "compact";

  return (
    <div className="inline-flex items-stretch">
      {/* ── Main toggle ──────────────────────────────────────── */}
      <button
        type="button"
        disabled={pending}
        onClick={onToggleMonitor}
        title={monitored ? "Monitored - click to unmonitor" : "Monitor"}
        className={cn(
          "inline-flex cursor-pointer items-center rounded-l-4xl border font-medium transition-colors",
          "focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-1 focus-visible:outline-none",
          "disabled:pointer-events-none disabled:opacity-50",
          compact ? "gap-1 px-2 py-1.5 text-xs" : "gap-1.5 px-3 py-1.5 text-sm",
          monitored
            ? "border-amber-500/40 bg-amber-500/10 text-amber-600 hover:bg-amber-500/20 dark:text-amber-400 dark:hover:bg-amber-500/15"
            : "border-border bg-input/30 text-muted-foreground hover:bg-input/50 hover:text-foreground",
        )}
      >
        <BookmarkIcon
          className={compact ? "size-3" : "size-3.5"}
          fill={monitored ? "currentColor" : "none"}
        />
        {!compact && (monitored ? "Monitored" : "Monitor")}
      </button>

      {/* ── Quality dropdown ─────────────────────────────────── */}
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <button
            type="button"
            disabled={pending}
            title={`Download quality: ${qualityOverride ?? `Default (${defaultQuality})`}`}
            className={cn(
              "-ml-px inline-flex cursor-pointer items-center rounded-r-4xl border font-medium transition-colors",
              "focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-1 focus-visible:outline-none",
              "disabled:pointer-events-none disabled:opacity-50",
              compact ? "px-1.5 py-1.5 text-[10px]" : "px-2 py-1.5 text-xs",
              monitored
                ? "border-amber-500/40 bg-amber-500/10 text-amber-600 hover:bg-amber-500/20 dark:text-amber-400 dark:hover:bg-amber-500/15"
                : "border-border bg-input/30 text-muted-foreground hover:bg-input/50 hover:text-foreground",
            )}
          >
            <ChevronDownIcon className="size-3" />
          </button>
        </DropdownMenuTrigger>
        <DropdownMenuContent align="end" className="min-w-40">
          <DropdownMenuLabel>Download Quality</DropdownMenuLabel>
          <DropdownMenuSeparator />
          <DropdownMenuRadioGroup
            value={qualityOverride ?? "default"}
            onValueChange={(value) =>
              onQualityChange(value === "default" ? null : (value as Quality))
            }
          >
            <DropdownMenuRadioItem value="default">
              Default ({defaultQuality})
            </DropdownMenuRadioItem>
            {QUALITY_OPTIONS.map((q) => (
              <DropdownMenuRadioItem key={q.value} value={q.value}>
                {q.label}
              </DropdownMenuRadioItem>
            ))}
          </DropdownMenuRadioGroup>
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  );
}
