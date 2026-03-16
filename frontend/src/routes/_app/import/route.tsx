import { Outlet, createFileRoute } from "@tanstack/react-router";

export const Route = createFileRoute("/_app/import")({
  component: ImportLayout,
  staticData: {
    breadcrumb: "Import",
  },
});

function ImportLayout() {
  return <Outlet />;
}
