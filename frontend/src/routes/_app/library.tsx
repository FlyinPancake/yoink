import { Outlet, createFileRoute } from "@tanstack/react-router";

export const Route = createFileRoute("/_app/library")({
  component: LibraryLayout,
  staticData: {
    breadcrumb: "Library",
  },
});

function LibraryLayout() {
  return <Outlet />;
}
