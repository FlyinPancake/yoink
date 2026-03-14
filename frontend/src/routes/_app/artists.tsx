import { Outlet, createFileRoute } from "@tanstack/react-router";

export const Route = createFileRoute("/_app/artists")({
  component: ArtistsLayout,
  staticData: {
    breadcrumb: "Artists",
  },
});

function ArtistsLayout() {
  return <Outlet />;
}
