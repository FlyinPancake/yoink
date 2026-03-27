import type { components } from "@/lib/api/types.gen";

export type LibraryAlbumSummary = components["schemas"]["LibraryAlbumSummary"];
export type AlbumSort = "added" | "artist" | "az" | "newest" | "oldest" | "za";

export function getLibraryAlbumArtistName(album: LibraryAlbumSummary): string {
  return album.artist_name ?? "Unknown Artist";
}

export function canLinkLibraryAlbum(album: LibraryAlbumSummary): boolean {
  return album.artist_id != null;
}

export function sortLibraryAlbums(
  list: LibraryAlbumSummary[],
  sort: AlbumSort,
): LibraryAlbumSummary[] {
  const sorted = [...list];

  switch (sort) {
    case "az":
      sorted.sort((a, b) => a.title.toLowerCase().localeCompare(b.title.toLowerCase()));
      break;
    case "za":
      sorted.sort((a, b) => b.title.toLowerCase().localeCompare(a.title.toLowerCase()));
      break;
    case "artist":
      sorted.sort((a, b) =>
        getLibraryAlbumArtistName(a)
          .toLowerCase()
          .localeCompare(getLibraryAlbumArtistName(b).toLowerCase()),
      );
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
    case "added":
      sorted.sort((a, b) => new Date(b.created_at).getTime() - new Date(a.created_at).getTime());
      break;
  }

  return sorted;
}

export function filterLibraryAlbums(
  list: LibraryAlbumSummary[],
  search: string,
): LibraryAlbumSummary[] {
  const query = search.trim().toLowerCase();
  if (query === "") {
    return list;
  }

  return list.filter(
    (album) =>
      album.title.toLowerCase().includes(query) ||
      getLibraryAlbumArtistName(album).toLowerCase().includes(query),
  );
}
