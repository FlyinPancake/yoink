import { createFileRoute } from "@tanstack/react-router";
import { $api } from "@/lib/api";
import { Skeleton } from "@/components/ui/skeleton";

export const Route = createFileRoute("/_app/settings")({
  component: SettingsPage,
});

function SettingsPage() {
  const { data: providers, isLoading: providersLoading } = $api.useQuery(
    "get",
    "/api/provider",
  );
  const { data: authStatus, isLoading: authLoading } = $api.useQuery(
    "get",
    "/api/auth/status",
  );

  return (
    <div className="space-y-8">
      <div>
        <h1 className="text-2xl font-bold tracking-tight">Settings</h1>
        <p className="text-muted-foreground">
          Manage your yoink instance configuration.
        </p>
      </div>

      <div className="grid gap-6 max-w-2xl">
        {/* Providers */}
        <section className="rounded-xl border bg-card shadow-sm">
          <div className="border-b px-5 py-4">
            <h2 className="font-semibold">Providers</h2>
            <p className="text-xs text-muted-foreground">
              Metadata and download source configuration.
            </p>
          </div>
          {providersLoading ? (
            <div className="divide-y">
              {Array.from({ length: 3 }).map((_, i) => (
                <div key={i} className="flex items-center gap-4 px-5 py-3.5">
                  <Skeleton className="h-4 w-24" />
                  <Skeleton className="ml-auto h-4 w-16" />
                </div>
              ))}
            </div>
          ) : providers && providers.length > 0 ? (
            <div className="divide-y">
              {providers.map((provider) => (
                <SettingRow
                  key={provider}
                  label={providerDisplayName(provider)}
                  value="Connected"
                  description={providerDescription(provider)}
                  status="ok"
                />
              ))}
            </div>
          ) : (
            <div className="px-5 py-8 text-center text-sm text-muted-foreground">
              No providers configured.
            </div>
          )}
        </section>

        {/* Security */}
        <section className="rounded-xl border bg-card shadow-sm">
          <div className="border-b px-5 py-4">
            <h2 className="font-semibold">Security</h2>
            <p className="text-xs text-muted-foreground">
              Authentication and access control.
            </p>
          </div>
          {authLoading ? (
            <div className="divide-y">
              <div className="flex items-center gap-4 px-5 py-3.5">
                <Skeleton className="h-4 w-24" />
                <Skeleton className="ml-auto h-4 w-16" />
              </div>
            </div>
          ) : authStatus ? (
            <div className="divide-y">
              <SettingRow
                label="Authentication"
                value={authStatus.auth_enabled ? "Enabled" : "Disabled"}
                description="Require login to access the application."
                status={authStatus.auth_enabled ? "ok" : "warn"}
              />
              {authStatus.authenticated && authStatus.username && (
                <SettingRow
                  label="Logged in as"
                  value={authStatus.username}
                  description="Current authenticated user."
                />
              )}
            </div>
          ) : (
            <div className="px-5 py-8 text-center text-sm text-muted-foreground">
              Unable to load auth status.
            </div>
          )}
        </section>
      </div>
    </div>
  );
}

function providerDisplayName(provider: string): string {
  const map: Record<string, string> = {
    tidal: "Tidal",
    deezer: "Deezer",
    musicbrainz: "MusicBrainz",
    soulseek: "Soulseek",
    spotify: "Spotify",
    qobuz: "Qobuz",
    lastfm: "Last.fm",
  };
  return map[provider.toLowerCase()] ?? provider;
}

function providerDescription(provider: string): string {
  const map: Record<string, string> = {
    tidal: "Metadata + lossless downloads.",
    deezer: "Fallback metadata source.",
    musicbrainz: "Open metadata database.",
    soulseek: "Peer-to-peer file sharing network.",
    spotify: "Metadata from Spotify.",
    qobuz: "Hi-res lossless downloads.",
    lastfm: "Scrobbling and metadata.",
  };
  return map[provider.toLowerCase()] ?? "Metadata provider.";
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
