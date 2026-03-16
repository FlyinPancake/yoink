/**
 * Mutation hooks for all write operations.
 *
 * Each hook wraps $api.useMutation() with automatic query invalidation
 * so the UI stays in sync after mutations.
 *
 * The create hooks for artists, albums, and tracks also insert a record
 * into the local-only `addedItemsCollection` (TanStack DB) so that the
 * search page can show an instant "Added" badge without re-fetching.
 */

import { useQueryClient } from "@tanstack/react-query";
import { $api } from "./client";
import { getCollections, addedItemKey } from "./collections";
import type { AddedItem } from "./collections";

// ── Artist mutations ───────────────────────────────────────────

export function useCreateArtist() {
  const qc = useQueryClient();
  return $api.useMutation("post", "/api/artist", {
    onSuccess: (_data, variables) => {
      void qc.invalidateQueries({ queryKey: ["get", "/api/artist"] });
      void qc.invalidateQueries({ queryKey: ["get", "/api/dashboard"] });

      // Record in TanStack DB so the search page shows "Added" instantly.
      const { provider, external_id } = variables.body;
      const { addedItemsCollection } = getCollections(qc);
      const item: AddedItem = {
        key: addedItemKey(provider, external_id),
        provider,
        external_id,
        entity_type: "artist",
      };
      addedItemsCollection.insert(item);
    },
  });
}

export function useDeleteArtist() {
  const qc = useQueryClient();
  return $api.useMutation("delete", "/api/artist/{artist_id}", {
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ["get", "/api/artist"] });
      void qc.invalidateQueries({ queryKey: ["get", "/api/dashboard"] });
      void qc.invalidateQueries({ queryKey: ["get", "/api/album"] });
    },
  });
}

export function useUpdateArtist() {
  const qc = useQueryClient();
  return $api.useMutation("patch", "/api/artist/{artist_id}", {
    onSuccess: (_data, variables) => {
      void qc.invalidateQueries({ queryKey: ["get", "/api/artist"] });
      void qc.invalidateQueries({
        queryKey: [
          "get",
          "/api/artist/{artist_id}",
          { params: { path: { artist_id: variables.params.path.artist_id } } },
        ],
      });
    },
  });
}

export function useToggleArtistMonitor() {
  const qc = useQueryClient();
  return $api.useMutation("patch", "/api/artist/{artist_id}/monitor", {
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ["get", "/api/artist"] });
      void qc.invalidateQueries({ queryKey: ["get", "/api/dashboard"] });
    },
  });
}

export function useSyncArtist() {
  const qc = useQueryClient();
  return $api.useMutation("post", "/api/artist/{artist_id}/sync", {
    onSuccess: (_data, variables) => {
      void qc.invalidateQueries({
        queryKey: [
          "get",
          "/api/artist/{artist_id}",
          { params: { path: { artist_id: variables.params.path.artist_id } } },
        ],
      });
      void qc.invalidateQueries({ queryKey: ["get", "/api/album"] });
    },
  });
}

export function useFetchArtistBio() {
  const qc = useQueryClient();
  return $api.useMutation("post", "/api/artist/{artist_id}/fetch-bio", {
    onSuccess: (_data, variables) => {
      void qc.invalidateQueries({
        queryKey: [
          "get",
          "/api/artist/{artist_id}",
          { params: { path: { artist_id: variables.params.path.artist_id } } },
        ],
      });
    },
  });
}

export function useLinkArtistProvider() {
  const qc = useQueryClient();
  return $api.useMutation("post", "/api/artist/{artist_id}/provider", {
    onSuccess: (_data, variables) => {
      void qc.invalidateQueries({
        queryKey: [
          "get",
          "/api/artist/{artist_id}",
          { params: { path: { artist_id: variables.params.path.artist_id } } },
        ],
      });
    },
  });
}

export function useUnlinkArtistProvider() {
  const qc = useQueryClient();
  return $api.useMutation("delete", "/api/artist/{artist_id}/provider", {
    onSuccess: (_data, variables) => {
      void qc.invalidateQueries({
        queryKey: [
          "get",
          "/api/artist/{artist_id}",
          { params: { path: { artist_id: variables.params.path.artist_id } } },
        ],
      });
    },
  });
}

