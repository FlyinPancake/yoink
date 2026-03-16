import { AlertCircleIcon, CheckCircle2Icon, PackageIcon, UserPlusIcon } from "lucide-react";
import type { components } from "@/lib/api/types.gen";

type ImportResultSummary = components["schemas"]["ImportResultSummary"];

export function ImportResultCard({ result }: { result: ImportResultSummary }) {
  const hasErrors = result.errors.length > 0;

  return (
    <div className="space-y-4">
      {/* Summary stats */}
      <div className="grid gap-3 sm:grid-cols-3">
        <div className="flex items-center gap-3 rounded-xl border bg-card p-4 shadow-sm">
          <PackageIcon className="size-5 shrink-0 text-blue-500" />
          <div>
            <p className="text-2xl font-bold tabular-nums">
              {result.imported}
              <span className="text-sm font-normal text-muted-foreground">
                /{result.total_selected}
              </span>
            </p>
            <p className="text-xs text-muted-foreground">Albums imported</p>
          </div>
        </div>

        <div className="flex items-center gap-3 rounded-xl border bg-card p-4 shadow-sm">
          <UserPlusIcon className="size-5 shrink-0 text-green-500" />
          <div>
            <p className="text-2xl font-bold tabular-nums">{result.artists_added}</p>
            <p className="text-xs text-muted-foreground">Artists added</p>
          </div>
        </div>

        <div className="flex items-center gap-3 rounded-xl border bg-card p-4 shadow-sm">
          {result.failed > 0 ? (
            <AlertCircleIcon className="size-5 shrink-0 text-red-500" />
          ) : (
            <CheckCircle2Icon className="size-5 shrink-0 text-green-500" />
          )}
          <div>
            <p className="text-2xl font-bold tabular-nums">{result.failed}</p>
            <p className="text-xs text-muted-foreground">Failed</p>
          </div>
        </div>
      </div>

      {/* Success banner */}
      {!hasErrors && result.imported > 0 && (
        <div className="flex items-center gap-2 rounded-xl border border-green-200 bg-green-50 px-4 py-3 text-sm text-green-700 dark:border-green-900/50 dark:bg-green-950/50 dark:text-green-400">
          <CheckCircle2Icon className="size-4 shrink-0" />
          Import completed successfully.
        </div>
      )}

      {/* Error list */}
      {hasErrors && (
        <div className="space-y-2">
          <div className="flex items-center gap-2 rounded-xl border border-red-200 bg-red-50 px-4 py-3 text-sm text-red-700 dark:border-red-900/50 dark:bg-red-950/50 dark:text-red-400">
            <AlertCircleIcon className="size-4 shrink-0" />
            {result.errors.length} error{result.errors.length > 1 ? "s" : ""} occurred during
            import.
          </div>
          <div className="max-h-48 overflow-y-auto rounded-xl border bg-muted/30 p-3">
            <ul className="space-y-1 text-xs text-muted-foreground">
              {result.errors.map((err, i) => (
                <li key={i} className="flex items-start gap-1.5">
                  <span className="mt-0.5 size-1.5 shrink-0 rounded-full bg-red-400" />
                  {err}
                </li>
              ))}
            </ul>
          </div>
        </div>
      )}
    </div>
  );
}
