import { createFileRoute, Link } from "@tanstack/react-router";
import { $api } from "@/lib/api";
import { Skeleton } from "@/components/ui/skeleton";

export const Route = createFileRoute("/_app/wanted")({
  component: WantedPage,
  staticData: {
    breadcrumb: "Wanted",
  },
});

function WantedPage() {
  const { data, isLoading, isError } = $api.useQuery("get", "/api/wanted");

  if (isLoading) {
    return (
      <div className="space-y-6">
        <div>
          <h1 className="text-2xl font-bold tracking-tight">Wanted</h1>
          <Skeleton className="mt-1 h-4 w-64" />
        </div>
        <div className="space-y-4">
          {Array.from({ length: 4 }).map((_, i) => (
            <Skeleton key={i} className="h-28 rounded-xl" />
          ))}
        </div>
      </div>
    );
  }

  if (isError || !data) {
    return (
      <div className="space-y-6">
        <div>
          <h1 className="text-2xl font-bold tracking-tight">Wanted</h1>
        </div>
        <div className="rounded-xl border border-red-200 bg-red-50 p-4 text-sm text-red-700 dark:border-red-900/50 dark:bg-red-950/50 dark:text-red-400">
          Failed to load wanted data.
        </div>
      </div>
    );
  }

  const { albums: wantedAlbums, artists } = data;

  // Build a map for quick artist name lookup
  const artistMap = new Map(artists.map((a) => [a.id, a]));

  // Count total wanted tracks across all albums
  const totalWantedTracks = wantedAlbums.reduce(
    (sum, wa) => sum + wa.tracks.filter((t) => t.monitored && !t.acquired).length,
    0,
  );

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-bold tracking-tight">Wanted</h1>
        <p className="text-muted-foreground">
          {wantedAlbums.length} album{wantedAlbums.length !== 1 ? "s" : ""} and {totalWantedTracks}{" "}
          track{totalWantedTracks !== 1 ? "s" : ""} waiting to be acquired.
        </p>
      </div>

      {wantedAlbums.length === 0 ? (
        <div className="flex flex-col items-center justify-center rounded-xl border border-dashed bg-muted/30 py-20">
          <p className="text-sm text-muted-foreground">
            Nothing wanted right now. Monitor some albums to start tracking.
          </p>
        </div>
      ) : (
        <div className="space-y-4">
          {wantedAlbums.map(({ album, tracks }) => {
            const wantedTracks = tracks.filter((t) => t.monitored && !t.acquired);
            const artist = artistMap.get(album.artist_id);
            const artistName = album.artist_credits?.[0]?.name ?? artist?.name ?? "Unknown Artist";

            return (
              <div key={album.id} className="overflow-hidden rounded-xl border bg-card shadow-sm">
                <Link
                  to="/artists/$artistId/albums/$albumId"
                  params={{
                    artistId: album.artist_id,
                    albumId: album.id,
                  }}
                  className="flex items-center gap-4 p-4 transition-colors hover:bg-muted/50"
                >
                  <div className="size-16 shrink-0 overflow-hidden rounded-lg bg-muted">
                    {album.cover_url ? (
                      <img
                        src={album.cover_url}
                        alt={album.title}
                        className="size-full object-cover"
                      />
                    ) : (
                      <div className="flex size-full items-center justify-center text-lg font-bold text-muted-foreground/30">
                        {album.title.charAt(0)}
                      </div>
                    )}
                  </div>
                  <div className="min-w-0 flex-1">
                    <p className="truncate font-semibold">{album.title}</p>
                    <p className="text-sm text-muted-foreground">
                      {artistName} &middot; {album.release_date?.slice(0, 4)}
                    </p>
                    <p className="mt-1 text-xs text-muted-foreground">
                      {tracks.length} track{tracks.length !== 1 ? "s" : ""} &middot;{" "}
                      {album.monitored
                        ? "Full album wanted"
                        : `${wantedTracks.length} track${wantedTracks.length !== 1 ? "s" : ""} wanted`}
                    </p>
                  </div>
                  <span className="shrink-0 rounded-full bg-amber-500/10 px-3 py-1 text-xs font-medium text-amber-500">
                    Wanted
                  </span>
                </Link>

                {wantedTracks.length > 0 && (
                  <div className="border-t">
                    <div className="divide-y">
                      {wantedTracks.map((track) => (
                        <div key={track.id} className="flex items-center gap-3 px-4 py-2 text-sm">
                          <span className="w-6 text-right text-muted-foreground tabular-nums">
                            {track.track_number}
                          </span>
                          <span className="flex-1 truncate">{track.title}</span>
                          <span className="text-xs text-muted-foreground">
                            {track.duration_display}
                          </span>
                        </div>
                      ))}
                    </div>
                  </div>
                )}
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
