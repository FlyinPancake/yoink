import { useState } from "react";
import { Link, createFileRoute } from "@tanstack/react-router";
import {
  ArrowLeftIcon,
  CheckIcon,
  DownloadIcon,
  ExternalLinkIcon,
  RefreshCwIcon,
  Trash2Icon,
} from "lucide-react";

import type { components } from "@/lib/api/types.gen";
import { $api, queryKeys } from "@/lib/api";
import {
  formatDurationSeconds,
  isAlbumAcquired,
  isAlbumInProgress,
  isAlbumWanted,
} from "@/lib/music";
import {
  useAcceptMatchSuggestion,
  useDismissMatchSuggestion,
  useRemoveAlbumFiles,
  useRetryDownload,
  useSetAlbumQuality,
  useSetTrackQuality,
  useToggleAlbumMonitor,
  useToggleSingleTrackMonitor,
} from "@/lib/api/mutations";
import { MonitorButton } from "@/components/monitor-button";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/components/ui/alert-dialog";

type Album = components["schemas"]["Album"] & {
  quality_override?: components["schemas"]["Quality"] | null;
};
type TrackInfo = components["schemas"]["TrackInfo"];
type ProviderLink = components["schemas"]["ProviderLink"];
type AlbumMatchSuggestion = components["schemas"]["AlbumMatchSuggestion"];
type DownloadJob = components["schemas"]["DownloadJob"];
type Quality = components["schemas"]["Quality"];
type ArtistWithPriority = components["schemas"]["ArtistWithPriority"];

export const Route = createFileRoute("/_app/artists/$artistId/albums/$albumId")({
  component: AlbumDetailPage,
  loader: async ({ context, params }) =>
    context.queryClient.ensureQueryData(queryKeys.albums.detail(params.albumId)),
  staticData: {
    breadcrumb: (match) =>
      (match.loaderData as { album?: { title?: string } } | undefined)?.album?.title ?? "Album",
  },
});

// ── Helpers ────────────────────────────────────────────────────

function providerDisplayName(provider: string): string {
  const map: Record<string, string> = {
    musicbrainz: "MusicBrainz",
    spotify: "Spotify",
    tidal: "Tidal",
    deezer: "Deezer",
    qobuz: "Qobuz",
    lastfm: "Last.fm",
  };
  return map[provider] ?? provider;
}

function albumTypeLabel(albumType: string | null | undefined): string {
  if (!albumType) return "Album";
  const map: Record<string, string> = {
    album: "Album",
    single: "Single",
    ep: "EP",
    compilation: "Compilation",
    live: "Live",
    remix: "Remix",
    soundtrack: "Soundtrack",
    other: "Other",
  };
  return map[albumType.toLowerCase()] ?? albumType;
}

function fallbackInitial(name: string): string {
  return name.charAt(0).toUpperCase() || "?";
}

function formatDuration(totalSecs: number): string {
  const totalMins = Math.floor(totalSecs / 60);
  const secs = totalSecs % 60;
  if (totalMins >= 60) {
    const hrs = Math.floor(totalMins / 60);
    const mins = totalMins % 60;
    return `${String(hrs)} hr ${String(mins)} min`;
  }
  return `${String(totalMins)} min ${String(secs).padStart(2, "0")} sec`;
}

// ── Page component ─────────────────────────────────────────────

function AlbumDetailPage() {
  const { artistId, albumId } = Route.useParams();
  const { data, isLoading, isError } = $api.useQuery("get", "/api/album/{album_id}", {
    params: { path: { album_id: albumId } },
  });

  if (isLoading) {
    return <AlbumDetailSkeleton />;
  }

  if (isError) {
    return (
      <div className="p-6">
        <div className="rounded-xl border border-red-200 bg-red-50 p-4 text-sm text-red-700 dark:border-red-900/50 dark:bg-red-950/50 dark:text-red-400">
          Failed to load album details.
        </div>
      </div>
    );
  }

  if (!data) {
    return (
      <div className="p-6">
        <p className="text-muted-foreground">Album not found.</p>
        <Button asChild className="mt-4" size="lg">
          <Link to="/artists/$artistId" params={{ artistId }}>
            <ArrowLeftIcon className="mr-2 size-4" />
            Back to Artist
          </Link>
        </Button>
      </div>
    );
  }

  return (
    <AlbumDetailContent
      album={data.album}
      albumArtists={data.album_artists}
      tracks={data.tracks}
      jobs={data.jobs}
      providerLinks={data.provider_links}
      matchSuggestions={data.album_match_suggestions}
      defaultQuality={data.default_quality}
      artistIdParam={artistId}
    />
  );
}

