import { createFileRoute } from "@tanstack/react-router";
import {
  AlertCircleIcon,
  CheckCircle2Icon,
  Loader2Icon,
  ClockIcon,
  SearchIcon,
  Trash2Icon,
  XCircleIcon,
} from "lucide-react";
import { $api } from "@/lib/api";
import { useCancelJob, useClearCompletedJobs } from "@/lib/api/mutations";
import { Skeleton } from "@/components/ui/skeleton";
import { Button } from "@/components/ui/button";
import type { components } from "@/lib/api/types.gen";

type DownloadJob = components["schemas"]["DownloadJob"];
type DownloadStatus = components["schemas"]["DownloadStatus"];

export const Route = createFileRoute("/_app/downloads")({
  component: DownloadsPage,
  staticData: {
    breadcrumb: "Downloads",
  },
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
  const { data: jobs, isLoading, isError } = $api.useQuery("get", "/api/job");
  const cancelJob = useCancelJob();
  const clearCompleted = useClearCompletedJobs();

  if (isLoading) {
    return (
      <div className="space-y-8">
        <div>
          <h1 className="text-2xl font-bold tracking-tight">Downloads</h1>
          <Skeleton className="mt-1 h-4 w-48" />
        </div>
        <div className="space-y-2">
          {Array.from({ length: 4 }).map((_, i) => (
            <Skeleton key={i} className="h-20 rounded-xl" />
          ))}
        </div>
      </div>
    );
  }

  if (isError || !jobs) {
    return (
      <div className="space-y-8">
        <div>
          <h1 className="text-2xl font-bold tracking-tight">Downloads</h1>
        </div>
        <div className="rounded-xl border border-red-200 bg-red-50 p-4 text-sm text-red-700 dark:border-red-900/50 dark:bg-red-950/50 dark:text-red-400">
          Failed to load downloads.
        </div>
      </div>
    );
  }

  const active = jobs.filter((d) => ["queued", "resolving", "downloading"].includes(d.status));
  const history = jobs.filter((d) => ["completed", "failed"].includes(d.status));

  return (
    <div className="space-y-8">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold tracking-tight">Downloads</h1>
          <p className="text-muted-foreground">
            {active.length} active &middot; {history.length} in history
          </p>
        </div>
        {history.length > 0 && (
          <Button
            variant="outline"
            size="sm"
            disabled={clearCompleted.isPending}
            onClick={() => clearCompleted.mutate({})}
          >
            <Trash2Icon className="mr-1.5 size-3.5" />
            {clearCompleted.isPending ? "Clearing..." : "Clear History"}
          </Button>
        )}
      </div>

      {jobs.length === 0 ? (
        <div className="flex flex-col items-center justify-center rounded-xl border border-dashed bg-muted/30 py-20">
          <p className="text-sm text-muted-foreground">
            No downloads yet. Monitor some albums to start downloading.
          </p>
        </div>
      ) : (
        <>
          {active.length > 0 && (
            <section className="space-y-3">
              <h2 className="text-sm font-semibold tracking-wider text-muted-foreground uppercase">
                Active
              </h2>
              <div className="space-y-2">
                {active.map((dl) => (
                  <DownloadRow
                    key={dl.id}
                    dl={dl}
                    onCancel={() =>
                      cancelJob.mutate({
                        params: { path: { job_id: dl.id } },
                      })
                    }
                    cancelling={cancelJob.isPending}
                  />
                ))}
              </div>
            </section>
          )}

          {history.length > 0 && (
            <section className="space-y-3">
              <h2 className="text-sm font-semibold tracking-wider text-muted-foreground uppercase">
                History
              </h2>
              <div className="space-y-2">
                {history.map((dl) => (
                  <DownloadRow key={dl.id} dl={dl} />
                ))}
              </div>
            </section>
          )}
        </>
      )}
    </div>
  );
}

function DownloadRow({
  dl,
  onCancel,
  cancelling,
}: {
  dl: DownloadJob;
  onCancel?: () => void;
  cancelling?: boolean;
}) {
  const cfg = statusConfig[dl.status];
  const Icon = cfg.icon;
  const progress =
    dl.total_tracks > 0 ? Math.round((dl.completed_tracks / dl.total_tracks) * 100) : 0;

  return (
    <div className="overflow-hidden rounded-xl border bg-card shadow-sm">
      <div className="flex items-center gap-4 p-4">
        <Icon
          className={`size-5 shrink-0 ${cfg.color} ${dl.status === "downloading" ? "animate-spin" : ""}`}
        />
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <p className="truncate font-semibold">{dl.album_title}</p>
            <span className="shrink-0 rounded bg-muted px-1.5 py-0.5 text-[10px] font-medium text-muted-foreground uppercase">
              {dl.quality}
            </span>
          </div>
          <p className="text-sm text-muted-foreground">
            {dl.artist_name} &middot; {dl.source}
          </p>
          {dl.error && <p className="mt-1 text-xs text-red-500">{dl.error}</p>}
        </div>
        <div className="flex shrink-0 items-center gap-3">
          <div className="text-right">
            <p className="text-sm font-medium tabular-nums">
              {dl.completed_tracks}/{dl.total_tracks}
            </p>
            <p className="text-xs text-muted-foreground">{progress}%</p>
          </div>
          {onCancel && dl.status === "queued" && (
            <Button
              variant="ghost"
              size="sm"
              disabled={cancelling}
              onClick={onCancel}
              title="Cancel download"
            >
              <XCircleIcon className="size-4" />
            </Button>
          )}
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
