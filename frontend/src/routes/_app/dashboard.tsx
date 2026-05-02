import { createFileRoute, Link } from "@tanstack/react-router";
import { DiscAlbumIcon, DownloadIcon, HeartIcon, LibraryIcon, MicIcon } from "lucide-react";
import { $api } from "@/lib/api";
import type { components } from "@/lib/api/types.gen";
import {
  isAlbumAcquired,
  isAlbumWantedLike,
  isDownloadActive,
  providerDisplayName,
} from "@/lib/music";
import { Skeleton } from "@/components/ui/skeleton";

type JobResponse = components["schemas"]["JobResponse"];
type DashboardAlbum = components["schemas"]["DashboardAlbum"];
type LibraryTrack = components["schemas"]["LibraryTrack"];
type MonitoredArtist = components["schemas"]["MonitoredArtist"];

export const Route = createFileRoute("/_app/dashboard")({
  component: DashboardPage,
  staticData: {
    breadcrumb: "Dashboard",
  },
});

function isTrackJob(
  job: JobResponse,
): job is JobResponse & { kind: "download_track"; track_id: string } {
  return job.kind === "download_track";
}

function downloadTitle(
  job: JobResponse,
  albumById: Map<string, DashboardAlbum>,
  trackById: Map<string, LibraryTrack>,
) {
  if (isTrackJob(job)) {
    return trackById.get(job.track_id)?.track.title ?? "Track download";
  }

  return albumById.get(job.album_id)?.title ?? "Album download";
}

function downloadSubtitle(
  job: JobResponse,
  albumById: Map<string, DashboardAlbum>,
  artistById: Map<string, MonitoredArtist>,
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
  const artistName = album?.artist_id != null ? artistById.get(album.artist_id)?.name : null;
  return artistName ? `${artistName} · ${provider}` : `Album download · ${provider}`;
}

function StatCard({
  label,
  value,
  icon: Icon,
}: {
  label: string;
  value: number;
  icon: React.ComponentType<{ className?: string }>;
}) {
  return (
    <div className="rounded-xl border bg-card p-5 shadow-sm">
      <div className="flex items-center justify-between">
        <span className="text-sm font-medium text-muted-foreground">{label}</span>
        <Icon className="size-4 text-muted-foreground" />
      </div>
      <p className="mt-2 text-2xl font-bold">{value}</p>
    </div>
  );
}

function StatCardSkeleton() {
  return (
    <div className="rounded-xl border bg-card p-5 shadow-sm">
      <div className="flex items-center justify-between">
        <Skeleton className="h-4 w-16" />
        <Skeleton className="size-4" />
      </div>
      <Skeleton className="mt-2 h-7 w-12" />
    </div>
  );
}

