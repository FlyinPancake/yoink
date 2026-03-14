/**
 * Barrel export for the API layer.
 *
 * Import everything from "@/lib/api" for convenience:
 *   import { $api, queryKeys, useCreateArtist } from "@/lib/api";
 */

export { fetchClient, $api } from "./client";
export { queryKeys } from "./queries";
export { connectSSE } from "./events";
export { getCollections, addedItemKey } from "./collections";
export type {
  MonitoredArtist,
  MonitoredAlbum,
  LibraryTrack,
  DownloadJob,
  AddedItem,
} from "./collections";
export type { components, paths, operations } from "./types.gen";

// Re-export all mutation hooks
export {
  useCreateArtist,
  useDeleteArtist,
  useUpdateArtist,
  useToggleArtistMonitor,
  useSyncArtist,
  useFetchArtistBio,
  useLinkArtistProvider,
  useUnlinkArtistProvider,
  useRefreshArtistMatchSuggestions,
  useCreateAlbum,
  useMergeAlbums,
  useToggleAlbumMonitor,
  useSetAlbumQuality,
  useRemoveAlbumFiles,
  useRetryDownload,
  useToggleAlbumTrackMonitor,
  useToggleSingleTrackMonitor,
  useSetTrackQuality,
  useAddAlbumArtist,
  useRemoveAlbumArtist,
  useCreateTrack,
  useCancelJob,
  useClearCompletedJobs,
  useAcceptMatchSuggestion,
  useDismissMatchSuggestion,
  useScanImport,
  useConfirmImport,
  useBrowsePath,
  usePreviewExternalImport,
  useConfirmExternalImport,
} from "./mutations";
