"use client";

import * as React from "react";

import { NavMain } from "@/components/nav-main";
import { NavSecondary } from "@/components/nav-secondary";
import { NavUser } from "@/components/nav-user";
import { Sidebar, SidebarContent, SidebarFooter, SidebarHeader } from "@/components/ui/sidebar";
import {
  DownloadIcon,
  FolderInputIcon,
  HeartIcon,
  LayoutDashboardIcon,
  LibraryIcon,
  SearchIcon,
  SettingsIcon,
} from "lucide-react";
import { ThemeSelector } from "./theme-selector";
import { $api } from "@/lib/api";

const navMain = [
  {
    title: "Dashboard",
    url: "/",
    icon: <LayoutDashboardIcon />,
  },
  {
    title: "Library",
    url: "/library",
    icon: <LibraryIcon />,
    isActive: true,
    items: [
      { title: "Artists", url: "/library/artists" },
      { title: "Albums", url: "/library/albums" },
      { title: "Tracks", url: "/library/tracks" },
    ],
  },
  {
    title: "Search",
    url: "/search",
    icon: <SearchIcon />,
  },
  {
    title: "Wanted",
    url: "/wanted",
    icon: <HeartIcon />,
  },
  {
    title: "Downloads",
    url: "/downloads",
    icon: <DownloadIcon />,
  },
  {
    title: "Import",
    url: "/import",
    icon: <FolderInputIcon />,
  },
];

const navSecondary = [
  {
    title: "Settings",
    url: "/settings",
    icon: <SettingsIcon />,
  },
];

export function AppSidebar({ ...props }: React.ComponentProps<typeof Sidebar>) {
  const { data: authStatus } = $api.useQuery("get", "/api/auth/status");

  return (
    <Sidebar variant="inset" {...props}>
      <SidebarHeader className="flex flex-row items-center justify-between gap-2">
        <div className="flex items-center gap-2">
          <div className="flex aspect-square size-8 items-center justify-center rounded-lg bg-sidebar-primary text-sidebar-primary-foreground">
            <img src="/yoink.svg" alt="yoink icon" />
          </div>
          <div className="grid flex-1 text-left text-sm leading-tight">
            <span className="truncate font-medium">yoink</span>
          </div>
        </div>
        <ThemeSelector />
      </SidebarHeader>
      <SidebarContent>
        <NavMain items={navMain} />
        <NavSecondary items={navSecondary} className="mt-auto" />
      </SidebarContent>
      {authStatus?.auth_enabled && (
        <SidebarFooter>
          <NavUser username={authStatus.username ?? ""} />
        </SidebarFooter>
      )}
    </Sidebar>
  );
}
