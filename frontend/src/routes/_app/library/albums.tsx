import { useMemo, useState } from "react";
import { createFileRoute, Link } from "@tanstack/react-router";
import { SearchIcon } from "lucide-react";
import { $api } from "@/lib/api";
import { useSleeveGlow } from "@/hooks/use-sleeve-glow";
import { useLocalStorage } from "@/hooks/use-local-storage";
import { isAlbumAcquired, isAlbumInProgress, isAlbumWanted } from "@/lib/music";
import {
  canLinkLibraryAlbum,
  filterLibraryAlbums,
  getLibraryAlbumArtistName,
  sortLibraryAlbums,
} from "@/lib/library/albums";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";
import type { AlbumSort, LibraryAlbumSummary as AlbumListItem } from "@/lib/library/albums";

export const Route = createFileRoute("/_app/library/albums")({
  component: AlbumsPage,
  staticData: {
    breadcrumb: "Albums",
  },
});

// ── Helpers ────────────────────────────────────────────────────

const ALBUM_TYPE_ORDER = [
  "album",
  "ep",
  "single",
  "compilation",
  "live",
  "remix",
  "soundtrack",
  "other",
] as const;

const ALBUM_TYPE_LABELS: Record<string, string> = {
  album: "Albums",
  ep: "EPs",
  single: "Singles",
  compilation: "Compilations",
  live: "Live",
  remix: "Remixes",
  soundtrack: "Soundtracks",
  other: "Other",
};

function albumTypeKey(albumType: string | null | undefined): string {
  const key = (albumType ?? "album").toLowerCase();
  return ALBUM_TYPE_ORDER.includes(key as (typeof ALBUM_TYPE_ORDER)[number]) ? key : "other";
}

function albumTypeRank(albumType: string | null | undefined): number {
  const idx = ALBUM_TYPE_ORDER.indexOf(
    albumTypeKey(albumType) as (typeof ALBUM_TYPE_ORDER)[number],
  );
  return idx === -1 ? 999 : idx;
}

// ── Page ───────────────────────────────────────────────────────