function DashboardPage() {
  const { data, isLoading, isError } = $api.useQuery("get", "/api/dashboard");
  const { data: tracks } = $api.useQuery("get", "/api/track");

  if (isLoading) {
    return (
      <div className="space-y-8">
        <div>
          <h1 className="text-2xl font-bold tracking-tight">Dashboard</h1>
          <p className="text-muted-foreground">Overview of your music library.</p>
        </div>
        <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
          {Array.from({ length: 6 }).map((_, i) => (
            <StatCardSkeleton key={i} />
          ))}
        </div>
        <div className="grid gap-6 lg:grid-cols-2">
          <Skeleton className="h-48 rounded-xl" />
          <Skeleton className="h-48 rounded-xl" />
        </div>
      </div>
    );
  }

  if (isError || !data) {
    return (
      <div className="space-y-8">
        <div>
          <h1 className="text-2xl font-bold tracking-tight">Dashboard</h1>
          <p className="text-muted-foreground">Overview of your music library.</p>
        </div>
        <div className="rounded-xl border border-red-200 bg-red-50 p-4 text-sm text-red-700 dark:border-red-900/50 dark:bg-red-950/50 dark:text-red-400">
          Failed to load dashboard data.
        </div>
      </div>
    );
  }

  const { artists, albums, jobs } = data;
  const albumById = new Map(albums.map((album) => [album.id, album]));
  const artistById = new Map(artists.map((artist) => [artist.id, artist]));
  const trackById = new Map((tracks ?? []).map((track) => [track.track.id, track]));

  const totalArtists = artists.length;
  const totalAlbums = albums.length;
  const wantedAlbums = albums.filter((a) => isAlbumWantedLike(a.wanted_status)).length;
  const acquiredAlbums = albums.filter((a) => isAlbumAcquired(a.wanted_status)).length;
  const activeDownloads = jobs.filter((j) => isDownloadActive(j.status)).length;
  const recentDownloads = [...jobs]
    .sort((a, b) => b.modified_at.localeCompare(a.modified_at))
    .slice(0, 3);
  const wantedAlbumsList = albums.filter((a) => isAlbumWantedLike(a.wanted_status)).slice(0, 5);

  return (
    <div className="space-y-8">
      <div>
        <h1 className="text-2xl font-bold tracking-tight">Dashboard</h1>
        <p className="text-muted-foreground">Overview of your music library.</p>
      </div>

      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
        <StatCard label="Artists" value={totalArtists} icon={MicIcon} />
        <StatCard label="Albums" value={totalAlbums} icon={DiscAlbumIcon} />
        <StatCard label="Wanted" value={wantedAlbums} icon={HeartIcon} />
        <StatCard label="Acquired" value={acquiredAlbums} icon={LibraryIcon} />
        <StatCard label="Active Downloads" value={activeDownloads} icon={DownloadIcon} />
      </div>

      <div className="grid gap-6 lg:grid-cols-2">
        {/* Recent downloads */}
        <div className="rounded-xl border bg-card shadow-sm">
          <div className="border-b px-5 py-4">
            <h2 className="text-sm font-semibold">Recent Downloads</h2>
          </div>
          {recentDownloads.length === 0 ? (
            <div className="px-5 py-8 text-center text-sm text-muted-foreground">
              No download activity yet.
            </div>
          ) : (
            <div className="divide-y">
              {recentDownloads.map((dl) => (
                <div key={dl.id} className="flex items-center justify-between px-5 py-3">
                  <div className="min-w-0">
                    <p className="truncate text-sm font-medium">
                      {downloadTitle(dl, albumById, trackById)}
                    </p>
                    <p className="truncate text-xs text-muted-foreground">
                      {downloadSubtitle(dl, albumById, artistById, trackById)}
                    </p>
                  </div>
                  <StatusBadge status={dl.status} />
                </div>
              ))}
            </div>
          )}
        </div>

        {/* Wanted albums */}
        <div className="rounded-xl border bg-card shadow-sm">
          <div className="border-b px-5 py-4">
            <h2 className="text-sm font-semibold">Wanted Albums</h2>
          </div>
          {wantedAlbumsList.length === 0 ? (
            <div className="px-5 py-8 text-center text-sm text-muted-foreground">
              No wanted albums. Add some from the search page.
            </div>
          ) : (
            <div className="divide-y">
              {wantedAlbumsList.map((album) => {
                const artist = album.artist_id
                  ? artists.find((a) => a.id === album.artist_id)
                  : undefined;
                const body = (
                  <>
                    <div className="min-w-0">
                      <p className="truncate text-sm font-medium">{album.title}</p>
                      <p className="truncate text-xs text-muted-foreground">
                        {artist?.name ?? "Unknown Artist"} &middot;{" "}
                        {album.release_date?.slice(0, 4)}
                      </p>
                    </div>
                    <span className="shrink-0 rounded-full bg-amber-500/10 px-2.5 py-0.5 text-xs font-medium text-amber-500">
                      Wanted
                    </span>
                  </>
                );

                if (!album.artist_id) {
                  return (
                    <div key={album.id} className="flex items-center justify-between px-5 py-3">
                      {body}
                    </div>
                  );
                }

                return (
                  <Link
                    key={album.id}
                    to="/artists/$artistId/albums/$albumId"
                    params={{
                      artistId: album.artist_id,
                      albumId: album.id,
                    }}
                    className="flex items-center justify-between px-5 py-3 transition-colors hover:bg-muted/50"
                  >
                    {body}
                  </Link>
                );
              })}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

function StatusBadge({ status }: { status: string }) {
  const styles: Record<string, string> = {
    queued: "bg-amber-500/10 text-amber-500",
    running: "bg-blue-500/10 text-blue-500",
    succeeded: "bg-green-500/10 text-green-500",
    failed: "bg-red-500/10 text-red-500",
    cancelled: "bg-muted text-muted-foreground",
  };

  return (
    <span
      className={`shrink-0 rounded-full px-2.5 py-0.5 text-xs font-medium capitalize ${styles[status] ?? ""}`}
    >
      {status}
    </span>
  );
}
