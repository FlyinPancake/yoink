import { useMemo, useState } from "react";
import { Link, createFileRoute } from "@tanstack/react-router";
import { ArrowLeftIcon, GitMergeIcon, XIcon } from "lucide-react";

import type { components } from "@/lib/api/types.gen";
import { $api } from "@/lib/api";
import { useDismissMatchSuggestion, useMergeAlbums } from "@/lib/api/mutations";

import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";

type MonitoredAlbum = components["schemas"]["MonitoredAlbum"];
type MatchSuggestion = components["schemas"]["MatchSuggestion"];

export const Route = createFileRoute("/_app/artists/$artistId/merge-albums")({
  component: MergeAlbumsPage,
  staticData: {
    breadcrumb: "Merge Albums",
  },
});

// ── Helpers ────────────────────────────────────────────────────

interface MergeCandidate {
  suggestion: MatchSuggestion;
  leftAlbum: MonitoredAlbum;
  rightAlbum: MonitoredAlbum;
}

function fallbackInitial(name: string): string {
  return name.charAt(0).toUpperCase() || "?";
}

// ── Page ───────────────────────────────────────────────────────

function MergeAlbumsPage() {
  const { artistId } = Route.useParams();

  // Fetch artist detail (includes albums + match suggestions)
  const { data, isLoading, isError } = $api.useQuery("get", "/api/artist/{artist_id}", {
    params: { path: { artist_id: artistId } },
  });

  if (isLoading) {
    return <MergeAlbumsSkeleton artistId={artistId} />;
  }

  if (isError || !data) {
    return (
      <div className="p-6 max-md:p-4">
        <div className="rounded-xl border border-red-200 bg-red-50 p-4 text-sm text-red-700 dark:border-red-900/50 dark:bg-red-950/50 dark:text-red-400">
          Failed to load artist data for merge candidates.
        </div>
      </div>
    );
  }

  return (
    <MergeAlbumsContent
      artistId={artistId}
      artistName={data.artist.name}
      albums={data.albums}
      matchSuggestions={data.match_suggestions}
    />
  );
}

// ── Skeleton ───────────────────────────────────────────────────

function MergeAlbumsSkeleton({ artistId }: { artistId: string }) {
  return (
    <div className="p-6 max-md:p-4">
      <div className="mb-6 flex items-center gap-3">
        <Button variant="outline" size="sm" asChild>
          <Link to="/artists/$artistId" params={{ artistId }}>
            <ArrowLeftIcon className="mr-1.5 size-3.5" />
            Back
          </Link>
        </Button>
        <Skeleton className="h-6 w-48" />
      </div>
      <div className="space-y-4">
        {Array.from({ length: 3 }).map((_, i) => (
          <Skeleton key={i} className="h-40 w-full rounded-xl" />
        ))}
      </div>
    </div>
  );
}

// ── Content ────────────────────────────────────────────────────

function MergeAlbumsContent({
  artistId,
  artistName,
  albums,
  matchSuggestions,
}: {
  artistId: string;
  artistName: string;
  albums: Array<MonitoredAlbum>;
  matchSuggestions: Array<MatchSuggestion>;
}) {
  // Build merge candidates from album-scoped pending match suggestions
  const candidates = useMemo(() => {
    const albumMap = new Map(albums.map((a) => [a.id, a]));
    const results: Array<MergeCandidate> = [];

    // Album-level merge suggestions have scope_type === "album"
    // left_external_id and right_external_id are album UUIDs
    const pending = matchSuggestions.filter(
      (m) => m.status === "pending" && m.scope_type === "album",
    );

    for (const suggestion of pending) {
      const left = albumMap.get(suggestion.left_external_id);
      const right = albumMap.get(suggestion.right_external_id);
      if (left && right) {
        results.push({ suggestion, leftAlbum: left, rightAlbum: right });
      }
    }

    // Sort by confidence descending
    results.sort((a, b) => b.suggestion.confidence - a.suggestion.confidence);
    return results;
  }, [albums, matchSuggestions]);

  return (
    <div className="p-6 max-md:p-4">
      {/* Header */}
      <div className="mb-6 flex flex-wrap items-center gap-3">
        <Button variant="outline" size="sm" asChild>
          <Link to="/artists/$artistId" params={{ artistId }}>
            <ArrowLeftIcon className="mr-1.5 size-3.5" />
            {artistName}
          </Link>
        </Button>
        <h1 className="text-xl font-bold">Merge Albums</h1>
        <span className="text-sm text-muted-foreground">
          {candidates.length} candidate{candidates.length !== 1 ? "s" : ""}
        </span>
      </div>

      {/* No candidates */}
      {candidates.length === 0 && (
        <div className="rounded-xl border bg-card px-5 py-10 text-center">
          <GitMergeIcon className="mx-auto mb-3 size-8 text-muted-foreground/40" />
          <p className="text-sm text-muted-foreground">
            No merge candidates found. Albums may already be fully linked, or no duplicates were
            detected.
          </p>
        </div>
      )}

      {/* Candidate list */}
      <div className="space-y-4">
        {candidates.map((candidate) => (
          <MergeCandidateCard
            key={candidate.suggestion.id}
            candidate={candidate}
            artistId={artistId}
          />
        ))}
      </div>
    </div>
  );
}

// ── Merge candidate card ───────────────────────────────────────

