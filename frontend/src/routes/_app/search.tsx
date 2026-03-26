import { useState } from "react";
import { createFileRoute } from "@tanstack/react-router";
import { useQueryClient } from "@tanstack/react-query";
import { useLiveQuery } from "@tanstack/react-db";
import {
  CheckCircle2Icon,
  ChevronDownIcon,
  DiscAlbumIcon,
  Loader2Icon,
  MicIcon,
  MusicIcon,
  PlusIcon,
  SearchIcon,
} from "lucide-react";

import { $api, getCollections, addedItemKey } from "@/lib/api";
import { useCreateArtist, useCreateAlbum, useCreateTrack } from "@/lib/api/mutations";
import { useLocalStorage } from "@/hooks/use-local-storage";
import { formatDurationSeconds, normalizeProvider, providerDisplayName } from "@/lib/music";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from "@/components/ui/collapsible";
import type { components } from "@/lib/api/types.gen";

type SearchArtistResult = components["schemas"]["SearchArtistResult"];
type SearchAlbumResult = components["schemas"]["SearchAlbumResult"];
type SearchTrackResult = components["schemas"]["SearchTrackResult"];

export const Route = createFileRoute("/_app/search")({
  component: SearchPage,
  staticData: {
    breadcrumb: "Search",
  },
});

// ── Hook: session-local "added" set from TanStack DB ───────────

/**
 * Returns a reactive `Set<string>` of composite keys (`"provider:external_id"`)
 * for items the user has added during this session.  Because it reads from the
 * `addedItemsCollection` via `useLiveQuery`, the set updates immediately when a
 * new item is inserted — no re-fetch needed.
 */
function useAddedItemKeys(): Set<string> {
  const queryClient = useQueryClient();
  const { addedItemsCollection } = getCollections(queryClient);

  const { data } = useLiveQuery(addedItemsCollection);

  const keys = new Set<string>();
  if (data) {
    for (const item of data) {
      keys.add(item.key);
    }
  }
  return keys;
}

// ── Page ───────────────────────────────────────────────────────

