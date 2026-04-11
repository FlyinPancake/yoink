import { HeadContent, Outlet, Scripts, createRootRouteWithContext } from "@tanstack/react-router";
import { TanStackRouterDevtoolsPanel } from "@tanstack/react-router-devtools";
import { ReactQueryDevtoolsPanel } from "@tanstack/react-query-devtools";
import { TanStackDevtools } from "@tanstack/react-devtools";
import { QueryClientProvider } from "@tanstack/react-query";
import type { QueryClient } from "@tanstack/react-query";

import appCss from "../styles.css?url";
import { TooltipProvider } from "@/components/ui/tooltip";
import type { components } from "@/lib/api";

type AuthStatus = components["schemas"]["AuthStatus"];

export interface RouterContext {
  queryClient: QueryClient;
  authStatus?: AuthStatus;
}

export const Route = createRootRouteWithContext<RouterContext>()({
  head: () => ({
    meta: [
      {
        charSet: "utf-8",
      },
      {
        name: "viewport",
        content: "width=device-width, initial-scale=1",
      },
      {
        title: "yoink",
      },
    ],
    links: [
      { rel: "icon", type: "image/svg+xml", href: "/yoink.svg" },
      { rel: "stylesheet", href: appCss },
    ],
  }),
  component: RootComponent,
});

function RootComponent() {
  const { queryClient } = Route.useRouteContext();
  return (
    <>
      <HeadContent />
      <QueryClientProvider client={queryClient}>
        <TooltipProvider>
          <Outlet />
        </TooltipProvider>
        <TanStackDevtools
          config={{
            position: "bottom-right",
          }}
          plugins={[
            {
              name: "Tanstack Router",
              render: <TanStackRouterDevtoolsPanel />,
            },
            {
              name: "Tanstack Query",
              render: <ReactQueryDevtoolsPanel />,
            },
          ]}
        />
      </QueryClientProvider>
      <Scripts />
    </>
  );
}
