import { createFileRoute } from "@tanstack/react-router";

export const Route = createFileRoute("/_app/settings")({
  component: SettingsPage,
});

function SettingsPage() {
  return (
    <div className="space-y-8">
      <div>
        <h1 className="text-2xl font-bold tracking-tight">Settings</h1>
        <p className="text-muted-foreground">
          Manage your yoink instance configuration.
        </p>
      </div>

      <div className="grid gap-6 max-w-2xl">
        {/* General */}
        <section className="rounded-xl border bg-card shadow-sm">
          <div className="border-b px-5 py-4">
            <h2 className="font-semibold">General</h2>
            <p className="text-xs text-muted-foreground">
              Core application settings.
            </p>
          </div>
          <div className="divide-y">
            <SettingRow
              label="Music directory"
              value="/data/music"
              description="Root path where acquired files are stored."
            />
            <SettingRow
              label="Default quality"
              value="Lossless"
              description="Preferred download quality when no override is set."
            />
            <SettingRow
              label="Naming template"
              value="{artist}/{album}/{track_number} - {title}"
              description="File naming pattern for acquired tracks."
            />
          </div>
        </section>

        {/* Providers */}
        <section className="rounded-xl border bg-card shadow-sm">
          <div className="border-b px-5 py-4">
            <h2 className="font-semibold">Providers</h2>
            <p className="text-xs text-muted-foreground">
              Metadata and download source configuration.
            </p>
          </div>
          <div className="divide-y">
            <SettingRow
              label="Tidal"
              value="Connected"
              description="Metadata + lossless downloads."
              status="ok"
            />
            <SettingRow
              label="Deezer"
              value="Connected"
              description="Fallback metadata source."
              status="ok"
            />
            <SettingRow
              label="MusicBrainz"
              value="Connected"
              description="Open metadata database."
              status="ok"
            />
            <SettingRow
              label="Soulseek"
              value="Not configured"
              description="Peer-to-peer file sharing network."
              status="warn"
            />
          </div>
        </section>

        {/* Security */}
        <section className="rounded-xl border bg-card shadow-sm">
          <div className="border-b px-5 py-4">
            <h2 className="font-semibold">Security</h2>
            <p className="text-xs text-muted-foreground">
              Authentication and access control.
            </p>
          </div>
          <div className="divide-y">
            <SettingRow
              label="Authentication"
              value="Enabled"
              description="Require login to access the application."
              status="ok"
            />
            <SettingRow
              label="API key"
              value="yoink_ak_...x7f2"
              description="Used for external integrations."
            />
          </div>
        </section>
      </div>
    </div>
  );
}

function SettingRow({
  label,
  value,
  description,
  status,
}: {
  label: string;
  value: string;
  description: string;
  status?: "ok" | "warn";
}) {
  return (
    <div className="flex items-center justify-between gap-4 px-5 py-3.5">
      <div className="min-w-0">
        <p className="text-sm font-medium">{label}</p>
        <p className="text-xs text-muted-foreground">{description}</p>
      </div>
      <div className="flex items-center gap-2 shrink-0">
        {status === "ok" && (
          <span className="size-2 rounded-full bg-green-500" />
        )}
        {status === "warn" && (
          <span className="size-2 rounded-full bg-amber-500" />
        )}
        <span className="text-sm text-muted-foreground">{value}</span>
      </div>
    </div>
  );
}