function SearchPage() {
  const [query, setQuery] = useState("");
  const [submitted, setSubmitted] = useState("");
  const [artistsOpen, setArtistsOpen] = useLocalStorage<"false" | "true">(
    "search-artists-open",
    "true",
  );
  const [albumsOpen, setAlbumsOpen] = useLocalStorage<"false" | "true">(
    "search-albums-open",
    "true",
  );
  const [tracksOpen, setTracksOpen] = useLocalStorage<"false" | "true">(
    "search-tracks-open",
    "true",
  );

  const { data, isLoading, isError } = $api.useQuery(
    "get",
    "/api/search",
    { params: { query: { query: submitted } } },
    { enabled: submitted.length > 0 },
  );

  const addedKeys = useAddedItemKeys();

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    const trimmed = query.trim();
    if (trimmed.length > 0) {
      setSubmitted(trimmed);
    }
  };

  const hasResults =
    data && (data.artists.length > 0 || data.albums.length > 0 || data.tracks.length > 0);

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-bold tracking-tight">Search</h1>
        <p className="text-muted-foreground">
          Find artists, albums, and tracks from external providers.
        </p>
      </div>

      <form onSubmit={handleSubmit} className="relative max-w-xl">
        <SearchIcon className="absolute top-1/2 left-3 size-4 -translate-y-1/2 text-muted-foreground" />
        <input
          type="text"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder="Search for artists, albums, tracks..."
          className="w-full rounded-lg border bg-background py-2.5 pr-4 pl-10 text-sm ring-ring transition-shadow outline-none placeholder:text-muted-foreground focus:ring-2"
        />
      </form>

      {isLoading && (
        <div className="flex items-center justify-center py-16">
          <Loader2Icon className="size-6 animate-spin text-muted-foreground" />
        </div>
      )}

      {isError && submitted && (
        <div className="rounded-xl border border-red-200 bg-red-50 p-4 text-sm text-red-700 dark:border-red-900/50 dark:bg-red-950/50 dark:text-red-400">
          Search failed. Please try again.
        </div>
      )}

      {!isLoading && !isError && submitted && !hasResults && (
        <div className="flex flex-col items-center justify-center rounded-xl border border-dashed bg-muted/30 py-20">
          <SearchIcon className="size-10 text-muted-foreground/40" />
          <p className="mt-4 text-sm text-muted-foreground">
            No results found for &ldquo;{submitted}&rdquo;.
          </p>
        </div>
      )}

      {!isLoading && !submitted && (
        <div className="flex flex-col items-center justify-center rounded-xl border border-dashed bg-muted/30 py-20">
          <SearchIcon className="size-10 text-muted-foreground/40" />
          <p className="mt-4 text-sm text-muted-foreground">
            Type a query to search across your configured providers.
          </p>
        </div>
      )}

      {hasResults && data && (
        <div className="space-y-8">
          {/* Artists */}
          {data.artists.length > 0 && (
            <Collapsible
              open={artistsOpen === "true"}
              onOpenChange={(open) => setArtistsOpen(open ? "true" : "false")}
            >
              <section className="space-y-3">
                <CollapsibleTrigger className="group flex w-full items-center gap-2 text-sm font-semibold tracking-wider text-muted-foreground uppercase hover:text-foreground">
                  <MicIcon className="size-4" />
                  Artists ({data.artists.length})
                  <ChevronDownIcon className="ml-auto size-4 transition-transform group-data-[state=open]:rotate-180" />
                </CollapsibleTrigger>
                <CollapsibleContent>
                  <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
                    {data.artists.map((artist) => (
                      <ArtistResultCard
                        key={`${artist.provider}-${artist.external_id}`}
                        artist={artist}
                        addedKeys={addedKeys}
                      />
                    ))}
                  </div>
                </CollapsibleContent>
              </section>
            </Collapsible>
          )}

          {/* Albums */}
          {data.albums.length > 0 && (
            <Collapsible
              open={albumsOpen === "true"}
              onOpenChange={(open) => setAlbumsOpen(open ? "true" : "false")}
            >
              <section className="space-y-3">
                <CollapsibleTrigger className="group flex w-full items-center gap-2 text-sm font-semibold tracking-wider text-muted-foreground uppercase hover:text-foreground">
                  <DiscAlbumIcon className="size-4" />
                  Albums ({data.albums.length})
                  <ChevronDownIcon className="ml-auto size-4 transition-transform group-data-[state=open]:rotate-180" />
                </CollapsibleTrigger>
                <CollapsibleContent>
                  <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
                    {data.albums.map((album) => (
                      <AlbumResultCard
                        key={`${album.provider}-${album.external_id}`}
                        album={album}
                        addedKeys={addedKeys}
                      />
                    ))}
                  </div>
                </CollapsibleContent>
              </section>
            </Collapsible>
          )}

          {/* Tracks */}
          {data.tracks.length > 0 && (
            <Collapsible
              open={tracksOpen === "true"}
              onOpenChange={(open) => setTracksOpen(open ? "true" : "false")}
            >
              <section className="space-y-3">
                <CollapsibleTrigger className="group flex w-full items-center gap-2 text-sm font-semibold tracking-wider text-muted-foreground uppercase hover:text-foreground">
                  <MusicIcon className="size-4" />
                  Tracks ({data.tracks.length})
                  <ChevronDownIcon className="ml-auto size-4 transition-transform group-data-[state=open]:rotate-180" />
                </CollapsibleTrigger>
                <CollapsibleContent>
                  <div className="space-y-1">
                    {data.tracks.map((track) => (
                      <TrackResultRow
                        key={`${track.provider}-${track.external_id}`}
                        track={track}
                        addedKeys={addedKeys}
                      />
                    ))}
                  </div>
                </CollapsibleContent>
              </section>
            </Collapsible>
          )}
        </div>
      )}
    </div>
  );
}

// ── Shared "Added" badge ───────────────────────────────────────

function AddedBadge() {
  return (
    <Badge variant="secondary" className="shrink-0">
      <CheckCircle2Icon className="mr-1 size-3" />
      Added
    </Badge>
  );
}

// ── Artist search result card ──────────────────────────────────

function ArtistResultCard({
  artist,
  addedKeys,
}: {
  artist: SearchArtistResult;
  addedKeys: Set<string>;
}) {
  const createArtist = useCreateArtist();
  const provider = normalizeProvider(artist.provider);

  const isAdded =
    artist.already_monitored || addedKeys.has(addedItemKey(artist.provider, artist.external_id));

  return (
    <div className="flex items-center gap-3 rounded-xl border bg-card p-3 shadow-sm">
      <div className="size-12 shrink-0 overflow-hidden rounded-full bg-muted">
        {artist.image_url ? (
          <img src={artist.image_url} alt={artist.name} className="size-full object-cover" />
        ) : (
          <div className="flex size-full items-center justify-center text-lg font-bold text-muted-foreground/30">
            {artist.name.charAt(0)}
          </div>
        )}
      </div>
      <div className="min-w-0 flex-1">
        <p className="truncate font-semibold">{artist.name}</p>
        <div className="flex flex-wrap items-center gap-1 text-xs text-muted-foreground">
          <Badge variant="outline" className="text-[10px]">
            {providerDisplayName(artist.provider)}
          </Badge>
          {artist.disambiguation && <span className="truncate">{artist.disambiguation}</span>}
        </div>
      </div>
      {isAdded ? (
        <AddedBadge />
      ) : (
        <Button
          size="sm"
          variant="outline"
          className="shrink-0"
          disabled={createArtist.isPending || provider == null}
          onClick={() =>
            provider &&
            createArtist.mutate({
              body: {
                name: artist.name,
                external_id: artist.external_id,
                provider,
                image_url: artist.image_url ?? null,
                external_url: artist.url ?? null,
              },
            })
          }
        >
          <PlusIcon className="mr-1 size-3.5" />
          {provider == null ? "Unsupported" : createArtist.isPending ? "Adding..." : "Add"}
        </Button>
      )}
    </div>
  );
}