export function useRefreshArtistMatchSuggestions() {
  const qc = useQueryClient();
  return $api.useMutation("post", "/api/artist/{artist_id}/match-suggestion/refresh", {
    onSuccess: (_data, variables) => {
      void qc.invalidateQueries({
        queryKey: [
          "get",
          "/api/artist/{artist_id}",
          {
            params: {
              path: { artist_id: variables.params.path.artist_id },
            },
          },
        ],
      });
    },
  });
}

// ── Album mutations ────────────────────────────────────────────

export function useCreateAlbum() {
  const qc = useQueryClient();
  return $api.useMutation("post", "/api/album", {
    onSuccess: (_data, variables) => {
      void qc.invalidateQueries({ queryKey: ["get", "/api/album"] });
      void qc.invalidateQueries({ queryKey: ["get", "/api/dashboard"] });
      void qc.invalidateQueries({ queryKey: ["get", "/api/wanted"] });

      // Record in TanStack DB so the search page shows "Added" instantly.
      const { provider, external_album_id } = variables.body;
      const { addedItemsCollection } = getCollections(qc);
      const item: AddedItem = {
        key: addedItemKey(provider, external_album_id),
        provider,
        external_id: external_album_id,
        entity_type: "album",
      };
      addedItemsCollection.insert(item);
    },
  });
}

export function useMergeAlbums() {
  const qc = useQueryClient();
  return $api.useMutation("post", "/api/album/merge", {
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ["get", "/api/album"] });
      void qc.invalidateQueries({ queryKey: ["get", "/api/dashboard"] });
    },
  });
}

export function useToggleAlbumMonitor() {
  const qc = useQueryClient();
  return $api.useMutation("patch", "/api/album/{album_id}/monitor", {
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ["get", "/api/album"] });
      void qc.invalidateQueries({ queryKey: ["get", "/api/wanted"] });
      void qc.invalidateQueries({ queryKey: ["get", "/api/dashboard"] });
    },
  });
}

export function useSetAlbumQuality() {
  const qc = useQueryClient();
  return $api.useMutation("patch", "/api/album/{album_id}/quality", {
    onSuccess: (_data, variables) => {
      void qc.invalidateQueries({
        queryKey: [
          "get",
          "/api/album/{album_id}",
          { params: { path: { album_id: variables.params.path.album_id } } },
        ],
      });
    },
  });
}

export function useRemoveAlbumFiles() {
  const qc = useQueryClient();
  return $api.useMutation("delete", "/api/album/{album_id}/file", {
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ["get", "/api/album"] });
      void qc.invalidateQueries({ queryKey: ["get", "/api/dashboard"] });
      void qc.invalidateQueries({ queryKey: ["get", "/api/wanted"] });
    },
  });
}

export function useRetryDownload() {
  const qc = useQueryClient();
  return $api.useMutation("post", "/api/album/{album_id}/download/retry", {
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ["get", "/api/job"] });
      void qc.invalidateQueries({ queryKey: ["get", "/api/dashboard"] });
    },
  });
}

export function useToggleAlbumTrackMonitor() {
  const qc = useQueryClient();
  return $api.useMutation("patch", "/api/album/{album_id}/track/monitor", {
    onSuccess: (_data, variables) => {
      void qc.invalidateQueries({
        queryKey: [
          "get",
          "/api/album/{album_id}",
          { params: { path: { album_id: variables.params.path.album_id } } },
        ],
      });
      void qc.invalidateQueries({ queryKey: ["get", "/api/wanted"] });
    },
  });
}

export function useToggleSingleTrackMonitor() {
  const qc = useQueryClient();
  return $api.useMutation("patch", "/api/album/{album_id}/track/{track_id}/monitor", {
    onSuccess: (_data, variables) => {
      void qc.invalidateQueries({
        queryKey: [
          "get",
          "/api/album/{album_id}",
          {
            params: {
              path: { album_id: variables.params.path.album_id },
            },
          },
        ],
      });
      void qc.invalidateQueries({ queryKey: ["get", "/api/wanted"] });
    },
  });
}

export function useSetTrackQuality() {
  const qc = useQueryClient();
  return $api.useMutation("patch", "/api/album/{album_id}/track/{track_id}/quality", {
    onSuccess: (_data, variables) => {
      void qc.invalidateQueries({
        queryKey: [
          "get",
          "/api/album/{album_id}",
          {
            params: {
              path: { album_id: variables.params.path.album_id },
            },
          },
        ],
      });
    },
  });
}

