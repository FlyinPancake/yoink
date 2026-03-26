/**
 * TanStack DB collections for core domain entities.
 *
 * Collections provide:
 * - Normalised client-side storage for entities loaded from multiple endpoints
 * - Sub-millisecond live queries with incremental updates
 * - Optimistic mutations that overlay synced data
 *
 * Data is loaded into collections via TanStack Query (queryCollectionOptions),
 * so the existing query cache, SSE invalidation, and stale-while-revalidate
 * behaviour all work seamlessly.
 *
 * The `addedItemsCollection` is a local-only collection that tracks
 * `(provider, external_id, entity_type)` tuples added during the current
 * session so the search page can show instant "Added" badges.
 *
 * Collections require a QueryClient instance, so they are created lazily
 * via `getCollections(queryClient)`.
 */

import { createCollection, localOnlyCollectionOptions } from "@tanstack/react-db";
import { queryCollectionOptions } from "@tanstack/query-db-collection";
import { fetchClient } from "./client";
import type { QueryClient } from "@tanstack/react-query";
import type { components } from "./types.gen";

// ── Type aliases for convenience ───────────────────────────────

export type MonitoredArtist = components["schemas"]["MonitoredArtist"];
export type MonitoredAlbum = components["schemas"]["Album"];
export type LibraryTrack = components["schemas"]["LibraryTrack"];
export type DownloadJob = components["schemas"]["DownloadJob"];

/**
 * A lightweight record inserted into the local-only `addedItemsCollection`
 * whenever the user adds an artist/album/track from search results.
 * Keyed by `"provider:external_id"` for O(1) lookups.
 */
export interface AddedItem {
  /** Composite key: `"${provider}:${external_id}"` */
  key: string;
  provider: string;
  external_id: string;
  entity_type: "artist" | "album" | "track";
}

// ── Lazy singleton ─────────────────────────────────────────────

let _collections: ReturnType<typeof createCollections> | null = null;

function createCollections(queryClient: QueryClient) {
  const artistsCollection = createCollection(
    queryCollectionOptions({
      id: "artists",
      queryKey: ["get", "/api/artist"],
      queryClient,
      queryFn: async () => {
        const { data } = await fetchClient.GET("/api/artist");
        return data ?? [];
      },
      getKey: (artist: MonitoredArtist) => artist.id,
    }),
  );

  const albumsCollection = createCollection(
    queryCollectionOptions({
      id: "albums",
      queryKey: ["get", "/api/album"],
      queryClient,
      queryFn: async () => {
        const { data } = await fetchClient.GET("/api/album");
        return data ?? [];
      },
      getKey: (album: MonitoredAlbum) => album.id,
    }),
  );

  const tracksCollection = createCollection(
    queryCollectionOptions({
      id: "tracks",
      queryKey: ["get", "/api/track"],
      queryClient,
      queryFn: async () => {
        const { data } = await fetchClient.GET("/api/track");
        return data ?? [];
      },
      getKey: (track: LibraryTrack) => track.track.id,
    }),
  );

  const jobsCollection = createCollection(
    queryCollectionOptions({
      id: "jobs",
      queryKey: ["get", "/api/job"],
      queryClient,
      queryFn: async () => {
        const { data } = await fetchClient.GET("/api/job");
        return data ?? [];
      },
      getKey: (job: DownloadJob) => job.id,
    }),
  );

  /**
   * Session-local collection that tracks items the user has added from
   * search results.  This allows the search page to show an instant
   * "Added" badge even before the search query is re-fetched from the
   * server.
   */
  const addedItemsCollection = createCollection<AddedItem, string>(
    localOnlyCollectionOptions({
      id: "added-items",
      getKey: (item: AddedItem) => item.key,
    }),
  );

  return {
    artistsCollection,
    albumsCollection,
    tracksCollection,
    jobsCollection,
    addedItemsCollection,
  };
}

/**
 * Get or create the singleton collections instance.
 * Call this after the QueryClient is available (e.g. in the router setup).
 */
export function getCollections(queryClient: QueryClient) {
  if (!_collections) {
    _collections = createCollections(queryClient);
  }
  return _collections;
}

// ── Helper ─────────────────────────────────────────────────────

/** Build the composite key used by `addedItemsCollection`. */
export function addedItemKey(provider: string, externalId: string): string {
  return `${provider}:${externalId}`;
}
