import { Outlet, createFileRoute } from "@tanstack/react-router";

export const Route = createFileRoute("/_app/settings")({
  component: SettingsLayout,
  staticData: {
    breadcrumb: "Settings",
  },
});

function SettingsLayout() {
  return (
    <div className="space-y-8">
      <div>
        <h1 className="text-2xl font-bold tracking-tight">Settings</h1>
        <p className="text-muted-foreground">Manage your yoink instance configuration.</p>
      </div>
      <Outlet />
    </div>
  );
}
