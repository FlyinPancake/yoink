import { Outlet, createFileRoute } from "@tanstack/react-router";

export const Route = createFileRoute("/_app/artists/$artistId/albums")({
  component: ArtistAlbumsLayout,
});

function ArtistAlbumsLayout() {
  return <Outlet />;
}
