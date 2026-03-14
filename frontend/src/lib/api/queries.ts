/**
 * Centralised query-key factory and re-exported typed hooks.
 *
 * Uses openapi-react-query ($api) which automatically generates
 * fully-typed useQuery/useSuspenseQuery/useMutation wrappers
 * from the OpenAPI paths type.
 *
 * This file also exposes a `queryKeys` namespace so invalidation
 * and prefetching can reference canonical keys without coupling
 * to the URL strings.
 */

import { $api } from "./client";

// ── Query key helpers ──────────────────────────────────────────
// These match the keys that openapi-react-query generates internally
// so they can be used with queryClient.invalidateQueries().

export const queryKeys = {
  // Dashboard
  dashboard: () => $api.queryOptions("get", "/api/dashboard"),

  // Artists
  artists: {
    list: () => $api.queryOptions("get", "/api/artist", {}),
    detail: (artistId: string) =>
      $api.queryOptions("get", "/api/artist/{artist_id}", {
        params: { path: { artist_id: artistId } },
      }),
    search: (query: string) =>
      $api.queryOptions("get", "/api/artist/search", {
        params: { query: { query } },
      }),
    providers: (artistId: string) =>
      $api.queryOptions("get", "/api/artist/{artist_id}/provider", {
        params: { path: { artist_id: artistId } },
      }),
    images: (artistId: string) =>
      $api.queryOptions("get", "/api/artist/{artist_id}/image", {
        params: { path: { artist_id: artistId } },
      }),
  },

  // Albums
  albums: {
    list: () => $api.queryOptions("get", "/api/album", {}),
    detail: (albumId: string) =>
      $api.queryOptions("get", "/api/album/{album_id}", {
        params: { path: { album_id: albumId } },
      }),
    search: (query: string) =>
      $api.queryOptions("get", "/api/album/search", {
        params: { query: { query } },
      }),
    tracks: (albumId: string) =>
      $api.queryOptions("get", "/api/album/{album_id}/track", {
        params: { path: { album_id: albumId } },
      }),
    providers: (albumId: string) =>
      $api.queryOptions("get", "/api/album/{album_id}/provider", {
        params: { path: { album_id: albumId } },
      }),
  },

  // Tracks
  tracks: {
    list: () => $api.queryOptions("get", "/api/track", {}),
    search: (query: string) =>
      $api.queryOptions("get", "/api/track/search", {
        params: { query: { query } },
      }),
  },

  // Search (aggregated)
  search: (query: string) =>
    $api.queryOptions("get", "/api/search", {
      params: { query: { query } },
    }),

  // Wanted
  wanted: () => $api.queryOptions("get", "/api/wanted", {}),

  // Jobs
  jobs: {
    list: () => $api.queryOptions("get", "/api/job", {}),
  },

  // Providers
  providers: () => $api.queryOptions("get", "/api/provider", {}),

  // Auth
  authStatus: () => $api.queryOptions("get", "/api/auth/status", {}),

  // Import
  importPreview: () => $api.queryOptions("get", "/api/import/preview", {}),
} as const;
