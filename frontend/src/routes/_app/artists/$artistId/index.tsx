import { useMemo, useState } from "react";
import { Link, createFileRoute, useNavigate } from "@tanstack/react-router";
import {
  ArrowLeftIcon,
  BookmarkIcon,
  ExternalLinkIcon,
  GitMergeIcon,
  PencilIcon,
  PlusIcon,
  RefreshCwIcon,
  Trash2Icon,
  XIcon,
} from "lucide-react";

import type { components } from "@/lib/api/types.gen";
import { $api } from "@/lib/api";
import { useSleeveGlow } from "@/hooks/use-sleeve-glow";
import { useLocalStorage } from "@/hooks/use-local-storage";
import { isAlbumAcquired, isAlbumInProgress, isAlbumWanted, isAlbumWantedLike } from "@/lib/music";
import {
  useAcceptMatchSuggestion,
  useDeleteArtist,
  useDismissMatchSuggestion,
  useFetchArtistBio,
  useRefreshArtistMatchSuggestions,
  useSetAlbumQuality,
  useSyncArtist,
  useToggleAlbumMonitor,
  useToggleArtistMonitor,
  useUnlinkArtistProvider,
  useUpdateArtist,
} from "@/lib/api/mutations";

import { MonitorButton } from "@/components/monitor-button";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { Switch } from "@/components/ui/switch";
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
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";

type MonitoredArtist = components["schemas"]["MonitoredArtist"];
type Album = components["schemas"]["Album"] & {
  quality_override?: components["schemas"]["Quality"] | null;
};
type ProviderLink = components["schemas"]["ProviderLink"];
type ArtistMatchSuggestion = components["schemas"]["ArtistMatchSuggestion"];
type Quality = components["schemas"]["Quality"];

