import { createFileRoute } from "@tanstack/react-router";
import { artists } from "@/lib/mock-data";

export const Route = createFileRoute("/_app/library/artists")({
  component: ArtistsPage,
});

function ArtistsPage() {
  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-bold tracking-tight">Artists</h1>
        <p className="text-muted-foreground">
          {artists.length} artists in your library.
        </p>
      </div>

      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4">
        {artists.map((artist) => (
          <div
            key={artist.id}
            className="group relative overflow-hidden rounded-xl border bg-card shadow-sm transition-shadow hover:shadow-md"
          >
            <div className="aspect-square bg-muted">
              {artist.imageUrl ? (
                <img
                  src={artist.imageUrl}
                  alt={artist.name}
                  className="size-full object-cover"
                />
              ) : (
                <div className="flex size-full items-center justify-center text-4xl font-bold text-muted-foreground/30">
                  {artist.name.charAt(0)}
                </div>
              )}
            </div>
            <div className="p-4">
              <p className="truncate font-semibold">{artist.name}</p>
              <p className="text-xs text-muted-foreground">
                {artist.albumCount} albums &middot; {artist.trackCount} tracks
              </p>
              <div className="mt-2 flex items-center gap-2">
                {artist.monitored ? (
                  <span className="rounded-full bg-green-500/10 px-2 py-0.5 text-xs font-medium text-green-500">
                    Monitored
                  </span>
                ) : (
                  <span className="rounded-full bg-muted px-2 py-0.5 text-xs font-medium text-muted-foreground">
                    Unmonitored
                  </span>
                )}
              </div>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
