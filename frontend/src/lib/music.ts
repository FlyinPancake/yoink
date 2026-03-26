import type { components } from "@/lib/api/types.gen";

type Provider = components["schemas"]["Provider"];
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

export function normalizeProvider(provider: string): Provider | null {
  switch (provider) {
    case "tidal":
    case "deezer":
    case "music_brainz":
    case "soulseek":
    case "none":
      return provider;
    case "musicbrainz":
      return "music_brainz";
    default:
      return null;
  }
}
