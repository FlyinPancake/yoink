import { createFileRoute } from "@tanstack/react-router";
import { SearchIcon } from "lucide-react";

export const Route = createFileRoute("/_app/search")({
  component: SearchPage,
});

function SearchPage() {
  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-bold tracking-tight">Search</h1>
        <p className="text-muted-foreground">
          Find artists and albums from external providers.
        </p>
      </div>

      <div className="relative max-w-xl">
        <SearchIcon className="absolute left-3 top-1/2 size-4 -translate-y-1/2 text-muted-foreground" />
        <input
          type="text"
          placeholder="Search for artists, albums..."
          className="w-full rounded-lg border bg-background py-2.5 pl-10 pr-4 text-sm outline-none ring-ring transition-shadow placeholder:text-muted-foreground focus:ring-2"
        />
      </div>

      <div className="flex flex-col items-center justify-center rounded-xl border border-dashed bg-muted/30 py-20">
        <SearchIcon className="size-10 text-muted-foreground/40" />
        <p className="mt-4 text-sm text-muted-foreground">
          Type a query to search across your configured providers.
        </p>
      </div>
    </div>
  );
}
