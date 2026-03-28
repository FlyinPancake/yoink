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
import { isDownloadActive } from "@/lib/music";
import { $api } from "./client";
import { getCollections, addedItemKey } from "./collections";
import { queryKeys } from "./queries";
import type { AddedItem } from "./collections";
import type { components } from "./types.gen";

type MonitoredArtist = components["schemas"]["MonitoredArtist"];
type LibraryAlbumSummary = components["schemas"]["LibraryAlbumSummary"];
type TrackInfo = components["schemas"]["TrackInfo"];
type ArtistDetailResponse = components["schemas"]["ArtistDetailResponse"];
type AlbumDetailResponse = components["schemas"]["AlbumDetailResponse"];
type DashboardData = components["schemas"]["DashboardData"];
type WantedData = components["schemas"]["WantedData"];
type DownloadJob = components["schemas"]["DownloadJob"];
type Quality = components["schemas"]["Quality"];
type WantedStatus = components["schemas"]["WantedStatus"];

function deriveWantedStatus(current: WantedStatus, monitored: boolean): WantedStatus {
  if (!monitored) {
    return "unmonitored";
  }

  return current === "unmonitored" ? "wanted" : current;
}

function patchArtistMonitorCaches(
  queryClient: ReturnType<typeof useQueryClient>,
  artistId: string,
  monitored: boolean,
) {
  queryClient.setQueryData<Array<MonitoredArtist> | undefined>(["get", "/api/artist"], (current) =>
    current?.map((artist) => (artist.id === artistId ? { ...artist, monitored } : artist)),
  );

  queryClient.setQueriesData<ArtistDetailResponse | undefined>(
    { queryKey: ["get", "/api/artist/{artist_id}"] },
    (current) =>
      current?.artist.id === artistId
        ? {
            ...current,
            artist: { ...current.artist, monitored },
          }
        : current,
  );
}

function patchAlbumMonitorCaches(
  queryClient: ReturnType<typeof useQueryClient>,
  albumId: string,
  monitored: boolean,
) {
  queryClient.setQueryData<Array<LibraryAlbumSummary> | undefined>(
    ["get", "/api/album"],
    (current) =>
      current?.map((album) =>
        album.id === albumId
          ? {
              ...album,
              monitored,
              wanted_status: deriveWantedStatus(album.wanted_status, monitored),
            }
          : album,
      ),
  );

  queryClient.setQueriesData<ArtistDetailResponse | undefined>(
    { queryKey: ["get", "/api/artist/{artist_id}"] },
    (current) =>
      current
        ? {
            ...current,
            albums: current.albums.map((album) =>
              album.id === albumId
                ? {
                    ...album,
                    monitored,
                    wanted_status: deriveWantedStatus(album.wanted_status, monitored),
                  }
                : album,
            ),
          }
        : current,
  );

  queryClient.setQueriesData<AlbumDetailResponse | undefined>(
    { queryKey: ["get", "/api/album/{album_id}"] },
    (current) =>
      current?.album.id === albumId
        ? {
            ...current,
            album: {
              ...current.album,
              monitored,
              wanted_status: deriveWantedStatus(current.album.wanted_status, monitored),
            },
          }
        : current,
  );
}

function patchTrackMonitorCaches(
  queryClient: ReturnType<typeof useQueryClient>,
  albumId: string,
  trackId: string,
  monitored: boolean,
) {
  queryClient.setQueriesData<AlbumDetailResponse | undefined>(
    { queryKey: ["get", "/api/album/{album_id}"] },
    (current) =>
      current?.album.id === albumId
        ? {
            ...current,
            tracks: current.tracks.map((track) =>
              track.id === trackId ? { ...track, monitored } : track,
            ),
          }
        : current,
  );

  queryClient.setQueriesData<Array<TrackInfo> | undefined>(
    { queryKey: ["get", "/api/album/{album_id}/track"] },
    (current) => current?.map((track) => (track.id === trackId ? { ...track, monitored } : track)),
  );
}

function nextDownloadWantedStatus(current: WantedStatus): WantedStatus {
  return current === "acquired" ? current : "in_progress";
}

function upsertAlbumJob(jobs: Array<DownloadJob>, job: DownloadJob): Array<DownloadJob> {
  const filtered = jobs.filter(
    (existing) =>
      existing.id !== job.id &&
      !(
        existing.album_id === job.album_id &&
        existing.kind === "album" &&
        isDownloadActive(existing.status)
      ),
  );

  return [job, ...filtered];
}