// ── Skeleton ───────────────────────────────────────────────────

function AlbumDetailSkeleton() {
  return (
    <div className="p-6 max-md:p-4">
      <div className="mb-5 rounded-xl border bg-card p-5">
        <div className="flex animate-pulse flex-col gap-6 md:flex-row">
          <Skeleton className="size-60 shrink-0 rounded-xl max-md:mx-auto max-md:size-48" />
          <div className="min-w-0 flex-1">
            <Skeleton className="mb-3 h-4 w-32" />
            <Skeleton className="mb-3 h-7 w-56" />
            <Skeleton className="mb-4 h-3.5 w-40" />
            <div className="mb-4 flex flex-wrap gap-1.5">
              {Array.from({ length: 4 }).map((_, i) => (
                <Skeleton key={i} className="h-7 w-20 rounded-lg" />
              ))}
            </div>
            <div className="flex flex-wrap gap-1.5">
              {Array.from({ length: 3 }).map((_, i) => (
                <Skeleton key={i} className="h-8 w-24 rounded-lg" />
              ))}
            </div>
          </div>
        </div>
      </div>
      <div className="overflow-hidden rounded-xl border bg-card">
        <div className="border-b px-5 py-3">
          <Skeleton className="h-4 w-24" />
        </div>
        <div className="p-5">
          {Array.from({ length: 8 }).map((_, i) => (
            <div key={i} className="mb-2.5 flex animate-pulse gap-3">
              <Skeleton className="h-3.5 w-6" />
              <Skeleton className="h-3.5 flex-1" />
              <Skeleton className="h-3.5 w-10" />
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

// ── Main content ───────────────────────────────────────────────

function AlbumDetailContent({
  album,
  albumArtists,
  tracks,
  jobs,
  providerLinks,
  matchSuggestions,
  defaultQuality,
  artistIdParam,
}: {
  album: Album;
  albumArtists: Array<ArtistWithPriority>;
  tracks: Array<TrackInfo>;
  jobs: Array<DownloadJob>;
  providerLinks: Array<ProviderLink>;
  matchSuggestions: Array<AlbumMatchSuggestion>;
  defaultQuality: Quality;
  artistIdParam: string;
}) {
  const [showRemoveFiles, setShowRemoveFiles] = useState(false);

  const toggleAlbumMonitor = useToggleAlbumMonitor();
  const removeAlbumFiles = useRemoveAlbumFiles();
  const retryDownload = useRetryDownload();
  const setAlbumQuality = useSetAlbumQuality();

  const releaseDate = album.release_date ?? "\u2014";
  const at = albumTypeLabel(album.album_type);
  const totalDurationSecs = tracks.reduce((sum, t) => sum + t.duration_secs, 0);
  const durationDisplay = formatDuration(totalDurationSecs);
  const trackCount = tracks.length;

  const artistName = albumArtists[0]?.name ?? "Unknown Artist";

  // Find the latest job for this album
  const latestJob = jobs
    .filter((j) => j.album_id === album.id)
    .sort((a, b) => b.updated_at.localeCompare(a.updated_at))[0] as DownloadJob | undefined;

  const hasActiveJob =
    latestJob && ["queued", "resolving", "downloading"].includes(latestJob.status);
  const canDownload =
    isAlbumWanted(album.wanted_status) && !isAlbumAcquired(album.wanted_status) && !hasActiveJob;
  const canRetry = latestJob?.status === "failed";

  const pendingSuggestions = matchSuggestions.filter((m) => m.status === "pending");

  return (
    <div className="p-6 max-md:p-4">
      {/* ── Hero card ──────────────────────────────────────── */}
      <div className="mb-5 overflow-hidden rounded-xl border bg-card">
        {/* Status + action buttons header bar */}
        <div className="flex flex-wrap items-center justify-between gap-2 border-b px-5 py-3">
          <div className="flex flex-wrap items-center gap-2">
            {isAlbumAcquired(album.wanted_status) && (
              <Badge className="bg-green-500/10 text-green-600">Acquired</Badge>
            )}
            {!isAlbumAcquired(album.wanted_status) && isAlbumWanted(album.wanted_status) && (
              <Badge className="bg-amber-500/10 text-amber-500">Wanted</Badge>
            )}
            {!isAlbumAcquired(album.wanted_status) && isAlbumInProgress(album.wanted_status) && (
              <Badge variant="outline" className="text-blue-500">
                In Progress
              </Badge>
            )}
            {latestJob && <JobStatusBadge job={latestJob} />}
          </div>
          <div className="flex flex-wrap items-center gap-2">
            {canRetry && (
              <Button
                size="sm"
                disabled={retryDownload.isPending}
                onClick={() =>
                  retryDownload.mutate({
                    params: { path: { album_id: album.id } },
                  })
                }
              >
                <RefreshCwIcon
                  className={`mr-1.5 size-3.5 ${retryDownload.isPending ? "animate-spin" : ""}`}
                />
                {retryDownload.isPending ? "Retrying..." : "Retry"}
              </Button>
            )}
            {canDownload && !canRetry && (
              <Button
                size="sm"
                disabled={retryDownload.isPending}
                onClick={() =>
                  retryDownload.mutate({
                    params: { path: { album_id: album.id } },
                  })
                }
              >
                <DownloadIcon className="mr-1.5 size-3.5" />
                {retryDownload.isPending ? "Starting..." : "Download"}
              </Button>
            )}
            <MonitorButton
              monitored={album.monitored}
              onToggleMonitor={() =>
                toggleAlbumMonitor.mutate({
                  params: { path: { album_id: album.id } },
                  body: { monitored: !album.monitored },
                })
              }
              qualityOverride={album.quality_override ?? null}
              defaultQuality={defaultQuality}
              onQualityChange={(quality) =>
                setAlbumQuality.mutate({
                  params: { path: { album_id: album.id } },
                  body: { quality },
                })
              }
              pending={toggleAlbumMonitor.isPending || setAlbumQuality.isPending}
            />
            {isAlbumAcquired(album.wanted_status) && (
              <Button variant="destructive" size="sm" onClick={() => setShowRemoveFiles(true)}>
                <Trash2Icon className="mr-1.5 size-3.5" />
                Remove Files
              </Button>
            )}
          </div>
        </div>

        {/* Job error banner */}
        {latestJob?.error && (
          <div className="border-b border-red-500/20 bg-red-500/6 px-5 py-2 text-sm text-red-600 dark:bg-red-500/10 dark:text-red-400">
            {latestJob.error}
          </div>
        )}

        {/* Body: cover art + identity info */}
        <div className="p-5 md:p-6">
          <div className="flex gap-5">
            {/* Cover art */}
            <div className="size-28 shrink-0 overflow-hidden rounded-lg bg-muted md:size-40">
              {album.cover_url ? (
                <img className="size-full object-cover" src={album.cover_url} alt="" />
              ) : (
                <div className="flex size-full items-center justify-center text-3xl font-bold text-muted-foreground/30">
                  {fallbackInitial(album.title)}
                </div>
              )}
            </div>

            {/* Core identity */}
            <div className="flex min-w-0 flex-1 flex-col justify-center">
              {/* Album type + explicit */}
              <div className="mb-1 flex items-center gap-2">
                <span className="text-[11px] font-semibold tracking-wider text-muted-foreground uppercase">
                  {at}
                </span>
                {album.explicit && (
                  <span className="rounded bg-muted px-1.5 py-0 text-[10px] font-medium text-muted-foreground">
                    Explicit
                  </span>
                )}
              </div>

              {/* Title */}
              <h1 className="m-0 mb-1.5 text-xl leading-snug font-bold wrap-break-word md:text-2xl">
                {album.title}
              </h1>

              {/* Artist(s) / date / tracks */}
              <div className="flex flex-wrap items-center gap-1.5 text-sm text-muted-foreground">
                {albumArtists.length === 0 ? (
                  <Link
                    to="/artists/$artistId"
                    params={{ artistId: artistIdParam }}
                    className="font-medium text-foreground/70 no-underline hover:text-blue-500"
                  >
                    {artistName}
                  </Link>
                ) : (
                  <span className="inline-flex flex-wrap items-center gap-0">
                    {albumArtists.map((credit, i) => (
                      <span key={credit.id ?? `${credit.name}-${String(i)}`}>
                        {i > 0 && ", "}
                        {credit.id ? (
                          <Link
                            to="/artists/$artistId"
                            params={{
                              artistId: credit.id,
                            }}
                            className="font-medium text-foreground/70 no-underline hover:text-blue-500"
                          >
                            {credit.name}
                          </Link>
                        ) : (
                          <span className="text-muted-foreground/60 italic">{credit.name}</span>
                        )}
                      </span>
                    ))}
                  </span>
                )}
                <span>&middot;</span>
                <span>{releaseDate}</span>
                <span>&middot;</span>
                <span>
                  {trackCount} tracks, {durationDisplay}
                </span>
              </div>

              {/* Provider links */}
              {providerLinks.length > 0 && (
                <div className="mt-2 flex flex-wrap items-center gap-1.5">
                  <span className="text-[11px] text-muted-foreground/60">Available on</span>
                  {providerLinks.map((link) => (
                    <span key={`${link.provider}-${link.external_id}`}>
                      {link.external_url ? (
                        <a
                          href={link.external_url}
                          target="_blank"
                          rel="noreferrer"
                          className="inline-flex items-center gap-1"
                        >
                          <Badge variant="outline">
                            {providerDisplayName(link.provider)}
                            <ExternalLinkIcon className="ml-0.5 size-2.5" />
                          </Badge>
                        </a>
                      ) : (
                        <Badge variant="outline">{providerDisplayName(link.provider)}</Badge>
                      )}
                    </span>
                  ))}
                </div>
              )}
            </div>
          </div>
        </div>
      </div>

      {/* ── Potential matches ──────────────────────────────── */}
      {pendingSuggestions.length > 0 && (
        <AlbumMatchSuggestionsPanel suggestions={pendingSuggestions} />
      )}

      {/* ── Tracklist ──────────────────────────────────────── */}
      <div className="overflow-hidden rounded-xl border bg-card">
        <div className="flex items-center gap-3 border-b px-5 py-3">
          <h2 className="text-sm font-semibold">Tracklist</h2>
          <span className="text-xs text-muted-foreground">
            {trackCount} tracks &middot; {durationDisplay}
          </span>
        </div>

        {tracks.length === 0 ? (
          <div className="px-4 py-10 text-center text-sm text-muted-foreground">
            No tracks available.
          </div>
        ) : (
          <TrackList
            tracks={tracks}
            albumId={album.id}
            albumArtists={albumArtists}
            artistName={artistName}
            effectiveAlbumQuality={album.quality_override ?? defaultQuality}
          />
        )}
      </div>

      {/* ── Remove files dialog ────────────────────────────── */}
      <AlertDialog open={showRemoveFiles} onOpenChange={setShowRemoveFiles}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Remove Files</AlertDialogTitle>
            <AlertDialogDescription>
              This will delete all downloaded files for &ldquo;{album.title}
              &rdquo; from disk.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction
              variant="destructive"
              disabled={removeAlbumFiles.isPending}
              onClick={() =>
                removeAlbumFiles.mutate({
                  params: {
                    path: { album_id: album.id },
                    query: { unmonitor: false },
                  },
                })
              }
            >
              {removeAlbumFiles.isPending ? "Removing..." : "Remove"}
            </AlertDialogAction>
            <AlertDialogAction
              variant="destructive"
              disabled={removeAlbumFiles.isPending}
              onClick={() =>
                removeAlbumFiles.mutate({
                  params: {
                    path: { album_id: album.id },
                    query: { unmonitor: true },
                  },
                })
              }
            >
              {removeAlbumFiles.isPending ? "Removing..." : "Remove & Unmonitor"}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </div>
  );
}

// ── Job status badge ───────────────────────────────────────────

function JobStatusBadge({ job }: { job: DownloadJob }) {
  const colorMap: Record<string, string> = {
    queued: "bg-amber-500/10 text-amber-500",
    resolving: "bg-violet-500/10 text-violet-500",
    downloading: "bg-blue-500/10 text-blue-500",
    completed: "bg-green-500/10 text-green-500",
    failed: "bg-red-500/10 text-red-500",
  };

  let label = job.status as string;
  if (job.status === "downloading") {
    label = `Downloading ${String(job.completed_tracks)}/${String(job.total_tracks)}`;
  }

  return (
    <span
      className={`inline-flex items-center rounded-full px-2 py-0.5 text-xs font-medium capitalize ${colorMap[job.status] ?? ""}`}
    >
      {label}
    </span>
  );
}

// ── Track list ─────────────────────────────────────────────────

function TrackList({
  tracks,
  albumId,
  albumArtists,
  artistName,
  effectiveAlbumQuality,
}: {
  tracks: Array<TrackInfo>;
  albumId: string;
  albumArtists: Array<ArtistWithPriority>;
  artistName: string;
  effectiveAlbumQuality: Quality;
}) {
  const hasMultipleDiscs = tracks.some((t) => t.disc_number > 1);
  const hasAnyArtist = tracks.some((t) => t.track_artist);
  const hasAnyPath = tracks.some((t) => t.file_path);

  // Build set of album-level artist names to suppress in track rows
  const albumArtistNames = new Set([artistName, ...albumArtists.map((c) => c.name)]);

  return (
    <div className="divide-y divide-border/50">
      {tracks.map((track) => {
        const trackNumDisplay = hasMultipleDiscs
          ? `${String(track.disc_number)}-${String(track.track_number)}`
          : String(track.track_number);

        // Show track artist only if it contains names not in the album artist list
        const showTrackArtist =
          hasAnyArtist &&
          track.track_artist
            ?.split(/[;,]/)
            .flatMap((s) => s.split(" & "))
            .some((name) => !albumArtistNames.has(name.trim()));

        return (
          <TrackRow
            key={track.id}
            track={track}
            albumId={albumId}
            trackNumDisplay={trackNumDisplay}
            showTrackArtist={!!showTrackArtist}
            hasAnyPath={hasAnyPath}
            effectiveAlbumQuality={effectiveAlbumQuality}
          />
        );
      })}
    </div>
  );
}

// ── Track row ──────────────────────────────────────────────────

function TrackRow({
  track,
  albumId,
  trackNumDisplay,
  showTrackArtist,
  hasAnyPath,
  effectiveAlbumQuality,
}: {
  track: TrackInfo;
  albumId: string;
  trackNumDisplay: string;
  showTrackArtist: boolean;
  hasAnyPath: boolean;
  effectiveAlbumQuality: Quality;
}) {
  const toggleTrackMonitor = useToggleSingleTrackMonitor();
  const setTrackQuality = useSetTrackQuality();

  return (
    <div className="flex items-center gap-3 px-5 py-2.5 transition-colors duration-100 hover:bg-blue-500/3 dark:hover:bg-blue-500/5">
      {/* Track status indicator */}
      <TrackStatusIndicator
        trackId={track.id}
        albumId={albumId}
        monitored={track.monitored}
        acquired={track.acquired}
        qualityOverride={track.quality_override ?? null}
        effectiveAlbumQuality={effectiveAlbumQuality}
        toggleMutation={toggleTrackMonitor}
        setTrackQuality={setTrackQuality}
      />

      <span className="w-8 shrink-0 text-right text-xs text-muted-foreground tabular-nums">
        {trackNumDisplay}
      </span>

      <div className="min-w-0 flex-1">
        <div className="flex flex-wrap items-center gap-1.5">
          <span className="truncate text-sm">{track.title}</span>
          {track.version && (
            <span className="shrink-0 text-xs text-muted-foreground">({track.version})</span>
          )}
          {track.explicit && (
            <span className="inline-flex shrink-0 items-center justify-center rounded bg-muted px-1 py-px text-[9px] leading-none font-bold tracking-wide text-muted-foreground uppercase">
              E
            </span>
          )}
        </div>
        {showTrackArtist && track.track_artist && (
          <span className="mt-0.5 block truncate text-[11px] leading-tight text-muted-foreground">
            {track.track_artist}
          </span>
        )}
        {/* ISRC + file path */}
        {(track.isrc ?? track.file_path) && (
          <div className="mt-0.5 flex flex-wrap items-center gap-2">
            {track.isrc && (
              <span className="font-mono text-[10px] leading-tight text-muted-foreground/70">
                {track.isrc}
              </span>
            )}
            {hasAnyPath && track.file_path && (
              <span
                className="max-w-100 truncate font-mono text-[10px] leading-tight text-muted-foreground/50"
                title={track.file_path}
              >
                {track.file_path}
              </span>
            )}
          </div>
        )}
      </div>

      <span className="shrink-0 text-xs text-muted-foreground tabular-nums">
        {formatDurationSeconds(track.duration_secs)}
      </span>
    </div>
  );
}

// ── Track status indicator ─────────────────────────────────────

function TrackStatusIndicator({
  trackId,
  albumId,
  monitored,
  acquired,
  qualityOverride,
  effectiveAlbumQuality,
  toggleMutation,
  setTrackQuality,
}: {
  trackId: string;
  albumId: string;
  monitored: boolean;
  acquired: boolean;
  qualityOverride: Quality | null;
  effectiveAlbumQuality: Quality;
  toggleMutation: ReturnType<typeof useToggleSingleTrackMonitor>;
  setTrackQuality: ReturnType<typeof useSetTrackQuality>;
}) {
  if (acquired) {
    return (
      <span
        className="flex size-5 shrink-0 items-center justify-center text-green-500 dark:text-green-400"
        title="Acquired"
      >
        <CheckIcon className="size-3.5" />
      </span>
    );
  }

  return (
    <div
      className="shrink-0"
      onClick={(e) => e.stopPropagation()}
      onPointerDown={(e) => e.stopPropagation()}
    >
      <MonitorButton
        variant="compact"
        monitored={monitored}
        onToggleMonitor={() =>
          toggleMutation.mutate({
            params: {
              path: { album_id: albumId, track_id: trackId },
            },
            body: { monitored: !monitored },
          })
        }
        qualityOverride={qualityOverride}
        defaultQuality={effectiveAlbumQuality}
        onQualityChange={(quality) =>
          setTrackQuality.mutate({
            params: {
              path: { album_id: albumId, track_id: trackId },
            },
            body: { quality },
          })
        }
        pending={toggleMutation.isPending || setTrackQuality.isPending}
      />
    </div>
  );
}

// ── Album match suggestions panel ──────────────────────────────

function AlbumMatchSuggestionsPanel({ suggestions }: { suggestions: Array<AlbumMatchSuggestion> }) {
  const acceptMatch = useAcceptMatchSuggestion();
  const dismissMatch = useDismissMatchSuggestion();

  return (
    <div className="mb-5 overflow-hidden rounded-xl border bg-card">
      <div className="flex items-center gap-2 border-b px-5 py-3">
        <h2 className="text-sm font-semibold">Potential Matches</h2>
      </div>
      <div className="flex flex-col gap-2 px-5 py-3">
        {suggestions.map((m) => {
          const displayProvider = providerDisplayName(m.right_provider);
          const displayName = m.external_name ?? "Unknown album match";
          const kind = m.match_kind === "isrc_exact" ? "ISRC" : "Fuzzy";

          return (
            <div key={m.id} className="flex items-start gap-3 text-xs">
              <div className="flex min-w-0 flex-1 flex-wrap items-center gap-2">
                <span className="inline-flex items-center rounded-md border bg-card px-1.5 py-0.5">
                  {kind} {m.confidence}%
                </span>
                <span>
                  {displayProvider}: {displayName}
                </span>
              </div>
              <div className="flex shrink-0 items-center gap-1.5">
                <Button
                  size="sm"
                  disabled={acceptMatch.isPending}
                  onClick={() =>
                    acceptMatch.mutate({
                      params: { path: { suggestion_id: m.id } },
                    })
                  }
                >
                  <CheckIcon className="mr-1 size-3" />
                  Accept
                </Button>
                <Button
                  variant="outline"
                  size="sm"
                  disabled={dismissMatch.isPending}
                  onClick={() =>
                    dismissMatch.mutate({
                      params: { path: { suggestion_id: m.id } },
                    })
                  }
                >
                  Dismiss
                </Button>
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}
