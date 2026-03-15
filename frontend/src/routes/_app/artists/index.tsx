import { Navigate, createFileRoute } from "@tanstack/react-router";

export const Route = createFileRoute("/_app/artists/")({
  component: ArtistsIndexRedirect,
  staticData: {
    breadcrumb: "Artists",
  },
});

function ArtistsIndexRedirect() {
  return <Navigate to="/library/artists" />;
}