function buildOptimisticAlbumJob(
  albumId: string,
  albumTitle: string,
  artistName: string,
  quality: Quality,
  source: string,
): DownloadJob {
  const now = new Date().toISOString();

  return {
    id: `optimistic-download-${albumId}`,
    kind: "album",
    album_id: albumId,
    track_id: null,
    source,
    album_title: albumTitle,
    track_title: null,
    artist_name: artistName,
    status: "queued",
    quality,
    total_tracks: 0,
    completed_tracks: 0,
    error: null,
    created_at: now,
    updated_at: now,
  };
}

function patchAlbumDownloadCaches(queryClient: ReturnType<typeof useQueryClient>, albumId: string) {
  const detailKey = queryKeys.albums.detail(albumId).queryKey;
  const detail = queryClient.getQueryData<AlbumDetailResponse>(detailKey);
  const detailArtistName = detail?.album_artists[0]?.name ?? "";
  const detailJob = detail?.jobs.find((job) => job.album_id === albumId);
  const optimisticJob =
    detail != null
      ? buildOptimisticAlbumJob(
          albumId,
          detail.album.title,
          detailArtistName,
          detailJob?.quality ?? detail.default_quality,
          detailJob?.source ?? detail.provider_links[0]?.provider ?? "tidal",
        )
      : null;

  queryClient.setQueryData<AlbumDetailResponse | undefined>(detailKey, (current) =>
    current == null
      ? current
      : {
          ...current,
          album: {
            ...current.album,
            monitored: true,
            wanted_status: nextDownloadWantedStatus(current.album.wanted_status),
          },
          jobs: optimisticJob == null ? current.jobs : upsertAlbumJob(current.jobs, optimisticJob),
        },
  );

  queryClient.setQueryData<Array<LibraryAlbumSummary> | undefined>(
    queryKeys.albums.list().queryKey,
    (current) =>
      current?.map((album) =>
        album.id === albumId
          ? {
              ...album,
              monitored: true,
              wanted_status: nextDownloadWantedStatus(album.wanted_status),
            }
          : album,
      ),
  );

  queryClient.setQueriesData<ArtistDetailResponse | undefined>(
    { queryKey: ["get", "/api/artist/{artist_id}"] },
    (current) =>
      current
        ? {
            ...current,
            albums: current.albums.map((album) =>
              album.id === albumId
                ? {
                    ...album,
                    monitored: true,
                    wanted_status: nextDownloadWantedStatus(album.wanted_status),
                  }
                : album,
            ),
          }
        : current,
  );

  queryClient.setQueryData<DashboardData | undefined>(queryKeys.dashboard().queryKey, (current) => {
    if (current == null) {
      return current;
    }

    const dashboardAlbum = current.albums.find((album) => album.id === albumId);
    const artistName =
      dashboardAlbum?.artist_id != null
        ? (current.artists.find((artist) => artist.id === dashboardAlbum.artist_id)?.name ?? "")
        : detailArtistName;
    const latestAlbumJob = current.jobs.find(
      (job) => job.album_id === albumId && job.kind === "album",
    );
    const queuedJob = buildOptimisticAlbumJob(
      albumId,
      dashboardAlbum?.title ??
        detail?.album.title ??
        latestAlbumJob?.album_title ??
        "Unknown Album",
      artistName,
      latestAlbumJob?.quality ?? optimisticJob?.quality ?? "Lossless",
      latestAlbumJob?.source ?? optimisticJob?.source ?? "tidal",
    );

    return {
      ...current,
      albums: current.albums.map((album) =>
        album.id === albumId
          ? {
              ...album,
              monitored: true,
              wanted_status: nextDownloadWantedStatus(album.wanted_status),
            }
          : album,
      ),
      jobs: upsertAlbumJob(current.jobs, queuedJob),
    };
  });

  queryClient.setQueryData<WantedData | undefined>(queryKeys.wanted().queryKey, (current) => {
    if (current == null) {
      return current;
    }

    const wantedAlbum = current.albums.find((entry) => entry.album.id === albumId);
    const artistName =
      wantedAlbum?.album.artist_id != null
        ? (current.artists.find((artist) => artist.id === wantedAlbum.album.artist_id)?.name ?? "")
        : detailArtistName;
    const latestAlbumJob = current.jobs.find(
      (job) => job.album_id === albumId && job.kind === "album",
    );
    const queuedJob = buildOptimisticAlbumJob(
      albumId,
      wantedAlbum?.album.title ??
        detail?.album.title ??
        latestAlbumJob?.album_title ??
        "Unknown Album",
      artistName,
      latestAlbumJob?.quality ?? optimisticJob?.quality ?? "Lossless",
      latestAlbumJob?.source ?? optimisticJob?.source ?? "tidal",
    );

    return {
      ...current,
      albums: current.albums.map((entry) =>
        entry.album.id === albumId
          ? {
              ...entry,
              album: {
                ...entry.album,
                monitored: true,
                wanted_status: nextDownloadWantedStatus(entry.album.wanted_status),
              },
            }
          : entry,
      ),
      jobs: upsertAlbumJob(current.jobs, queuedJob),
    };
  });

  if (optimisticJob != null) {
    queryClient.setQueryData<Array<DownloadJob> | undefined>(
      queryKeys.jobs.list().queryKey,
      (current) => (current == null ? [optimisticJob] : upsertAlbumJob(current, optimisticJob)),
    );
  }
}

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
    onSuccess: (_data, variables) => {
      patchArtistMonitorCaches(qc, variables.params.path.artist_id, variables.body.monitored);
      void qc.invalidateQueries({ queryKey: ["get", "/api/artist"] });
      void qc.invalidateQueries({ queryKey: ["get", "/api/artist/{artist_id}"] });
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
    onSuccess: (_data, variables) => {
      patchAlbumMonitorCaches(qc, variables.params.path.album_id, variables.body.monitored);
      void qc.invalidateQueries({ queryKey: ["get", "/api/album"] });
      void qc.invalidateQueries({ queryKey: ["get", "/api/album/{album_id}"] });
      void qc.invalidateQueries({ queryKey: ["get", "/api/artist/{artist_id}"] });
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
        queryKey: queryKeys.albums.detail(variables.params.path.album_id).queryKey,
      });
      void qc.invalidateQueries({ queryKey: queryKeys.albums.list().queryKey });
      void qc.invalidateQueries({ queryKey: ["get", "/api/artist/{artist_id}"] });
    },
  });
}

