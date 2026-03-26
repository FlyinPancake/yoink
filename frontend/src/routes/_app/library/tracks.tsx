import { createFileRoute, Link } from "@tanstack/react-router";
import { CheckIcon, XIcon } from "lucide-react";
import { $api } from "@/lib/api";
import { formatDurationSeconds } from "@/lib/music";
import { Skeleton } from "@/components/ui/skeleton";

export const Route = createFileRoute("/_app/library/tracks")({
  component: TracksPage,
  staticData: {
    breadcrumb: "Tracks",
  },
});

function TracksPage() {
  const { data: tracks, isLoading, isError } = $api.useQuery("get", "/api/track");

  if (isLoading) {
    return (
      <div className="space-y-6">
        <div>
          <h1 className="text-2xl font-bold tracking-tight">Tracks</h1>
          <Skeleton className="mt-1 h-4 w-48" />
        </div>
        <div className="overflow-hidden rounded-xl border bg-card shadow-sm">
          <div className="space-y-3 p-4">
            {Array.from({ length: 10 }).map((_, i) => (
              <Skeleton key={i} className="h-8 w-full" />
            ))}
          </div>
        </div>
      </div>
    );
  }

  if (isError || !tracks) {
    return (
      <div className="space-y-6">
        <div>
          <h1 className="text-2xl font-bold tracking-tight">Tracks</h1>
        </div>
        <div className="rounded-xl border border-red-200 bg-red-50 p-4 text-sm text-red-700 dark:border-red-900/50 dark:bg-red-950/50 dark:text-red-400">
          Failed to load tracks.
        </div>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-bold tracking-tight">Tracks</h1>
        <p className="text-muted-foreground">
          {tracks.length} track{tracks.length !== 1 ? "s" : ""} in your library.
        </p>
      </div>

      {tracks.length === 0 ? (
        <div className="flex flex-col items-center justify-center rounded-xl border border-dashed bg-muted/30 py-20">
          <p className="text-sm text-muted-foreground">
            No tracks yet. Sync an artist or add albums to get started.
          </p>
        </div>
      ) : (
        <div className="overflow-hidden rounded-xl border bg-card shadow-sm">
          <table className="w-full text-sm">
            <thead>
              <tr className="border-b bg-muted/50 text-left text-xs font-medium tracking-wider text-muted-foreground uppercase">
                <th className="w-10 px-4 py-3">#</th>
                <th className="px-4 py-3">Title</th>
                <th className="hidden px-4 py-3 md:table-cell">Album</th>
                <th className="hidden px-4 py-3 lg:table-cell">Artist</th>
                <th className="w-20 px-4 py-3 text-right">Duration</th>
                <th className="w-20 px-4 py-3 text-center">Status</th>
              </tr>
            </thead>
            <tbody className="divide-y">
              {tracks.map((lt) => (
                <tr key={lt.track.id} className="transition-colors hover:bg-muted/30">
                  <td className="px-4 py-2.5 text-muted-foreground tabular-nums">
                    {lt.track.track_number}
                  </td>
                  <td className="px-4 py-2.5">
                    <span className="font-medium">{lt.track.title}</span>
                    {lt.track.explicit && (
                      <span className="ml-1.5 inline-flex items-center justify-center rounded bg-muted px-1 text-[10px] font-bold text-muted-foreground uppercase">
                        E
                      </span>
                    )}
                  </td>
                  <td className="hidden px-4 py-2.5 text-muted-foreground md:table-cell">
                    <Link
                      to="/artists/$artistId/albums/$albumId"
                      params={{
                        artistId: lt.artist_id,
                        albumId: lt.album_id,
                      }}
                      className="hover:text-foreground hover:underline"
                    >
                      {lt.album_title}
                    </Link>
                  </td>
                  <td className="hidden px-4 py-2.5 text-muted-foreground lg:table-cell">
                    <Link
                      to="/artists/$artistId"
                      params={{ artistId: lt.artist_id }}
                      className="hover:text-foreground hover:underline"
                    >
                      {lt.artist_name}
                    </Link>
                  </td>
                  <td className="px-4 py-2.5 text-right text-muted-foreground tabular-nums">
                    {formatDurationSeconds(lt.track.duration_secs)}
                  </td>
                  <td className="px-4 py-2.5 text-center">
                    {lt.track.acquired ? (
                      <CheckIcon className="mx-auto size-4 text-green-500" />
                    ) : (
                      <XIcon className="mx-auto size-4 text-muted-foreground/40" />
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