export function useAddAlbumArtist() {
  const qc = useQueryClient();
  return $api.useMutation("post", "/api/album/{album_id}/artist", {
    onSuccess: (_data, variables) => {
      void qc.invalidateQueries({
        queryKey: [
          "get",
          "/api/album/{album_id}",
          { params: { path: { album_id: variables.params.path.album_id } } },
        ],
      });
    },
  });
}

export function useRemoveAlbumArtist() {
  const qc = useQueryClient();
  return $api.useMutation("delete", "/api/album/{album_id}/artist/{artist_id}", {
    onSuccess: (_data, variables) => {
      void qc.invalidateQueries({
        queryKey: [
          "get",
          "/api/album/{album_id}",
          {
            params: {
              path: { album_id: variables.params.path.album_id },
            },
          },
        ],
      });
    },
  });
}

// ── Track mutations ────────────────────────────────────────────

export function useCreateTrack() {
  const qc = useQueryClient();
  return $api.useMutation("post", "/api/track", {
    onSuccess: (_data, variables) => {
      void qc.invalidateQueries({ queryKey: ["get", "/api/track"] });
      void qc.invalidateQueries({ queryKey: ["get", "/api/album"] });
      void qc.invalidateQueries({ queryKey: ["get", "/api/dashboard"] });

      // Record in TanStack DB so the search page shows "Added" instantly.
      const { provider, external_track_id } = variables.body;
      const { addedItemsCollection } = getCollections(qc);
      const item: AddedItem = {
        key: addedItemKey(provider, external_track_id),
        provider,
        external_id: external_track_id,
        entity_type: "track",
      };
      addedItemsCollection.insert(item);
    },
  });
}

// ── Job mutations ──────────────────────────────────────────────

export function useCancelJob() {
  const qc = useQueryClient();
  return $api.useMutation("post", "/api/job/{job_id}/cancel", {
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ["get", "/api/job"] });
      void qc.invalidateQueries({ queryKey: ["get", "/api/dashboard"] });
    },
  });
}

export function useClearCompletedJobs() {
  const qc = useQueryClient();
  return $api.useMutation("delete", "/api/job/completed", {
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ["get", "/api/job"] });
      void qc.invalidateQueries({ queryKey: ["get", "/api/dashboard"] });
    },
  });
}

// ── Match suggestion mutations ─────────────────────────────────

export function useAcceptMatchSuggestion() {
  const qc = useQueryClient();
  return $api.useMutation("post", "/api/match-suggestion/{suggestion_id}/accept", {
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ["get", "/api/artist"] });
      void qc.invalidateQueries({ queryKey: ["get", "/api/album"] });
    },
  });
}

export function useDismissMatchSuggestion() {
  const qc = useQueryClient();
  return $api.useMutation("post", "/api/match-suggestion/{suggestion_id}/dismiss", {
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ["get", "/api/artist"] });
      void qc.invalidateQueries({ queryKey: ["get", "/api/album"] });
    },
  });
}

// ── Import mutations ───────────────────────────────────────────

export function useScanImport() {
  return $api.useMutation("post", "/api/import/scan");
}

export function useConfirmImport() {
  const qc = useQueryClient();
  return $api.useMutation("post", "/api/import/confirm", {
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ["get", "/api/artist"] });
      void qc.invalidateQueries({ queryKey: ["get", "/api/album"] });
      void qc.invalidateQueries({ queryKey: ["get", "/api/dashboard"] });
    },
  });
}

export function useBrowsePath() {
  return $api.useMutation("post", "/api/import/browse");
}

export function usePreviewExternalImport() {
  return $api.useMutation("post", "/api/import/external/preview");
}

export function useConfirmExternalImport() {
  const qc = useQueryClient();
  return $api.useMutation("post", "/api/import/external/confirm", {
    onSuccess: () => {
      void qc.invalidateQueries({ queryKey: ["get", "/api/artist"] });
      void qc.invalidateQueries({ queryKey: ["get", "/api/album"] });
      void qc.invalidateQueries({ queryKey: ["get", "/api/dashboard"] });
    },
  });
}