export const Route = createFileRoute("/_app/artists/$artistId/")({
  component: ArtistDetailPage,
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

const ALBUM_TYPE_ORDER = [
  "album",
  "ep",
  "single",
  "compilation",
  "live",
  "remix",
  "soundtrack",
  "other",
] as const;

const ALBUM_TYPE_GROUP_LABELS: Record<string, string> = {
  album: "Albums",
  ep: "EPs",
  single: "Singles",
  compilation: "Compilations",
  live: "Live",
  remix: "Remixes",
  soundtrack: "Soundtracks",
  other: "Other",
};

function albumTypeKey(albumType: string | null | undefined): string {
  const key = (albumType ?? "album").toLowerCase();
  return ALBUM_TYPE_ORDER.includes(key as (typeof ALBUM_TYPE_ORDER)[number]) ? key : "other";
}

function albumTypeRank(albumType: string | null | undefined): number {
  const idx = ALBUM_TYPE_ORDER.indexOf(
    albumTypeKey(albumType) as (typeof ALBUM_TYPE_ORDER)[number],
  );
  return idx === -1 ? 999 : idx;
}

function fallbackInitial(name: string): string {
  return name.charAt(0).toUpperCase() || "?";
}

// ── Page component ─────────────────────────────────────────────

function ArtistDetailPage() {
  const { artistId } = Route.useParams();
  const { data, isLoading, isError } = $api.useQuery("get", "/api/artist/{artist_id}", {
    params: { path: { artist_id: artistId } },
  });

  if (isLoading) {
    return <ArtistDetailSkeleton />;
  }

  if (isError) {
    return (
      <div className="p-6">
        <div className="rounded-xl border border-red-200 bg-red-50 p-4 text-sm text-red-700 dark:border-red-900/50 dark:bg-red-950/50 dark:text-red-400">
          Failed to load artist details.
        </div>
      </div>
    );
  }

  if (!data) {
    return (
      <div className="p-6">
        <p className="text-muted-foreground">Artist not found.</p>
        <Button asChild className="mt-4" size="lg">
          <Link to="/library/artists">
            <ArrowLeftIcon className="mr-2 size-4" />
            Library
          </Link>
        </Button>
      </div>
    );
  }

  return (
    <ArtistDetailContent
      artist={data.artist}
      albums={data.albums}
      providerLinks={data.provider_links}
      artistMatchSuggestions={data.artist_match_suggestions}
      defaultQuality={data.default_quality}
    />
  );
}

// ── Skeleton ───────────────────────────────────────────────────

function ArtistDetailSkeleton() {
  return (
    <div className="p-6">
      <div className="mb-5 rounded-xl border bg-card p-5">
        <div className="flex animate-pulse flex-wrap items-center gap-5">
          <Skeleton className="size-20 shrink-0 rounded-full" />
          <div className="min-w-0 flex-1">
            <Skeleton className="mb-3 h-6 w-40" />
            <Skeleton className="mb-3 h-3.5 w-64" />
            <div className="flex flex-wrap gap-1.5">
              {Array.from({ length: 4 }).map((_, i) => (
                <Skeleton key={i} className="h-7 w-20 rounded-lg" />
              ))}
            </div>
          </div>
        </div>
      </div>
      <div className="overflow-hidden rounded-xl border bg-card">
        <div className="border-b px-5 py-3">
          <Skeleton className="h-4 w-24" />
        </div>
        <div className="p-4">
          <div className="grid grid-cols-[repeat(auto-fill,minmax(180px,1fr))] gap-5">
            {Array.from({ length: 6 }).map((_, i) => (
              <div key={i} className="animate-pulse overflow-hidden rounded-xl border">
                <Skeleton className="aspect-square w-full" />
                <div className="p-3">
                  <Skeleton className="mb-2 h-3.5 w-24" />
                  <Skeleton className="h-3 w-16" />
                </div>
              </div>
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}

// ── Main content ───────────────────────────────────────────────

function ArtistDetailContent({
  artist,
  albums,
  providerLinks,
  artistMatchSuggestions,
  defaultQuality,
}: {
  artist: MonitoredArtist;
  albums: Array<Album>;
  providerLinks: Array<ProviderLink>;
  artistMatchSuggestions: Array<ArtistMatchSuggestion>;
  defaultQuality: Quality;
}) {
  const navigate = useNavigate();
  const [albumSort, setAlbumSort] = useLocalStorage<"az" | "newest" | "oldest">(
    "artist-detail-albums-sort",
    "newest",
  );
  const [showRemoveDialog, setShowRemoveDialog] = useState(false);
  const [showEditDialog, setShowEditDialog] = useState(false);

  const deleteArtist = useDeleteArtist();
  const syncArtist = useSyncArtist();
  const toggleMonitor = useToggleArtistMonitor();

  const albumCount = albums.length;
  const monitoredCount = albums.filter((a) => a.monitored).length;
  const acquiredCount = albums.filter((a) => isAlbumAcquired(a.wanted_status)).length;
  const wantedCount = albums.filter((a) => isAlbumWantedLike(a.wanted_status)).length;

  const pendingSuggestions = artistMatchSuggestions.filter((m) => m.status === "pending");

  /** Sort a list of albums by the current sort mode. */
  const sortList = (list: Album[]): Album[] => {
    const sorted = [...list];
    switch (albumSort) {
      case "az":
        sorted.sort((a, b) => a.title.toLowerCase().localeCompare(b.title.toLowerCase()));
        break;
      case "newest":
        sorted.sort(
          (a, b) =>
            (b.release_date ?? "").localeCompare(a.release_date ?? "") ||
            a.title.localeCompare(b.title),
        );
        break;
      case "oldest":
        sorted.sort(
          (a, b) =>
            (a.release_date ?? "").localeCompare(b.release_date ?? "") ||
            a.title.localeCompare(b.title),
        );
        break;
    }
    return sorted;
  };

  /** Albums grouped by type, each group sorted by the active sort. */
  const albumGroups = useMemo(() => {
    const buckets = new Map<string, Album[]>();
    for (const album of albums) {
      const key = albumTypeKey(album.album_type);
      const bucket = buckets.get(key);
      if (bucket) {
        bucket.push(album);
      } else {
        buckets.set(key, [album]);
      }
    }
    return [...buckets.entries()]
      .sort(([a], [b]) => albumTypeRank(a) - albumTypeRank(b))
      .map(([type, items]) => ({
        type,
        label: ALBUM_TYPE_GROUP_LABELS[type] ?? type,
        albums: sortList(items),
      }));
  }, [albums, albumSort]);

  return (
    <div className="p-6 max-md:p-4">
      {/* ── Artist header card ─────────────────────────────── */}
      <div className="mb-5 rounded-xl border bg-card p-5">
        <div className="flex flex-wrap items-center gap-5">
          {artist.image_url ? (
            <img
              className="size-20 shrink-0 rounded-full border-2 border-blue-500/20 bg-muted object-cover dark:border-blue-500/30"
              src={artist.image_url}
              alt=""
            />
          ) : (
            <div className="inline-flex size-20 shrink-0 items-center justify-center rounded-full border-2 border-blue-500/20 bg-muted text-[32px] font-bold text-muted-foreground dark:border-blue-500/30">
              {fallbackInitial(artist.name)}
            </div>
          )}

          <div className="min-w-0 flex-1">
            <div className="mb-1 text-[22px] font-bold">{artist.name}</div>
            <div className="mb-2 flex flex-wrap items-center gap-2 text-[13px] text-muted-foreground">
              <span>
                {albumCount} albums &middot; {monitoredCount} monitored &middot; {acquiredCount}{" "}
                acquired &middot; {wantedCount} wanted
              </span>
              {artist.monitored ? (
                <Badge variant="outline" className="border-blue-500/30 text-blue-500">
                  Monitored
                </Badge>
              ) : (
                <Badge variant="outline" className="border-amber-500/30 text-amber-500">
                  Lightweight
                </Badge>
              )}
            </div>

            {!artist.monitored && (
              <div className="mb-2 text-[12px] text-amber-700 dark:text-amber-300">
                This artist is lightweight. Promote to monitored to sync full discography
                automatically.
              </div>
            )}

            {/* Provider link chips */}
            <ProviderChips artistId={artist.id} providerLinks={providerLinks} />

            {/* Action buttons */}
            <div className="flex flex-wrap gap-1.5">
              <Button variant="outline" size="sm" onClick={() => setShowEditDialog(true)}>
                <PencilIcon className="mr-1.5 size-3.5" />
                Edit
              </Button>
              {artist.monitored ? (
                <Button
                  variant="outline"
                  size="sm"
                  disabled={syncArtist.isPending}
                  onClick={() =>
                    syncArtist.mutate({
                      params: { path: { artist_id: artist.id } },
                    })
                  }
                >
                  <RefreshCwIcon
                    className={`mr-1.5 size-3.5 ${syncArtist.isPending ? "animate-spin" : ""}`}
                  />
                  {syncArtist.isPending ? "Syncing..." : "Sync Albums"}
                </Button>
              ) : (
                <Button
                  size="sm"
                  disabled={toggleMonitor.isPending}
                  onClick={() =>
                    toggleMonitor.mutate({
                      params: { path: { artist_id: artist.id } },
                      body: { monitored: true },
                    })
                  }
                >
                  <BookmarkIcon className="mr-1.5 size-3.5" />
                  {toggleMonitor.isPending ? "Promoting..." : "Monitor Artist"}
                </Button>
              )}
              <Button
                variant="destructive"
                size="sm"
                className="ml-auto"
                onClick={() => setShowRemoveDialog(true)}
              >
                <Trash2Icon className="mr-1.5 size-3.5" />
                Remove Artist
              </Button>
            </div>
          </div>
        </div>

        {/* Bio section */}
        {artist.bio && (
          <div className="mt-4 border-t pt-4">
            <ArtistBio bio={artist.bio} />
          </div>
        )}
      </div>

      {/* ── Match suggestions ──────────────────────────────── */}
      {pendingSuggestions.length > 0 && (
        <MatchSuggestionsPanel artistId={artist.id} suggestions={pendingSuggestions} />
      )}

      {/* ── Discography ────────────────────────────────────── */}
      <div className="overflow-hidden rounded-xl border bg-card">
        <div className="flex flex-wrap items-center justify-between gap-3 border-b px-5 py-3">
          <div className="flex items-center gap-3">
            <h2 className="text-sm font-semibold">Discography</h2>
            <span className="text-xs text-muted-foreground">{albumCount} albums</span>
          </div>
          <div className="flex flex-wrap items-center gap-2">
            <Button variant="outline" size="sm" asChild>
              <Link to="/artists/$artistId/merge-albums" params={{ artistId: artist.id }}>
                <GitMergeIcon className="mr-1.5 size-3.5" />
                Merge Albums
              </Link>
            </Button>
            {albumCount > 0 && (
              <>
                <div className="mx-1 h-4 w-px bg-border" />
                <Select value={albumSort} onValueChange={setAlbumSort}>
                  <SelectTrigger className="h-8 w-32.5 text-xs">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="az">A - Z</SelectItem>
                    <SelectItem value="newest">Newest First</SelectItem>
                    <SelectItem value="oldest">Oldest First</SelectItem>
                  </SelectContent>
                </Select>
              </>
            )}
          </div>
        </div>

        {albumGroups.length === 0 ? (
          <div className="px-5 py-8 text-center text-sm text-muted-foreground">
            No albums synced. Hit Sync Albums to fetch from provider.
          </div>
        ) : (
          <div className="space-y-6 p-4">
            {albumGroups.map((group) => (
              <AlbumGroupSection
                key={group.type}
                label={group.label}
                albums={group.albums}
                artistId={artist.id}
                defaultQuality={defaultQuality}
                albumSort={albumSort}
              />
            ))}
          </div>
        )}
      </div>

      {/* ── Dialogs ────────────────────────────────────────── */}
      <RemoveArtistDialog
        open={showRemoveDialog}
        onOpenChange={setShowRemoveDialog}
        artistName={artist.name}
        onConfirm={(removeFiles) => {
          deleteArtist.mutate(
            {
              params: {
                path: { artist_id: artist.id },
                query: { remove_files: removeFiles },
              },
            },
            {
              onSuccess: () => {
                void navigate({ to: "/library/artists" });
              },
            },
          );
        }}
        isPending={deleteArtist.isPending}
      />

      <EditArtistDialog open={showEditDialog} onOpenChange={setShowEditDialog} artist={artist} />
    </div>
  );
}

// ── Provider chips ─────────────────────────────────────────────

function ProviderChips({
  artistId,
  providerLinks,
}: {
  artistId: string;
  providerLinks: Array<ProviderLink>;
}) {
  const unlinkProvider = useUnlinkArtistProvider();

  return (
    <div className="mb-2.5 flex flex-wrap items-center gap-1.5">
      {providerLinks.map((link) => (
        <div
          key={`${link.provider}-${link.external_id}`}
          className="group inline-flex items-center gap-1.5 rounded-lg border bg-card py-1 pr-1 pl-2 text-xs transition-colors hover:border-foreground/20"
        >
          <span className="font-medium text-muted-foreground">
            {providerDisplayName(link.provider)}
          </span>
          {link.external_url && (
            <a
              href={link.external_url}
              target="_blank"
              rel="noreferrer"
              className="text-muted-foreground hover:text-blue-500"
            >
              <ExternalLinkIcon className="size-3" />
            </a>
          )}
          <button
            type="button"
            className="cursor-pointer border-none bg-transparent p-0.5 text-muted-foreground opacity-0 transition-opacity group-hover:opacity-100 hover:text-red-500"
            title="Unlink this provider"
            onClick={() =>
              unlinkProvider.mutate({
                params: {
                  path: { artist_id: artistId },
                },
                body: {
                  provider: link.provider,
                  external_id: link.external_id,
                },
              })
            }
          >
            <XIcon className="size-3" />
          </button>
        </div>
      ))}
      <button
        type="button"
        className="inline-flex cursor-pointer items-center gap-1 rounded-lg border border-dashed bg-transparent px-2 py-1 text-[11px] font-medium text-muted-foreground transition-colors hover:border-blue-500/30 hover:text-blue-500"
      >
        <PlusIcon className="size-3" />
        Link
      </button>
    </div>
  );
}

// ── Match suggestions panel ────────────────────────────────────

function MatchSuggestionsPanel({
  artistId,
  suggestions,
}: {
  artistId: string;
  suggestions: Array<ArtistMatchSuggestion>;
}) {
  const acceptMatch = useAcceptMatchSuggestion();
  const dismissMatch = useDismissMatchSuggestion();
  const refreshMatches = useRefreshArtistMatchSuggestions();

  return (
    <div className="mb-5 overflow-hidden rounded-xl border bg-card">
      <div className="flex items-center justify-between border-b px-5 py-3">
        <h2 className="text-sm font-semibold">Potential Matches ({suggestions.length})</h2>
        <Button
          variant="outline"
          size="sm"
          disabled={refreshMatches.isPending}
          onClick={() =>
            refreshMatches.mutate({
              params: { path: { artist_id: artistId } },
            })
          }
        >
          <RefreshCwIcon
            className={`mr-1.5 size-3.5 ${refreshMatches.isPending ? "animate-spin" : ""}`}
          />
          Refresh
        </Button>
      </div>
      <div className="flex flex-col gap-2 p-4">
        {suggestions.map((m) => {
          const name = m.external_name ?? "Unknown artist match";
          const right = providerDisplayName(m.right_provider);
          const kind = m.match_kind === "isrc_exact" ? "ISRC" : "Fuzzy";

          const subtitle = [
            (m.disambiguation ??
              [m.artist_type, m.country && `from ${m.country}`].filter(Boolean).join(" ")) ||
              null,
            m.popularity != null ? `${String(m.popularity)}% popularity` : null,
          ]
            .filter(Boolean)
            .join(" \u00b7 ");

          const confidenceColor =
            m.confidence >= 80
              ? "bg-green-500/10 text-green-600"
              : m.confidence >= 50
                ? "bg-amber-500/10 text-amber-600"
                : "bg-red-500/10 text-red-600";

          return (
            <div key={m.id} className="flex items-start gap-3 rounded-lg border p-2">
              {m.image_url ? (
                <img
                  className="size-9 shrink-0 rounded-full border border-blue-500/20 bg-muted object-cover"
                  src={m.image_url}
                  alt=""
                />
              ) : (
                <div className="inline-flex size-9 shrink-0 items-center justify-center rounded-full border border-blue-500/20 bg-muted text-sm font-bold text-muted-foreground">
                  {fallbackInitial(name)}
                </div>
              )}
              <div className="min-w-0 flex-1">
                <div className="flex flex-wrap items-center gap-2">
                  <span className="text-[15px] font-semibold">{name}</span>
                  {m.external_url ? (
                    <a href={m.external_url} target="_blank" rel="noreferrer">
                      <Badge variant="outline">{right}</Badge>
                    </a>
                  ) : (
                    <Badge variant="outline">{right}</Badge>
                  )}
                  <Badge variant="secondary">{kind}</Badge>
                  <span
                    className={`inline-flex items-center rounded-full px-2 py-0.5 text-xs font-medium ${confidenceColor}`}
                  >
                    {m.confidence}%
                  </span>
                </div>
                {subtitle && (
                  <div className="mt-0.5 text-[12px] leading-snug text-muted-foreground">
                    {subtitle}
                  </div>
                )}
                {m.tags.length > 0 && (
                  <div className="mt-1 flex flex-wrap gap-1">
                    {m.tags.map((tag) => (
                      <Badge key={tag} variant="outline">
                        {tag}
                      </Badge>
                    ))}
                  </div>
                )}
                {m.explanation && (
                  <div className="mt-1 text-[11px] text-muted-foreground">{m.explanation}</div>
                )}
                <div className="mt-0.5 text-[10px] text-muted-foreground/70">
                  ID: {m.right_external_id}
                </div>
              </div>
              <div className="flex shrink-0 flex-col items-center gap-1.5 lg:flex-row">
                <Button
                  size="sm"
                  disabled={acceptMatch.isPending}
                  onClick={() =>
                    acceptMatch.mutate({
                      params: { path: { suggestion_id: m.id } },
                    })
                  }
                >
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

// ── Album group section ────────────────────────────────────────

function AlbumGroupSection({
  label,
  albums,
  artistId,
  defaultQuality,
  albumSort,
}: {
  label: string;
  albums: Array<Album>;
  artistId: string;
  defaultQuality: Quality;
  albumSort: string;
}) {
  const gridRef = useSleeveGlow([albums.length, albumSort]);

  return (
    <section>
      <div className="mb-3 flex items-center gap-2">
        <h3 className="text-sm font-semibold">{label}</h3>
        <span className="text-xs text-muted-foreground">{albums.length}</span>
      </div>
      <div
        ref={gridRef}
        className="grid grid-cols-[repeat(auto-fill,minmax(180px,1fr))] gap-5 max-md:grid-cols-[repeat(auto-fill,minmax(140px,1fr))] max-md:gap-3"
      >
        {albums.map((album) => (
          <AlbumCard
            key={album.id}
            album={album}
            artistId={artistId}
            defaultQuality={defaultQuality}
          />
        ))}
      </div>
    </section>
  );
}

// ── Album card ─────────────────────────────────────────────────

function AlbumCard({
  album,
  artistId,
  defaultQuality,
}: {
  album: Album;
  artistId: string;
  defaultQuality: Quality;
}) {
  const toggleAlbumMonitor = useToggleAlbumMonitor();
  const setAlbumQuality = useSetAlbumQuality();
  const releaseDate = album.release_date ?? "\u2014";
  const at = albumTypeLabel(album.album_type);

  const statusBadge = isAlbumAcquired(album.wanted_status)
    ? { label: "Acquired", className: "bg-green-500/10 text-green-600" }
    : isAlbumWanted(album.wanted_status)
      ? { label: "Wanted", className: "bg-amber-500/10 text-amber-500" }
      : isAlbumInProgress(album.wanted_status)
        ? { label: "In Progress", className: "bg-blue-500/10 text-blue-500" }
        : null;

  return (
    <div className="sleeve group relative">
      <Link
        to="/artists/$artistId/albums/$albumId"
        params={{ artistId, albumId: album.id }}
        className="block"
      >
        <div className="relative aspect-square bg-muted">
          {album.cover_url ? (
            <img
              src={album.cover_url}
              alt={album.title}
              className="sleeve-cover"
              crossOrigin="anonymous"
            />
          ) : (
            <div className="flex size-full items-center justify-center text-3xl font-bold text-muted-foreground/30">
              {fallbackInitial(album.title)}
            </div>
          )}
          {statusBadge && (
            <span
              className={`absolute top-2 right-2 z-10 rounded-full px-2 py-0.5 text-[10px] font-medium ${statusBadge.className}`}
            >
              {statusBadge.label}
            </span>
          )}
          {album.explicit && (
            <span className="absolute bottom-2 left-2 z-10 rounded bg-zinc-900/70 px-1 py-0.5 text-[9px] font-bold tracking-wide text-white">
              E
            </span>
          )}
        </div>
        <div className="p-3">
          <p className="truncate text-sm font-semibold">{album.title}</p>
          <p className="truncate text-xs text-muted-foreground">
            {releaseDate.slice(0, 4)} &middot; {at}
          </p>
        </div>
      </Link>
      <div className="absolute right-2 bottom-2">
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
          variant="compact"
        />
      </div>
    </div>
  );
}

// ── Artist bio (collapsible) ───────────────────────────────────

function ArtistBio({ bio }: { bio: string }) {
  const [expanded, setExpanded] = useState(false);
  const isLong = bio.length > 280;

  return (
    <div className="text-[13px] leading-relaxed text-muted-foreground">
      <p className={!expanded && isLong ? "line-clamp-3" : ""}>{bio}</p>
      {isLong && (
        <button
          type="button"
          className="mt-1 cursor-pointer border-none bg-transparent p-0 text-[12px] font-medium text-blue-500 hover:text-blue-600 dark:text-blue-400 dark:hover:text-blue-300"
          onClick={() => setExpanded(!expanded)}
        >
          {expanded ? "Show less" : "Read more"}
        </button>
      )}
    </div>
  );
}

// ── Remove artist dialog ───────────────────────────────────────

function RemoveArtistDialog({
  open,
  onOpenChange,
  artistName,
  onConfirm,
  isPending,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  artistName: string;
  onConfirm: (removeFiles: boolean) => void;
  isPending: boolean;
}) {
  const [removeFiles, setRemoveFiles] = useState(false);

  return (
    <AlertDialog open={open} onOpenChange={onOpenChange}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>Remove Artist</AlertDialogTitle>
          <AlertDialogDescription>
            This will remove <strong>{artistName}</strong> and all associated data. This cannot be
            undone.
          </AlertDialogDescription>
        </AlertDialogHeader>
        <div className="flex items-center gap-2">
          <Switch id="remove-files" checked={removeFiles} onCheckedChange={setRemoveFiles} />
          <Label htmlFor="remove-files" className="text-sm">
            Also remove downloaded files from disk
          </Label>
        </div>
        <AlertDialogFooter>
          <AlertDialogCancel>Cancel</AlertDialogCancel>
          <AlertDialogAction
            variant="destructive"
            disabled={isPending}
            onClick={() => onConfirm(removeFiles)}
          >
            {isPending ? "Removing..." : "Remove"}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}

// ── Edit artist dialog ─────────────────────────────────────────

function EditArtistDialog({
  open,
  onOpenChange,
  artist,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  artist: MonitoredArtist;
}) {
  const [name, setName] = useState(artist.name);
  const [imageUrl, setImageUrl] = useState(artist.image_url ?? "");
  const updateArtist = useUpdateArtist();
  const fetchBio = useFetchArtistBio();

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent>
        <DialogHeader>
          <DialogTitle>Edit Artist</DialogTitle>
          <DialogDescription>Update the artist name and image.</DialogDescription>
        </DialogHeader>
        <div className="grid gap-4">
          <div className="grid gap-1.5">
            <Label htmlFor="artist-name">Name</Label>
            <Input id="artist-name" value={name} onChange={(e) => setName(e.target.value)} />
          </div>
          <div className="grid gap-1.5">
            <Label htmlFor="artist-image">Image URL</Label>
            <Input
              id="artist-image"
              value={imageUrl}
              onChange={(e) => setImageUrl(e.target.value)}
              placeholder="https://..."
            />
          </div>
        </div>
        <DialogFooter>
          <Button
            variant="outline"
            disabled={fetchBio.isPending}
            onClick={() =>
              fetchBio.mutate({
                params: { path: { artist_id: artist.id } },
              })
            }
          >
            {fetchBio.isPending ? "Fetching..." : "Fetch Bio"}
          </Button>
          <Button
            disabled={updateArtist.isPending}
            onClick={() => {
              updateArtist.mutate(
                {
                  params: { path: { artist_id: artist.id } },
                  body: {
                    name,
                    image_url: imageUrl || null,
                  },
                },
                {
                  onSuccess: () => onOpenChange(false),
                },
              );
            }}
          >
            {updateArtist.isPending ? "Saving..." : "Save"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
