import { createFileRoute } from "@tanstack/react-router";
import {
  DiscAlbumIcon,
  DownloadIcon,
  HeartIcon,
  LibraryIcon,
  MicIcon,
  MusicIcon,
} from "lucide-react";
import { stats, downloads, albums } from "@/lib/mock-data";

export const Route = createFileRoute("/_app/dashboard")({
  component: DashboardPage,
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
        <span className="text-sm font-medium text-muted-foreground">
          {label}
        </span>
        <Icon className="size-4 text-muted-foreground" />
      </div>
      <p className="mt-2 text-2xl font-bold">{value}</p>
    </div>
  );
}

function DashboardPage() {
  const recentDownloads = downloads.slice(0, 3);
  const wantedAlbums = albums.filter((a) => a.wanted).slice(0, 5);

  return (
    <div className="space-y-8">
      <div>
        <h1 className="text-2xl font-bold tracking-tight">Dashboard</h1>
        <p className="text-muted-foreground">Overview of your music library.</p>
      </div>

      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
        <StatCard label="Artists" value={stats.totalArtists} icon={MicIcon} />
        <StatCard
          label="Albums"
          value={stats.totalAlbums}
          icon={DiscAlbumIcon}
        />
        <StatCard label="Tracks" value={stats.totalTracks} icon={MusicIcon} />
        <StatCard label="Wanted" value={stats.wantedAlbums} icon={HeartIcon} />
        <StatCard
          label="Acquired"
          value={stats.acquiredAlbums}
          icon={LibraryIcon}
        />
        <StatCard
          label="Active Downloads"
          value={stats.activeDownloads}
          icon={DownloadIcon}
        />
      </div>

      <div className="grid gap-6 lg:grid-cols-2">
        {/* Recent downloads */}
        <div className="rounded-xl border bg-card shadow-sm">
          <div className="border-b px-5 py-4">
            <h2 className="text-sm font-semibold">Recent Downloads</h2>
          </div>
          <div className="divide-y">
            {recentDownloads.map((dl) => (
              <div
                key={dl.id}
                className="flex items-center justify-between px-5 py-3"
              >
                <div className="min-w-0">
                  <p className="truncate text-sm font-medium">
                    {dl.albumTitle}
                  </p>
                  <p className="truncate text-xs text-muted-foreground">
                    {dl.artistName} &middot; {dl.source}
                  </p>
                </div>
                <StatusBadge status={dl.status} />
              </div>
            ))}
          </div>
        </div>

        {/* Wanted albums */}
        <div className="rounded-xl border bg-card shadow-sm">
          <div className="border-b px-5 py-4">
            <h2 className="text-sm font-semibold">Wanted Albums</h2>
          </div>
          <div className="divide-y">
            {wantedAlbums.map((album) => (
              <div
                key={album.id}
                className="flex items-center justify-between px-5 py-3"
              >
                <div className="min-w-0">
                  <p className="truncate text-sm font-medium">{album.title}</p>
                  <p className="truncate text-xs text-muted-foreground">
                    {album.artistName} &middot; {album.releaseDate?.slice(0, 4)}
                  </p>
                </div>
                <span className="shrink-0 rounded-full bg-amber-500/10 px-2.5 py-0.5 text-xs font-medium text-amber-500">
                  Wanted
                </span>
              </div>
            ))}
          </div>
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