function AlbumsPage() {
  const [search, setSearch] = useState("");
  const [sort, setSort] = useLocalStorage<AlbumSort>("albums-sort", "newest");

  const { data: albums, isLoading, isError } = $api.useQuery("get", "/api/album");

  /** Resolve the display name for an album's primary artist. */
  const artistName = (album: AlbumListItem): string => getLibraryAlbumArtistName(album);

  /** Apply search filter then sort within each group. */
  const sortList = (list: AlbumListItem[]): AlbumListItem[] => {
    return sortLibraryAlbums(list, sort);
  };

  /** Grouped albums: array of { type, label, albums } ordered by type rank. */
  const groups = useMemo(() => {
    if (!albums) return [];

    const list = filterLibraryAlbums(albums, search);

    // Bucket by album type
    const buckets = new Map<string, AlbumListItem[]>();
    for (const album of list) {
      const key = albumTypeKey(album.album_type);
      const bucket = buckets.get(key);
      if (bucket) {
        bucket.push(album);
      } else {
        buckets.set(key, [album]);
      }
    }

    // Sort the groups by canonical type order, then sort albums within each
    return [...buckets.entries()]
      .sort(([a], [b]) => albumTypeRank(a) - albumTypeRank(b))
      .map(([type, items]) => ({
        type,
        label: ALBUM_TYPE_LABELS[type] ?? type,
        albums: sortList(items),
      }));
  }, [albums, search, sort]);

  const totalFiltered = groups.reduce((n, g) => n + g.albums.length, 0);

  if (isLoading) {
    return (
      <div className="space-y-6">
        <div>
          <h1 className="text-2xl font-bold tracking-tight">Albums</h1>
          <Skeleton className="mt-1 h-4 w-48" />
        </div>
        <Skeleton className="h-9 w-full max-w-sm" />
        <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4">
          {Array.from({ length: 8 }).map((_, i) => (
            <div key={i} className="overflow-hidden rounded-xl border bg-card shadow-sm">
              <Skeleton className="aspect-square w-full" />
              <div className="p-4">
                <Skeleton className="mb-2 h-4 w-28" />
                <Skeleton className="mb-1 h-3 w-36" />
                <Skeleton className="h-3 w-20" />
              </div>
            </div>
          ))}
        </div>
      </div>
    );
  }

  if (isError || !albums) {
    return (
      <div className="space-y-6">
        <div>
          <h1 className="text-2xl font-bold tracking-tight">Albums</h1>
        </div>
        <div className="rounded-xl border border-red-200 bg-red-50 p-4 text-sm text-red-700 dark:border-red-900/50 dark:bg-red-950/50 dark:text-red-400">
          Failed to load albums.
        </div>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-bold tracking-tight">Albums</h1>
        <p className="text-muted-foreground">
          {albums.length} album{albums.length !== 1 ? "s" : ""} in your library.
        </p>
      </div>

      {albums.length === 0 ? (
        <div className="flex flex-col items-center justify-center rounded-xl border border-dashed bg-muted/30 py-20">
          <p className="text-sm text-muted-foreground">
            No albums yet. Add some from the search page or sync an artist.
          </p>
        </div>
      ) : (
        <>
          {/* ── Search & sort toolbar ──────────────────────── */}
          <div className="flex flex-wrap items-center gap-3">
            <div className="relative max-w-sm flex-1">
              <SearchIcon className="pointer-events-none absolute top-1/2 left-3 size-4 -translate-y-1/2 text-muted-foreground" />
              <Input
                value={search}
                onChange={(e) => setSearch(e.target.value)}
                placeholder="Search albums or artists..."
                className="pl-9"
              />
            </div>
            <Select value={sort} onValueChange={setSort}>
              <SelectTrigger className="h-9 w-36 text-xs">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="az">A &ndash; Z</SelectItem>
                <SelectItem value="za">Z &ndash; A</SelectItem>
                <SelectItem value="artist">By Artist</SelectItem>
                <SelectItem value="newest">Newest Release</SelectItem>
                <SelectItem value="oldest">Oldest Release</SelectItem>
                <SelectItem value="added">Recently Added</SelectItem>
              </SelectContent>
            </Select>
          </div>

          {totalFiltered === 0 ? (
            <div className="flex flex-col items-center justify-center rounded-xl border border-dashed bg-muted/30 py-12">
              <p className="text-sm text-muted-foreground">
                No albums match &ldquo;{search}&rdquo;
              </p>
            </div>
          ) : (
            <div className="space-y-8">
              {groups.map((group) => (
                <AlbumGroup
                  key={group.type}
                  label={group.label}
                  albums={group.albums}
                  artistNameFn={artistName}
                  sort={sort}
                />
              ))}
            </div>
          )}
        </>
      )}
    </div>
  );
}

// ── Album group (type section) ─────────────────────────────────

function AlbumGroup({
  label,
  albums,
  artistNameFn,
  sort,
}: {
  label: string;
  albums: AlbumListItem[];
  artistNameFn: (album: AlbumListItem) => string;
  sort: string;
}) {
  const gridRef = useSleeveGlow([albums.length, sort]);

  return (
    <section>
      <div className="mb-3 flex items-center gap-2">
        <h2 className="text-lg font-semibold">{label}</h2>
        <span className="text-sm text-muted-foreground">{albums.length}</span>
      </div>

      <div ref={gridRef} className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4">
        {albums.map((album) => {
          const name = artistNameFn(album);
          const cardBody = (
            <>
              <div className="relative aspect-square bg-muted">
                {album.cover_url ? (
                  <img
                    src={album.cover_url}
                    alt={album.title}
                    className="sleeve-cover"
                    crossOrigin="anonymous"
                  />
                ) : (
                  <div className="flex size-full items-center justify-center text-4xl font-bold text-muted-foreground/30">
                    {album.title.charAt(0)}
                  </div>
                )}
              </div>
              <div className="p-4">
                <p className="truncate font-semibold">{album.title}</p>
                <p className="truncate text-xs text-muted-foreground">
                  {name} &middot; {album.release_date?.slice(0, 4) ?? "Unknown"}
                </p>
                <div className="mt-2 flex flex-wrap items-center gap-1.5">
                  {isAlbumAcquired(album.wanted_status) && (
                    <span className="rounded-full bg-green-500/10 px-2 py-0.5 text-xs font-medium text-green-500">
                      Acquired
                    </span>
                  )}
                  {isAlbumWanted(album.wanted_status) && (
                    <span className="rounded-full bg-amber-500/10 px-2 py-0.5 text-xs font-medium text-amber-500">
                      Wanted
                    </span>
                  )}
                  {isAlbumInProgress(album.wanted_status) && (
                    <span className="rounded-full bg-blue-500/10 px-2 py-0.5 text-xs font-medium text-blue-500">
                      In Progress
                    </span>
                  )}
                </div>
              </div>
            </>
          );

          return (
            <div key={album.id} className="sleeve group">
              {canLinkLibraryAlbum(album) ? (
                <Link
                  to="/artists/$artistId/albums/$albumId"
                  params={{
                    artistId: album.artist_id!,
                    albumId: album.id,
                  }}
                  className="block"
                >
                  {cardBody}
                </Link>
              ) : (
                <div>{cardBody}</div>
              )}
            </div>
          );
        })}
      </div>
    </section>
  );
}
