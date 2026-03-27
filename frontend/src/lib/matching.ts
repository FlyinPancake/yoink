import type { components } from "@/lib/api/types.gen";

type MatchStatus = components["schemas"]["MatchStatus"];

export function isPendingMatchSuggestion(status: MatchStatus): boolean {
  return status === "pending";
}
