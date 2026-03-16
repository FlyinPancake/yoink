import { useEffect } from "react";
import { Outlet, createFileRoute, redirect } from "@tanstack/react-router";
import { useQueryClient } from "@tanstack/react-query";
import { Breadcrumbs } from "@/components/app-breadcrumbs";
import { AppSidebar } from "@/components/app-sidebar";
import { SidebarInset, SidebarProvider, SidebarTrigger } from "@/components/ui/sidebar";
import { connectSSE, getCollections } from "@/lib/api";
import { fetchClient } from "@/lib/api";

export const Route = createFileRoute("/_app")({
  beforeLoad: async ({ location }) => {
    const { data, error } = await fetchClient.GET("/api/auth/status");

    if (error || !data) {
      throw redirect({
        to: "/login",
        search: { next: location.pathname },
      });
    }

    if (data.auth_enabled && !data.authenticated) {
      throw redirect({
        to: "/login",
        search: { next: location.pathname },
      });
    }

    if (data.auth_enabled && data.authenticated && data.must_change_password) {
      throw redirect({ to: "/setup/password" });
    }

    return { authStatus: data };
  },
  component: AppLayout,
});

function AppLayout() {
  const queryClient = useQueryClient();

  // Initialise TanStack DB collections (idempotent singleton).
  getCollections(queryClient);

  useEffect(() => {
    return connectSSE(queryClient);
  }, [queryClient]);

  return (
    <SidebarProvider>
      <AppSidebar />
      <SidebarInset className="overflow-hidden">
        <header className="sticky top-0 z-20 flex h-12 shrink-0 items-center gap-2 border-b bg-background/95 px-4 backdrop-blur supports-[backdrop-filter]:bg-background/80">
          <SidebarTrigger className="-ml-1" />
          <div className="min-w-0 flex-1">
            <Breadcrumbs />
          </div>
        </header>
        <main className="flex-1 overflow-auto p-6">
          <Outlet />
        </main>
      </SidebarInset>
    </SidebarProvider>
  );
}
