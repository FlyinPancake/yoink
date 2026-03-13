import { createFileRoute } from "@tanstack/react-router";
import { albums, tracks } from "@/lib/mock-data";

export const Route = createFileRoute("/_app/wanted")({
  component: WantedPage,
});

function WantedPage() {
  const wantedAlbums = albums.filter((a) => a.wanted);
  const wantedTracks = tracks.filter((t) => t.monitored && !t.acquired);

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-bold tracking-tight">Wanted</h1>
        <p className="text-muted-foreground">
          {wantedAlbums.length} albums and {wantedTracks.length} tracks waiting
          to be acquired.
        </p>
      </div>

      <div className="space-y-4">
        {wantedAlbums.map((album) => {
          const albumTracks = wantedTracks.filter(
            (t) => t.albumId === album.id,
          );
          return (
            <div
              key={album.id}
              className="overflow-hidden rounded-xl border bg-card shadow-sm"
            >
              <div className="flex items-center gap-4 p-4">
                <div className="size-16 shrink-0 overflow-hidden rounded-lg bg-muted">
                  {album.coverUrl ? (
                    <img
                      src={album.coverUrl}
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
                    {album.artistName} &middot; {album.releaseDate?.slice(0, 4)}
                  </p>
                  <p className="mt-1 text-xs text-muted-foreground">
                    {album.trackCount} tracks &middot;{" "}
                    {album.monitored
                      ? "Full album wanted"
                      : `${albumTracks.length} tracks wanted`}
                  </p>
                </div>
                <span className="shrink-0 rounded-full bg-amber-500/10 px-3 py-1 text-xs font-medium text-amber-500">
                  Wanted
                </span>
              </div>

              {albumTracks.length > 0 && (
                <div className="border-t">
                  <div className="divide-y">
                    {albumTracks.map((track) => (
                      <div
                        key={track.id}
                        className="flex items-center gap-3 px-4 py-2 text-sm"
                      >
                        <span className="w-6 text-right tabular-nums text-muted-foreground">
                          {track.trackNumber}
                        </span>
                        <span className="flex-1 truncate">{track.title}</span>
                        <span className="text-xs text-muted-foreground">
                          {track.durationDisplay}
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
    </div>
  );
}
