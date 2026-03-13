import { createFileRoute } from "@tanstack/react-router";
import { CheckIcon, XIcon } from "lucide-react";
import { tracks } from "@/lib/mock-data";

export const Route = createFileRoute("/_app/library/tracks")({
  component: TracksPage,
});

function TracksPage() {
  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-bold tracking-tight">Tracks</h1>
        <p className="text-muted-foreground">
          {tracks.length} tracks in your library.
        </p>
      </div>

      <div className="overflow-hidden rounded-xl border bg-card shadow-sm">
        <table className="w-full text-sm">
          <thead>
            <tr className="border-b bg-muted/50 text-left text-xs font-medium uppercase tracking-wider text-muted-foreground">
              <th className="px-4 py-3 w-10">#</th>
              <th className="px-4 py-3">Title</th>
              <th className="px-4 py-3 hidden md:table-cell">Album</th>
              <th className="px-4 py-3 hidden lg:table-cell">Artist</th>
              <th className="px-4 py-3 w-20 text-right">Duration</th>
              <th className="px-4 py-3 w-20 text-center">Status</th>
            </tr>
          </thead>
          <tbody className="divide-y">
            {tracks.map((track) => (
              <tr
                key={track.id}
                className="transition-colors hover:bg-muted/30"
              >
                <td className="px-4 py-2.5 tabular-nums text-muted-foreground">
                  {track.trackNumber}
                </td>
                <td className="px-4 py-2.5">
                  <span className="font-medium">{track.title}</span>
                  {track.explicit && (
                    <span className="ml-1.5 inline-flex items-center justify-center rounded bg-muted px-1 text-[10px] font-bold uppercase text-muted-foreground">
                      E
                    </span>
                  )}
                </td>
                <td className="hidden px-4 py-2.5 text-muted-foreground md:table-cell">
                  {track.albumTitle}
                </td>
                <td className="hidden px-4 py-2.5 text-muted-foreground lg:table-cell">
                  {track.artistName}
                </td>
                <td className="px-4 py-2.5 text-right tabular-nums text-muted-foreground">
                  {track.durationDisplay}
                </td>
                <td className="px-4 py-2.5 text-center">
                  {track.acquired ? (
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
    </div>
  );
}
