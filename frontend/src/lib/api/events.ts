/**
 * SSE event source that listens to /api/events and invalidates
 * relevant TanStack Query caches when the server broadcasts a refresh.
 */

import type { QueryClient } from "@tanstack/react-query";

let eventSource: EventSource | null = null;

/**
 * Connect to the server-sent events stream.
 * Automatically reconnects on error.
 * Call once (e.g. in the root layout effect).
 */
export function connectSSE(queryClient: QueryClient): () => void {
  if (typeof window === "undefined") return () => {};

  function connect() {
    eventSource = new EventSource("/api/events");

    eventSource.addEventListener("update", (event) => {
      if (event.data === "refresh") {
        // Invalidate all API queries so the UI picks up server-side changes.
        // This is intentionally broad — TanStack Query will only refetch
        // queries that are actively mounted.
        void queryClient.invalidateQueries({ queryKey: ["get"] });
      }
    });

    eventSource.onerror = () => {
      eventSource?.close();
      // Reconnect after a delay
      setTimeout(connect, 5000);
    };
  }

  connect();

  return () => {
    eventSource?.close();
    eventSource = null;
  };
}
