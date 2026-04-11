import { createFileRoute } from "@tanstack/react-router";
import { Landing } from "@/landing";

export const Route = createFileRoute("/")({
  component: Landing,
});
