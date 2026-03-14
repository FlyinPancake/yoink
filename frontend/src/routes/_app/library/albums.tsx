import { createFileRoute, Link } from "@tanstack/react-router";
import { $api } from "@/lib/api";
import { Skeleton } from "@/components/ui/skeleton";

export const Route = createFileRoute("/_app/library/albums")({
  component: AlbumsPage,
});

function AlbumsPage() {
  const {
    data: albums,
    isLoading,
    isError,
  } = $api.useQuery("get", "/api/album");

  // Also load artists so we can resolve artist names
  const { data: artists } = $api.useQuery("get", "/api/artist");

  if (isLoading) {
    return (
      <div className="space-y-6">
        <div>
          <h1 className="text-2xl font-bold tracking-tight">Albums</h1>
          <Skeleton className="mt-1 h-4 w-48" />
        </div>
        <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4">
          {Array.from({ length: 8 }).map((_, i) => (
            <div
              key={i}
              className="overflow-hidden rounded-xl border bg-card shadow-sm"
            >
              <Skeleton className="aspect-square w-full" />
              <div className="p-4">
                <Skeleton className="mb-2 h-4 w-28" />
                <Skeleton className="mb-1 h-3 w-36" />
                <Skeleton className="h-3 w-20" />
              </div>
            </div>
          ))}
        </div>
      </div>
    );
  }

  if (isError || !albums) {
    return (
      <div className="space-y-6">
        <div>
          <h1 className="text-2xl font-bold tracking-tight">Albums</h1>
        </div>
        <div className="rounded-xl border border-red-200 bg-red-50 p-4 text-sm text-red-700 dark:border-red-900/50 dark:bg-red-950/50 dark:text-red-400">
          Failed to load albums.
        </div>
      </div>
    );
  }

  const artistMap = new Map((artists ?? []).map((a) => [a.id, a.name]));

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-bold tracking-tight">Albums</h1>
        <p className="text-muted-foreground">
          {albums.length} album{albums.length !== 1 ? "s" : ""} in your library.
        </p>
      </div>

      {albums.length === 0 ? (
        <div className="flex flex-col items-center justify-center rounded-xl border border-dashed bg-muted/30 py-20">
          <p className="text-sm text-muted-foreground">
            No albums yet. Add some from the search page or sync an artist.
          </p>
        </div>
      ) : (
        <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4">
          {albums.map((album) => {
            const artistName =
              album.artist_credits?.[0]?.name ??
              artistMap.get(album.artist_id) ??
              "Unknown Artist";

            return (
              <Link
                key={album.id}
                to="/artists/$artistId/albums/$albumId"
                params={{
                  artistId: album.artist_id,
                  albumId: album.id,
                }}
                className="group overflow-hidden rounded-xl border bg-card shadow-sm transition-shadow hover:shadow-md"
              >
                <div className="aspect-square bg-muted">
                  {album.cover_url ? (
                    <img
                      src={album.cover_url}
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
                    {artistName} &middot;{" "}
                    {album.release_date?.slice(0, 4) ?? "Unknown"}
                  </p>
                  <div className="mt-2 flex flex-wrap items-center gap-1.5">
                    {album.album_type && (
                      <span className="rounded-full bg-muted px-2 py-0.5 text-xs font-medium text-muted-foreground">
                        {album.album_type}
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
                    {album.partially_wanted && !album.wanted && (
                      <span className="rounded-full bg-amber-500/10 px-2 py-0.5 text-xs font-medium text-amber-400">
                        Partial
                      </span>
                    )}
                  </div>
                </div>
              </Link>
            );
          })}
        </div>
      )}
    </div>
  );
}