// ── Album search result card ───────────────────────────────────

function AlbumResultCard({
  album,
  addedKeys,
}: {
  album: SearchAlbumResult;
  addedKeys: Set<string>;
}) {
  const createAlbum = useCreateAlbum();
  const provider = normalizeProvider(album.provider);

  const isAdded =
    album.already_added || addedKeys.has(addedItemKey(album.provider, album.external_id));

  return (
    <div className="flex items-center gap-3 rounded-xl border bg-card p-3 shadow-sm">
      <div className="size-12 shrink-0 overflow-hidden rounded-lg bg-muted">
        {album.cover_url ? (
          <img src={album.cover_url} alt={album.title} className="size-full object-cover" />
        ) : (
          <div className="flex size-full items-center justify-center text-lg font-bold text-muted-foreground/30">
            {album.title.charAt(0)}
          </div>
        )}
      </div>
      <div className="min-w-0 flex-1">
        <p className="truncate font-semibold">{album.title}</p>
        <div className="flex flex-wrap items-center gap-1 text-xs text-muted-foreground">
          <span className="truncate">{album.artist_name}</span>
          <span>&middot;</span>
          <span>{album.release_date?.slice(0, 4)}</span>
          <Badge variant="outline" className="text-[10px]">
            {providerDisplayName(album.provider)}
          </Badge>
        </div>
      </div>
      {isAdded ? (
        <AddedBadge />
      ) : (
        <Button
          size="sm"
          variant="outline"
          className="shrink-0"
          disabled={createAlbum.isPending || provider == null}
          onClick={() =>
            provider &&
            createAlbum.mutate({
              body: {
                external_album_id: album.external_id,
                artist_external_id: album.artist_external_id,
                artist_name: album.artist_name,
                provider,
                monitor_all: true,
              },
            })
          }
        >
          <PlusIcon className="mr-1 size-3.5" />
          {provider == null ? "Unsupported" : createAlbum.isPending ? "Adding..." : "Add"}
        </Button>
      )}
    </div>
  );
}

// ── Track search result row ────────────────────────────────────

function TrackResultRow({
  track,
  addedKeys,
}: {
  track: SearchTrackResult;
  addedKeys: Set<string>;
}) {
  const createTrack = useCreateTrack();
  const provider = normalizeProvider(track.provider);

  const isAdded =
    track.already_added || addedKeys.has(addedItemKey(track.provider, track.external_id));

  return (
    <div className="flex items-center gap-3 rounded-lg border bg-card px-4 py-2.5 shadow-sm">
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2">
          <span className="truncate text-sm font-medium">{track.title}</span>
          {track.explicit && (
            <span className="inline-flex items-center justify-center rounded bg-muted px-1 text-[10px] font-bold text-muted-foreground uppercase">
              E
            </span>
          )}
        </div>
        <div className="flex flex-wrap items-center gap-1 text-xs text-muted-foreground">
          <span className="truncate">{track.artist_name}</span>
          <span>&middot;</span>
          <span className="truncate">{track.album_title}</span>
          <span>&middot;</span>
          <span>{formatDurationSeconds(track.duration_secs)}</span>
          <Badge variant="outline" className="text-[10px]">
            {providerDisplayName(track.provider)}
          </Badge>
        </div>
      </div>
      {isAdded ? (
        <AddedBadge />
      ) : (
        <Button
          size="sm"
          variant="outline"
          className="shrink-0"
          disabled={createTrack.isPending || provider == null}
          onClick={() =>
            provider &&
            createTrack.mutate({
              body: {
                external_track_id: track.external_id,
                external_album_id: track.album_external_id,
                artist_external_id: track.artist_external_id,
                artist_name: track.artist_name,
                provider,
              },
            })
          }
        >
          <PlusIcon className="mr-1 size-3.5" />
          {provider == null ? "Unsupported" : createTrack.isPending ? "Adding..." : "Add"}
        </Button>
      )}
    </div>
  );
}
