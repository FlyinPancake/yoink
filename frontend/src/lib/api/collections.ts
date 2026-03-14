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
 * Collections require a QueryClient instance, so they are created lazily
 * via `getCollections(queryClient)`.
 */

import { createCollection } from "@tanstack/react-db";
import { queryCollectionOptions } from "@tanstack/query-db-collection";
import { fetchClient } from "./client";
import type { QueryClient } from "@tanstack/react-query";
import type { components } from "./types.gen";

// ── Type aliases for convenience ───────────────────────────────

export type MonitoredArtist = components["schemas"]["MonitoredArtist"];
export type MonitoredAlbum = components["schemas"]["MonitoredAlbum"];
export type LibraryTrack = components["schemas"]["LibraryTrack"];
export type DownloadJob = components["schemas"]["DownloadJob"];

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

  return {
    artistsCollection,
    albumsCollection,
    tracksCollection,
    jobsCollection,
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
