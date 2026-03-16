import { Outlet, createFileRoute } from "@tanstack/react-router";
import { queryKeys } from "@/lib/api";

export const Route = createFileRoute("/_app/artists/$artistId")({
  component: ArtistLayout,
  loader: async ({ context, params }) =>
    context.queryClient.ensureQueryData(queryKeys.artists.detail(params.artistId)),
  staticData: {
    breadcrumb: (match) =>
      (match.loaderData as { artist?: { name?: string } } | undefined)?.artist?.name ?? "Artist",
  },
});

function ArtistLayout() {
  return <Outlet />;
}
