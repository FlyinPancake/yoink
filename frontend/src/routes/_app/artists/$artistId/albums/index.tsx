import { Navigate, createFileRoute } from "@tanstack/react-router";

export const Route = createFileRoute("/_app/artists/$artistId/albums/")({
  component: ArtistAlbumsIndexRedirect,
});

function ArtistAlbumsIndexRedirect() {
  const { artistId } = Route.useParams();
  return <Navigate to="/artists/$artistId" params={{ artistId }} />;
}
