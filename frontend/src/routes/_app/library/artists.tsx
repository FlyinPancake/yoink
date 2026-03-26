import { useMemo, useState } from "react";
import { createFileRoute, Link } from "@tanstack/react-router";
import { SearchIcon } from "lucide-react";
import { $api } from "@/lib/api";
import { useLocalStorage } from "@/hooks/use-local-storage";
import { Badge } from "@/components/ui/badge";
import { Input } from "@/components/ui/input";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Skeleton } from "@/components/ui/skeleton";

export const Route = createFileRoute("/_app/library/artists")({
  component: ArtistsPage,
  staticData: {
    breadcrumb: "Artists",
  },
});

function fallbackInitial(name: string): string {
  return name.charAt(0).toUpperCase() || "?";
}

function ArtistsPage() {
  const [search, setSearch] = useState("");
  const [sort, setSort] = useLocalStorage<"az" | "newest" | "oldest" | "za">("artists-sort", "az");

  const { data: artists, isLoading, isError } = $api.useQuery("get", "/api/artist");

  const filtered = useMemo(() => {
    if (!artists) return [];
    const q = search.trim().toLowerCase();
    let list = q
      ? artists.filter(
          (a) => a.name.toLowerCase().includes(q) || (a.bio && a.bio.toLowerCase().includes(q)),
        )
      : [...artists];

    switch (sort) {
      case "az":
        list.sort((a, b) => a.name.toLowerCase().localeCompare(b.name.toLowerCase()));
        break;
      case "za":
        list.sort((a, b) => b.name.toLowerCase().localeCompare(a.name.toLowerCase()));
        break;
      case "newest":
        list.sort((a, b) => new Date(b.created_at).getTime() - new Date(a.created_at).getTime());
        break;
      case "oldest":
        list.sort((a, b) => new Date(a.created_at).getTime() - new Date(b.created_at).getTime());
        break;
    }
    return list;
  }, [artists, search, sort]);

  if (isLoading) {
    return (
      <div className="space-y-6">
        <div>
          <h1 className="text-2xl font-bold tracking-tight">Artists</h1>
          <Skeleton className="mt-1 h-4 w-48" />
        </div>
        <Skeleton className="h-9 w-full max-w-sm" />
        <div className="grid gap-4 sm:grid-cols-1 lg:grid-cols-2">
          {Array.from({ length: 6 }).map((_, i) => (
            <div key={i} className="overflow-hidden rounded-xl border bg-card p-5">
              <div className="flex animate-pulse items-center gap-5">
                <Skeleton className="size-16 shrink-0 rounded-full" />
                <div className="min-w-0 flex-1">
                  <Skeleton className="mb-2 h-5 w-36" />
                  <Skeleton className="mb-2 h-3.5 w-52" />
                  <Skeleton className="h-5 w-20 rounded-full" />
                </div>
              </div>
            </div>
          ))}
        </div>
      </div>
    );
  }

  if (isError || !artists) {
    return (
      <div className="space-y-6">
        <div>
          <h1 className="text-2xl font-bold tracking-tight">Artists</h1>
        </div>
        <div className="rounded-xl border border-red-200 bg-red-50 p-4 text-sm text-red-700 dark:border-red-900/50 dark:bg-red-950/50 dark:text-red-400">
          Failed to load artists.
        </div>
      </div>
    );
  }

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-bold tracking-tight">Artists</h1>
        <p className="text-muted-foreground">
          {artists.length} artist{artists.length !== 1 ? "s" : ""} in your library.
        </p>
      </div>

      {artists.length === 0 ? (
        <div className="flex flex-col items-center justify-center rounded-xl border border-dashed bg-muted/30 py-20">
          <p className="text-sm text-muted-foreground">
            No artists yet. Add some from the search page.
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
                placeholder="Search artists..."
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
                <SelectItem value="newest">Recently Added</SelectItem>
                <SelectItem value="oldest">Oldest Added</SelectItem>
              </SelectContent>
            </Select>
          </div>

          {filtered.length === 0 ? (
            <div className="flex flex-col items-center justify-center rounded-xl border border-dashed bg-muted/30 py-12">
              <p className="text-sm text-muted-foreground">
                No artists match &ldquo;{search}&rdquo;
              </p>
            </div>
          ) : (
            <div className="grid gap-4 sm:grid-cols-1 lg:grid-cols-2">
              {filtered.map((artist) => (
                <Link
                  key={artist.id}
                  to="/artists/$artistId"
                  params={{ artistId: artist.id }}
                  className="group flex items-center gap-5 rounded-xl border bg-card p-5 transition-shadow hover:shadow-md"
                >
                  {/* Circular avatar */}
                  {artist.image_url ? (
                    <img
                      className="size-16 shrink-0 rounded-full border-2 border-blue-500/20 bg-muted object-cover dark:border-blue-500/30"
                      src={artist.image_url}
                      alt=""
                    />
                  ) : (
                    <div className="inline-flex size-16 shrink-0 items-center justify-center rounded-full border-2 border-blue-500/20 bg-muted text-[26px] font-bold text-muted-foreground dark:border-blue-500/30">
                      {fallbackInitial(artist.name)}
                    </div>
                  )}

                  {/* Info block */}
                  <div className="min-w-0 flex-1">
                    <div className="mb-1 flex flex-wrap items-center gap-2">
                      <span className="truncate text-lg font-bold transition-colors group-hover:text-blue-500">
                        {artist.name}
                      </span>
                      {artist.monitored ? (
                        <Badge variant="outline" className="border-blue-500/30 text-blue-500">
                          Monitored
                        </Badge>
                      ) : (
                        <Badge variant="outline" className="border-amber-500/30 text-amber-500">
                          Lightweight
                        </Badge>
                      )}
                    </div>

                    {artist.bio ? (
                      <p className="line-clamp-1 text-[13px] leading-relaxed text-muted-foreground">
                        {artist.bio}
                      </p>
                    ) : (
                      <p className="text-[13px] text-muted-foreground/50">No bio available</p>
                    )}

                    <div className="mt-1.5 text-[12px] text-muted-foreground/60">
                      Added{" "}
                      {new Date(artist.created_at).toLocaleDateString(undefined, {
                        year: "numeric",
                        month: "short",
                        day: "numeric",
                      })}
                    </div>
                  </div>
                </Link>
              ))}
            </div>
          )}
        </>
      )}
    </div>
  );
}
