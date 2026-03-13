import { createFileRoute } from "@tanstack/react-router";
import { albums } from "@/lib/mock-data";

export const Route = createFileRoute("/_app/library/albums")({
  component: AlbumsPage,
});

function AlbumsPage() {
  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-bold tracking-tight">Albums</h1>
        <p className="text-muted-foreground">
          {albums.length} albums in your library.
        </p>
      </div>

      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4">
        {albums.map((album) => (
          <div
            key={album.id}
            className="group overflow-hidden rounded-xl border bg-card shadow-sm transition-shadow hover:shadow-md"
          >
            <div className="aspect-square bg-muted">
              {album.coverUrl ? (
                <img
                  src={album.coverUrl}
                  alt={album.title}
                  className="size-full object-cover"
                />
              ) : (
                <div className="flex size-full items-center justify-center text-4xl font-bold text-muted-foreground/30">
                  {album.title.charAt(0)}
                </div>
              )}
            </div>
            <div className="p-4">
              <p className="truncate font-semibold">{album.title}</p>
              <p className="truncate text-xs text-muted-foreground">
                {album.artistName} &middot;{" "}
                {album.releaseDate?.slice(0, 4) ?? "Unknown"}
              </p>
              <div className="mt-2 flex flex-wrap items-center gap-1.5">
                {album.albumType && (
                  <span className="rounded-full bg-muted px-2 py-0.5 text-xs font-medium text-muted-foreground">
                    {album.albumType}
                  </span>
                )}
                {album.acquired && (
                  <span className="rounded-full bg-green-500/10 px-2 py-0.5 text-xs font-medium text-green-500">
                    Acquired
                  </span>
                )}
                {album.wanted && (
                  <span className="rounded-full bg-amber-500/10 px-2 py-0.5 text-xs font-medium text-amber-500">
                    Wanted
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
