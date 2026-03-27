import { describe, expect, it } from "vitest";
import {
  canLinkLibraryAlbum,
  filterLibraryAlbums,
  getLibraryAlbumArtistName,
  sortLibraryAlbums,
} from "./albums";
import type { LibraryAlbumSummary } from "./albums";

function album(overrides: Partial<LibraryAlbumSummary> = {}): LibraryAlbumSummary {
  return {
    id: overrides.id ?? "album-1",
    title: overrides.title ?? "Album Title",
    album_type: overrides.album_type ?? "album",
    release_date: "release_date" in overrides ? overrides.release_date! : "2024-01-01",
    cover_url: "cover_url" in overrides ? overrides.cover_url! : null,
    explicit: overrides.explicit ?? false,
    monitored: overrides.monitored ?? true,
    wanted_status: overrides.wanted_status ?? "wanted",
    quality_override: "quality_override" in overrides ? overrides.quality_override! : null,
    created_at: overrides.created_at ?? "2024-01-02T00:00:00Z",
    artist_id: "artist_id" in overrides ? overrides.artist_id! : "artist-1",
    artist_name: "artist_name" in overrides ? overrides.artist_name! : "Artist Name",
  };
}

describe("getLibraryAlbumArtistName", () => {
  it("returns the artist name from the API response", () => {
    expect(getLibraryAlbumArtistName(album({ artist_name: "Radiohead" }))).toBe("Radiohead");
  });

  it("falls back to Unknown Artist when artist name is missing", () => {
    expect(getLibraryAlbumArtistName(album({ artist_id: null, artist_name: null }))).toBe(
      "Unknown Artist",
    );
  });
});

describe("filterLibraryAlbums", () => {
  it("matches albums by artist name", () => {
    const albums = [
      album({ id: "1", artist_name: "Aphex Twin" }),
      album({ id: "2", artist_name: "Burial" }),
    ];

    expect(filterLibraryAlbums(albums, "burial").map((entry) => entry.id)).toEqual(["2"]);
  });
});

describe("sortLibraryAlbums", () => {
  it("sorts albums by artist name", () => {
    const albums = [
      album({ id: "1", artist_name: "Burial" }),
      album({ id: "2", artist_name: "Aphex Twin" }),
    ];

    expect(sortLibraryAlbums(albums, "artist").map((entry) => entry.id)).toEqual(["2", "1"]);
  });
});

describe("canLinkLibraryAlbum", () => {
  it("returns true when artist_id exists", () => {
    expect(canLinkLibraryAlbum(album({ artist_id: "artist-1" }))).toBe(true);
  });

  it("returns false when artist_id is missing", () => {
    expect(canLinkLibraryAlbum(album({ artist_id: null, artist_name: null }))).toBe(false);
  });
});
