import { createFileRoute, Link } from "@tanstack/react-router";
import { $api } from "@/lib/api";
import { Skeleton } from "@/components/ui/skeleton";

export const Route = createFileRoute("/_app/library/artists")({
  component: ArtistsPage,
  staticData: {
    breadcrumb: "Artists",
  },
});

function ArtistsPage() {
  const {
    data: artists,
    isLoading,
    isError,
  } = $api.useQuery("get", "/api/artist");

  if (isLoading) {
    return (
      <div className="space-y-6">
        <div>
          <h1 className="text-2xl font-bold tracking-tight">Artists</h1>
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
                <Skeleton className="h-3 w-20" />
              </div>
            </div>
          ))}
        </div>
      </div>
    );
  }

  if (isError || !artists) {
    return (
      <div className="space-y-6">
        <div>
          <h1 className="text-2xl font-bold tracking-tight">Artists</h1>
        </div>
        <div className="rounded-xl border border-red-200 bg-red-50 p-4 text-sm text-red-700 dark:border-red-900/50 dark:bg-red-950/50 dark:text-red-400">
          Failed to load artists.
        </div>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-bold tracking-tight">Artists</h1>
        <p className="text-muted-foreground">
          {artists.length} artist{artists.length !== 1 ? "s" : ""} in your
          library.
        </p>
      </div>

      {artists.length === 0 ? (
        <div className="flex flex-col items-center justify-center rounded-xl border border-dashed bg-muted/30 py-20">
          <p className="text-sm text-muted-foreground">
            No artists yet. Add some from the search page.
          </p>
        </div>
      ) : (
        <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4">
          {artists.map((artist) => (
            <Link
              key={artist.id}
              to="/artists/$artistId"
              params={{ artistId: artist.id }}
              className="group relative overflow-hidden rounded-xl border bg-card shadow-sm transition-shadow hover:shadow-md"
            >
              <div className="aspect-square bg-muted">
                {artist.image_url ? (
                  <img
                    src={artist.image_url}
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
                <div className="mt-2 flex items-center gap-2">
                  {artist.monitored ? (
                    <span className="rounded-full bg-green-500/10 px-2 py-0.5 text-xs font-medium text-green-500">
                      Monitored
                    </span>
                  ) : (
                    <span className="rounded-full bg-muted px-2 py-0.5 text-xs font-medium text-muted-foreground">
                      Lightweight
                    </span>
                  )}
                </div>
              </div>
            </Link>
          ))}
        </div>
      )}
    </div>
  );
}
