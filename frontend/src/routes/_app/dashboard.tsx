import { createFileRoute, Link } from "@tanstack/react-router";
import { DiscAlbumIcon, DownloadIcon, HeartIcon, LibraryIcon, MicIcon } from "lucide-react";
import { $api } from "@/lib/api";
import { Skeleton } from "@/components/ui/skeleton";

export const Route = createFileRoute("/_app/dashboard")({
  component: DashboardPage,
  staticData: {
    breadcrumb: "Dashboard",
  },
});

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

  const totalArtists = artists.length;
  const totalAlbums = albums.length;
  const wantedAlbums = albums.filter((a) => a.wanted || a.partially_wanted).length;
  const acquiredAlbums = albums.filter((a) => a.acquired).length;
  const activeDownloads = jobs.filter((j) =>
    ["queued", "resolving", "downloading"].includes(j.status),
  ).length;
  const recentDownloads = jobs.slice(0, 3);
  const wantedAlbumsList = albums.filter((a) => a.wanted || a.partially_wanted).slice(0, 5);

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
                    <p className="truncate text-sm font-medium">{dl.album_title}</p>
                    <p className="truncate text-xs text-muted-foreground">
                      {dl.artist_name} &middot; {dl.source}
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
                const artist = artists.find((a) => a.id === album.artist_id);
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
    resolving: "bg-violet-500/10 text-violet-500",
    downloading: "bg-blue-500/10 text-blue-500",
    completed: "bg-green-500/10 text-green-500",
    failed: "bg-red-500/10 text-red-500",
  };

  return (
    <span
      className={`shrink-0 rounded-full px-2.5 py-0.5 text-xs font-medium capitalize ${styles[status] ?? ""}`}
    >
      {status}
    </span>
  );
}