export function useRemoveAlbumFiles() {
  const qc = useQueryClient();
  return $api.useMutation("delete", "/api/album/{album_id}/file", {
    onSuccess: (_data, variables) => {
      void qc.invalidateQueries({
        queryKey: [
          "get",
          "/api/album/{album_id}",
          { params: { path: { album_id: variables.params.path.album_id } } },
        ],
      });
      void qc.invalidateQueries({ queryKey: ["get", "/api/artist/{artist_id}"] });
      void qc.invalidateQueries({ queryKey: ["get", "/api/album"] });
      void qc.invalidateQueries({ queryKey: ["get", "/api/dashboard"] });
      void qc.invalidateQueries({ queryKey: ["get", "/api/wanted"] });
    },
  });
}

export function useRetryDownload() {
  const qc = useQueryClient();
  return $api.useMutation("post", "/api/album/{album_id}/download/retry", {
    onMutate: async (variables) => {
      patchAlbumDownloadCaches(qc, variables.params.path.album_id);
    },
    onError: (_error, variables) => {
      void qc.invalidateQueries({
        queryKey: queryKeys.albums.detail(variables.params.path.album_id).queryKey,
      });
      void qc.invalidateQueries({ queryKey: queryKeys.albums.list().queryKey });
      void qc.invalidateQueries({ queryKey: queryKeys.dashboard().queryKey });
      void qc.invalidateQueries({ queryKey: queryKeys.wanted().queryKey });
      void qc.invalidateQueries({ queryKey: queryKeys.jobs.list().queryKey });
      void qc.invalidateQueries({ queryKey: ["get", "/api/artist/{artist_id}"] });
    },
    onSuccess: (_data, variables) => {
      void qc.invalidateQueries({
        queryKey: queryKeys.albums.detail(variables.params.path.album_id).queryKey,
      });
      void qc.invalidateQueries({ queryKey: queryKeys.albums.list().queryKey });
      void qc.invalidateQueries({ queryKey: queryKeys.jobs.list().queryKey });
      void qc.invalidateQueries({ queryKey: queryKeys.dashboard().queryKey });
      void qc.invalidateQueries({ queryKey: queryKeys.wanted().queryKey });
      void qc.invalidateQueries({ queryKey: ["get", "/api/artist/{artist_id}"] });
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
      patchTrackMonitorCaches(
        qc,
        variables.params.path.album_id,
        variables.params.path.track_id,
        variables.body.monitored,
      );
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
      void qc.invalidateQueries({
        queryKey: [
          "get",
          "/api/album/{album_id}/track",
          { params: { path: { album_id: variables.params.path.album_id } } },
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
      void qc.invalidateQueries({
        queryKey: [
          "get",
          "/api/album/{album_id}/track",
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
