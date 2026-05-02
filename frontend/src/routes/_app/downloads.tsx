import { createFileRoute } from "@tanstack/react-router";
import {
  AlertCircleIcon,
  CheckCircle2Icon,
  Loader2Icon,
  ClockIcon,
  Trash2Icon,
  XCircleIcon,
} from "lucide-react";
import { $api } from "@/lib/api";
import { useCancelJob, useClearCompletedJobs } from "@/lib/api/mutations";
import {
  canCancelDownload,
  isDownloadActive,
  isDownloadHistory,
  providerDisplayName,
} from "@/lib/music";
import { Skeleton } from "@/components/ui/skeleton";
import { Button } from "@/components/ui/button";
import type { components } from "@/lib/api/types.gen";

type JobResponse = components["schemas"]["JobResponse"];
type JobStatus = components["schemas"]["JobStatus"];
type LibraryAlbumSummary = components["schemas"]["LibraryAlbumSummary"];
type LibraryTrack = components["schemas"]["LibraryTrack"];

export const Route = createFileRoute("/_app/downloads")({
  component: DownloadsPage,
  staticData: {
    breadcrumb: "Downloads",
  },
});

const statusConfig: Record<
  JobStatus,
  { icon: React.ComponentType<{ className?: string }>; color: string }
> = {
  queued: { icon: ClockIcon, color: "text-amber-500" },
  running: { icon: Loader2Icon, color: "text-blue-500" },
  succeeded: { icon: CheckCircle2Icon, color: "text-green-500" },
  failed: { icon: AlertCircleIcon, color: "text-red-500" },
  cancelled: { icon: XCircleIcon, color: "text-muted-foreground" },
};

function isTrackJob(
  job: JobResponse,
): job is JobResponse & { kind: "download_track"; track_id: string } {
  return job.kind === "download_track";
}

function progressPercent(progress: number): number {
  return Math.max(0, Math.min(100, Math.round(progress * 100)));
}

function downloadTitle(
  job: JobResponse,
  albumById: Map<string, LibraryAlbumSummary>,
  trackById: Map<string, LibraryTrack>,
) {
  if (isTrackJob(job)) {
    return trackById.get(job.track_id)?.track.title ?? "Track download";
  }

  return albumById.get(job.album_id)?.title ?? "Album download";
}

function downloadSubtitle(
  job: JobResponse,
  albumById: Map<string, LibraryAlbumSummary>,
  trackById: Map<string, LibraryTrack>,
) {
  const provider = providerDisplayName(job.provider);

  if (isTrackJob(job)) {
    const track = trackById.get(job.track_id);
    return track != null
      ? `${track.artist_name} · ${track.album_title} · ${provider}`
      : `Track download · ${provider}`;
  }

  const album = albumById.get(job.album_id);
  return album?.artist_name ? `${album.artist_name} · ${provider}` : `Album download · ${provider}`;
}

function DownloadsPage() {
  const { data: jobs, isLoading, isError } = $api.useQuery("get", "/api/job");
  const { data: albums } = $api.useQuery("get", "/api/album");
  const { data: tracks } = $api.useQuery("get", "/api/track");
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

  const active = jobs.filter((d) => isDownloadActive(d.status));
  const history = jobs.filter((d) => isDownloadHistory(d.status));
  const albumById = new Map((albums ?? []).map((album) => [album.id, album]));
  const trackById = new Map((tracks ?? []).map((track) => [track.track.id, track]));

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
                    albumById={albumById}
                    trackById={trackById}
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
                  <DownloadRow key={dl.id} dl={dl} albumById={albumById} trackById={trackById} />
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
  albumById,
  trackById,
  onCancel,
  cancelling,
}: {
  dl: JobResponse;
  albumById: Map<string, LibraryAlbumSummary>;
  trackById: Map<string, LibraryTrack>;
  onCancel?: () => void;
  cancelling?: boolean;
}) {
  const cfg = statusConfig[dl.status];
  const Icon = cfg.icon;
  const progress = progressPercent(dl.progress);
  const kindLabel = isTrackJob(dl) ? "track" : "album";

  return (
    <div className="overflow-hidden rounded-xl border bg-card shadow-sm">
      <div className="flex items-center gap-4 p-4">
        <Icon
          className={`size-5 shrink-0 ${cfg.color} ${dl.status === "running" ? "animate-spin" : ""}`}
        />
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-2">
            <p className="truncate font-semibold">{downloadTitle(dl, albumById, trackById)}</p>
            <span className="shrink-0 rounded bg-muted px-1.5 py-0.5 text-[10px] font-medium text-muted-foreground uppercase">
              {kindLabel}
            </span>
            <span className="shrink-0 rounded bg-muted px-1.5 py-0.5 text-[10px] font-medium text-muted-foreground uppercase">
              {providerDisplayName(dl.provider)}
            </span>
          </div>
          <p className="text-sm text-muted-foreground">
            {downloadSubtitle(dl, albumById, trackById)}
          </p>
          {dl.error_message && <p className="mt-1 text-xs text-red-500">{dl.error_message}</p>}
        </div>
        <div className="flex shrink-0 items-center gap-3">
          <div className="text-right">
            <p className="text-sm font-medium capitalize">{dl.status.replaceAll("_", " ")}</p>
            <p className="text-xs text-muted-foreground tabular-nums">{progress}%</p>
          </div>
          {onCancel && canCancelDownload(dl.status) && (
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
      {isDownloadActive(dl.status) && (
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
