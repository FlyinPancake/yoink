import type { components } from "@/lib/api/types.gen";

type DownloadStatus = components["schemas"]["DownloadStatus"];
type WantedStatus = components["schemas"]["WantedStatus"];

export function formatDurationSeconds(totalSeconds: number): string {
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;

  if (hours > 0) {
    return `${String(hours)}:${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`;
  }

  return `${String(minutes)}:${String(seconds).padStart(2, "0")}`;
}

export function isAlbumAcquired(status: WantedStatus): boolean {
  return status === "acquired";
}

export function isAlbumWanted(status: WantedStatus): boolean {
  return status === "wanted";
}

export function isAlbumWantedLike(status: WantedStatus): boolean {
  return status === "wanted" || status === "in_progress";
}

export function isAlbumInProgress(status: WantedStatus): boolean {
  return status === "in_progress";
}

export function isDownloadActive(status: DownloadStatus): boolean {
  return status === "queued" || status === "resolving" || status === "downloading";
}

export function isDownloadHistory(status: DownloadStatus): boolean {
  return status === "completed" || status === "failed";
}

export function canCancelDownload(status: DownloadStatus): boolean {
  return status === "queued";
}

export function providerDisplayName(provider: string): string {
  switch (provider.toLowerCase()) {
    case "musicbrainz":
    case "music_brainz":
      return "MusicBrainz";
    case "soulseek":
      return "Soulseek";
    case "tidal":
      return "Tidal";
    case "deezer":
      return "Deezer";
    case "spotify":
      return "Spotify";
    case "qobuz":
      return "Qobuz";
    case "lastfm":
      return "Last.fm";
    default:
      return provider;
  }
}
