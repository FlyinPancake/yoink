import { Navigate, createFileRoute } from "@tanstack/react-router";

export const Route = createFileRoute("/_app/library/")({
  component: () => <Navigate to="/library/artists" />,
});
