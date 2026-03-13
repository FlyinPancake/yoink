import { createFileRoute } from "@tanstack/react-router";
import {
  AlertCircleIcon,
  CheckCircle2Icon,
  Loader2Icon,
  ClockIcon,
  SearchIcon,
} from "lucide-react";
import { downloads, type DownloadStatus } from "@/lib/mock-data";

export const Route = createFileRoute("/_app/downloads")({
  component: DownloadsPage,
});

const statusConfig: Record<
  DownloadStatus,
  { icon: React.ComponentType<{ className?: string }>; color: string }
> = {
  queued: { icon: ClockIcon, color: "text-amber-500" },
  resolving: { icon: SearchIcon, color: "text-violet-500" },
  downloading: { icon: Loader2Icon, color: "text-blue-500" },
  completed: { icon: CheckCircle2Icon, color: "text-green-500" },
  failed: { icon: AlertCircleIcon, color: "text-red-500" },
};

function DownloadsPage() {
  const active = downloads.filter((d) =>
    ["queued", "resolving", "downloading"].includes(d.status),
  );
  const history = downloads.filter((d) =>
    ["completed", "failed"].includes(d.status),
  );

  return (
    <div className="space-y-8">
      <div>
        <h1 className="text-2xl font-bold tracking-tight">Downloads</h1>
        <p className="text-muted-foreground">
          {active.length} active &middot; {history.length} in history
        </p>
      </div>

      {active.length > 0 && (
        <section className="space-y-3">
          <h2 className="text-sm font-semibold text-muted-foreground uppercase tracking-wider">
            Active
          </h2>
          <div className="space-y-2">
            {active.map((dl) => (
              <DownloadRow key={dl.id} dl={dl} />
            ))}
          </div>
        </section>
      )}

      {history.length > 0 && (
        <section className="space-y-3">
          <h2 className="text-sm font-semibold text-muted-foreground uppercase tracking-wider">
            History
          </h2>
          <div className="space-y-2">
            {history.map((dl) => (
              <DownloadRow key={dl.id} dl={dl} />
            ))}
          </div>
        </section>
      )}
    </div>
  );
}

function DownloadRow({ dl }: { dl: (typeof downloads)[number] }) {
  const cfg = statusConfig[dl.status];
  const Icon = cfg.icon;
  const progress =
    dl.totalTracks > 0
      ? Math.round((dl.completedTracks / dl.totalTracks) * 100)
      : 0;

  return (
    <div className="overflow-hidden rounded-xl border bg-card shadow-sm">
      <div className="flex items-center gap-4 p-4">
        <Icon
          className={`size-5 shrink-0 ${cfg.color} ${dl.status === "downloading" ? "animate-spin" : ""}`}
        />
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <p className="truncate font-semibold">{dl.albumTitle}</p>
            <span className="shrink-0 rounded bg-muted px-1.5 py-0.5 text-[10px] font-medium uppercase text-muted-foreground">
              {dl.quality}
            </span>
          </div>
          <p className="text-sm text-muted-foreground">
            {dl.artistName} &middot; {dl.source}
          </p>
          {dl.error && <p className="mt-1 text-xs text-red-500">{dl.error}</p>}
        </div>
        <div className="shrink-0 text-right">
          <p className="text-sm font-medium tabular-nums">
            {dl.completedTracks}/{dl.totalTracks}
          </p>
          <p className="text-xs text-muted-foreground">{progress}%</p>
        </div>
      </div>
      {dl.status === "downloading" && (
        <div className="h-1 bg-muted">
          <div
            className="h-full bg-blue-500 transition-all duration-300"
            style={{ width: `${progress}%` }}
          />
        </div>
      )}
    </div>
  );
}