function MergeCandidateCard({
  candidate,
  artistId,
}: {
  candidate: MergeCandidate;
  artistId: string;
}) {
  const { suggestion, leftAlbum, rightAlbum } = candidate;
  const [selectedTarget, setSelectedTarget] = useState<"left" | "right">("left");

  const mergeAlbums = useMergeAlbums();
  const dismissMatch = useDismissMatchSuggestion();

  const targetAlbum = selectedTarget === "left" ? leftAlbum : rightAlbum;
  const sourceAlbum = selectedTarget === "left" ? rightAlbum : leftAlbum;

  const kind = suggestion.match_kind === "isrc_exact" ? "ISRC" : "Fuzzy";

  const confidenceColor =
    suggestion.confidence >= 80
      ? "bg-green-500/10 text-green-600"
      : suggestion.confidence >= 50
        ? "bg-amber-500/10 text-amber-600"
        : "bg-red-500/10 text-red-600";

  return (
    <div className="overflow-hidden rounded-xl border bg-card">
      {/* Header */}
      <div className="flex flex-wrap items-center justify-between gap-2 border-b px-5 py-3">
        <div className="flex items-center gap-2">
          <Badge variant="secondary">{kind}</Badge>
          <span
            className={`inline-flex rounded-full px-2 py-0.5 text-xs font-medium ${confidenceColor}`}
          >
            {suggestion.confidence}% confidence
          </span>
          {suggestion.explanation && (
            <span className="text-xs text-muted-foreground">{suggestion.explanation}</span>
          )}
        </div>
        <div className="flex items-center gap-1.5">
          <Button
            size="sm"
            disabled={mergeAlbums.isPending}
            onClick={() =>
              mergeAlbums.mutate(
                {
                  body: {
                    target_album_id: targetAlbum.id,
                    source_album_id: sourceAlbum.id,
                    result_title: targetAlbum.title,
                    result_cover_url: targetAlbum.cover_url,
                  },
                },
                {
                  onSuccess: () => {
                    // Also dismiss the suggestion
                    dismissMatch.mutate({
                      params: { path: { suggestion_id: suggestion.id } },
                    });
                  },
                },
              )
            }
          >
            <GitMergeIcon className="mr-1.5 size-3.5" />
            {mergeAlbums.isPending ? "Merging..." : "Merge"}
          </Button>
          <Button
            variant="outline"
            size="sm"
            disabled={dismissMatch.isPending}
            onClick={() =>
              dismissMatch.mutate({
                params: { path: { suggestion_id: suggestion.id } },
              })
            }
          >
            <XIcon className="mr-1.5 size-3.5" />
            Dismiss
          </Button>
        </div>
      </div>

      {/* Body: side-by-side comparison */}
      <div className="grid gap-4 p-5 md:grid-cols-2">
        <AlbumComparisonCard
          album={leftAlbum}
          artistId={artistId}
          isTarget={selectedTarget === "left"}
          onSelect={() => setSelectedTarget("left")}
          label="Album A"
        />
        <AlbumComparisonCard
          album={rightAlbum}
          artistId={artistId}
          isTarget={selectedTarget === "right"}
          onSelect={() => setSelectedTarget("right")}
          label="Album B"
        />
      </div>

      {/* Merge preview */}
      <div className="border-t bg-muted/30 px-5 py-3">
        <p className="text-xs text-muted-foreground">
          Merge will keep <strong>{targetAlbum.title}</strong> and absorb tracks from{" "}
          <strong>{sourceAlbum.title}</strong>.{" "}
          {targetAlbum.cover_url
            ? "Cover art from the target will be used."
            : "No cover art on target."}
        </p>
      </div>
    </div>
  );
}

// ── Album comparison card ──────────────────────────────────────

function AlbumComparisonCard({
  album,
  artistId,
  isTarget,
  onSelect,
  label,
}: {
  album: MonitoredAlbum;
  artistId: string;
  isTarget: boolean;
  onSelect: () => void;
  label: string;
}) {
  return (
    <button
      type="button"
      className={`cursor-pointer rounded-lg border-2 p-3 text-left transition-colors ${
        isTarget
          ? "border-blue-500 bg-blue-500/5"
          : "border-transparent bg-card hover:border-muted-foreground/20"
      }`}
      onClick={onSelect}
    >
      <div className="mb-2 flex items-center gap-2">
        <span className="text-[10px] font-semibold tracking-wider text-muted-foreground uppercase">
          {label}
        </span>
        {isTarget && <Badge className="bg-blue-500/10 text-blue-500">Target</Badge>}
      </div>
      <div className="flex gap-3">
        <div className="size-16 shrink-0 overflow-hidden rounded-lg bg-muted">
          {album.cover_url ? (
            <img src={album.cover_url} alt={album.title} className="size-full object-cover" />
          ) : (
            <div className="flex size-full items-center justify-center text-lg font-bold text-muted-foreground/30">
              {fallbackInitial(album.title)}
            </div>
          )}
        </div>
        <div className="min-w-0 flex-1">
          <Link
            to="/artists/$artistId/albums/$albumId"
            params={{ artistId, albumId: album.id }}
            className="text-sm font-semibold no-underline hover:text-blue-500"
            onClick={(e) => e.stopPropagation()}
          >
            {album.title}
          </Link>
          <p className="text-xs text-muted-foreground">
            {album.release_date?.slice(0, 4) ?? "\u2014"} &middot; {album.album_type ?? "Album"}
          </p>
          <div className="mt-1 flex flex-wrap gap-1">
            {album.acquired && (
              <Badge className="bg-green-500/10 text-green-600" variant="secondary">
                Acquired
              </Badge>
            )}
            {album.wanted && !album.acquired && (
              <Badge className="bg-amber-500/10 text-amber-500" variant="secondary">
                Wanted
              </Badge>
            )}
            {album.monitored && <Badge variant="outline">Monitored</Badge>}
          </div>
        </div>
      </div>
    </button>
  );
}
