import type { AnyRouteMatch } from "@tanstack/react-router";
import { describe, expect, it } from "vitest";
import { buildBreadcrumbs, resolveBreadcrumbLabel } from "@/lib/router/breadcrumbs";

function createMatch(overrides: Partial<AnyRouteMatch> = {}): AnyRouteMatch {
  return {
    id: overrides.id ?? "match-id",
    pathname: overrides.pathname ?? "/example",
    staticData: overrides.staticData ?? {},
    loaderData: overrides.loaderData,
  } as AnyRouteMatch;
}

describe("resolveBreadcrumbLabel", () => {
  it("returns static breadcrumb labels", () => {
    const match = createMatch({
      staticData: { breadcrumb: "Library" },
    });

    expect(resolveBreadcrumbLabel(match)).toBe("Library");
  });

  it("returns resolver breadcrumb labels from loader data", () => {
    const match = createMatch({
      loaderData: { artist: { name: "Radiohead" } },
      staticData: {
        breadcrumb: (currentMatch) =>
          (currentMatch.loaderData as { artist?: { name?: string } })?.artist?.name,
      },
    });

    expect(resolveBreadcrumbLabel(match)).toBe("Radiohead");
  });

  it("hides null, undefined, and blank breadcrumbs", () => {
    const nullMatch = createMatch({
      staticData: { breadcrumb: () => undefined },
    });
    const blankMatch = createMatch({
      staticData: { breadcrumb: "   " },
    });

    expect(resolveBreadcrumbLabel(nullMatch)).toBeNull();
    expect(resolveBreadcrumbLabel(blankMatch)).toBeNull();
  });
});

describe("buildBreadcrumbs", () => {
  it("returns visible breadcrumbs in route order", () => {
    const matches = [
      createMatch({
        id: "library",
        pathname: "/library",
        staticData: { breadcrumb: "Library" },
      }),
      createMatch({
        id: "artists",
        pathname: "/library/artists",
        staticData: { breadcrumb: "Artists" },
      }),
    ];

    expect(buildBreadcrumbs(matches)).toEqual([
      { id: "library", pathname: "/library", label: "Library" },
      {
        id: "artists",
        pathname: "/library/artists",
        label: "Artists",
      },
    ]);
  });

  it("filters hidden intermediate routes from compact album trails", () => {
    const matches = [
      createMatch({
        id: "artists-root",
        pathname: "/artists",
        staticData: { breadcrumb: "Artists" },
      }),
      createMatch({
        id: "artist",
        pathname: "/artists/artist-1",
        loaderData: { artist: { name: "Radiohead" } },
        staticData: {
          breadcrumb: (match) => (match.loaderData as { artist?: { name?: string } })?.artist?.name,
        },
      }),
      createMatch({
        id: "albums-parent",
        pathname: "/artists/artist-1/albums",
        staticData: { breadcrumb: undefined },
      }),
      createMatch({
        id: "album",
        pathname: "/artists/artist-1/albums/album-1",
        loaderData: { album: { title: "OK Computer" } },
        staticData: {
          breadcrumb: (match) => (match.loaderData as { album?: { title?: string } })?.album?.title,
        },
      }),
    ];

    expect(buildBreadcrumbs(matches)).toEqual([
      { id: "artists-root", pathname: "/artists", label: "Artists" },
      {
        id: "artist",
        pathname: "/artists/artist-1",
        label: "Radiohead",
      },
      {
        id: "album",
        pathname: "/artists/artist-1/albums/album-1",
        label: "OK Computer",
      },
    ]);
  });
});
