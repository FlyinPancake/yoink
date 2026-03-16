import { createFileRoute, useSearch } from "@tanstack/react-router";
import { useForm } from "@tanstack/react-form";
import { $api } from "@/lib/api";
import { Label } from "@/components/ui/label";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import { AlertCircleIcon, CheckCircleIcon } from "lucide-react";
import { submitForm } from "@/lib/utils";

interface SecuritySearch {
  success?: string;
  error?: string;
}

export const Route = createFileRoute("/_app/settings/security")({
  validateSearch: (search: Record<string, unknown>): SecuritySearch => ({
    success: typeof search.success === "string" ? search.success : undefined,
    error: typeof search.error === "string" ? search.error : undefined,
  }),
  component: SecurityPage,
  staticData: {
    breadcrumb: "Security",
  },
});

function SecurityPage() {
  const { success, error } = useSearch({ from: "/_app/settings/security" });
  const { data: authStatus, isLoading } = $api.useQuery("get", "/api/auth/status");

  if (isLoading) {
    return (
      <div className="max-w-lg space-y-4">
        <Skeleton className="h-8 w-48" />
        <Skeleton className="h-64 w-full" />
      </div>
    );
  }

  if (!authStatus?.auth_enabled) {
    return (
      <div className="max-w-lg rounded-xl border bg-card px-5 py-8 text-center text-sm text-muted-foreground shadow-sm">
        Authentication is disabled. Enable it via the{" "}
        <code className="rounded bg-muted px-1.5 py-0.5 text-xs">AUTH_DISABLED</code> environment
        variable to manage credentials.
      </div>
    );
  }

  return <SecurityForm username={authStatus.username ?? ""} success={success} error={error} />;
}

function SecurityForm({
  username: currentUsername,
  success,
  error,
}: {
  username: string;
  success?: string;
  error?: string;
}) {
  const form = useForm({
    defaultValues: {
      username: currentUsername,
      current_password: "",
      new_password: "",
      confirm_password: "",
    },
    onSubmit: ({ value }) => {
      submitForm("/auth/update-credentials", value);
    },
    validators: {
      onSubmit: ({ value }) => {
        const errors: string[] = [];
        if (!value.username.trim()) {
          errors.push("Username is required");
        }
        if (!value.current_password) {
          errors.push("Current password is required");
        }
        if (!value.new_password) {
          errors.push("New password is required");
        } else if (value.new_password.length < 8) {
          errors.push("New password must be at least 8 characters");
        }
        if (value.new_password !== value.confirm_password) {
          errors.push("Passwords do not match");
        }
        return errors.length > 0 ? errors.join(", ") : undefined;
      },
    },
  });

  return (
    <div className="max-w-lg">
      <section className="rounded-xl border bg-card shadow-sm">
        <div className="border-b px-5 py-4">
          <h2 className="font-semibold">Update Credentials</h2>
          <p className="text-xs text-muted-foreground">Change your username or password.</p>
        </div>
        <div className="px-5 py-5">
          <form
            onSubmit={(e) => {
              e.preventDefault();
              e.stopPropagation();
              void form.handleSubmit();
            }}
            className="flex flex-col gap-4"
          >
            {success && (
              <div className="flex items-center gap-2 rounded-lg border border-green-200 bg-green-50 px-3 py-2.5 text-sm text-green-700 dark:border-green-900/50 dark:bg-green-950/50 dark:text-green-400">
                <CheckCircleIcon className="size-4 shrink-0" />
                Credentials updated successfully.
              </div>
            )}

            {error && (
              <div className="flex items-center gap-2 rounded-lg border border-red-200 bg-red-50 px-3 py-2.5 text-sm text-red-700 dark:border-red-900/50 dark:bg-red-950/50 dark:text-red-400">
                <AlertCircleIcon className="size-4 shrink-0" />
                {error}
              </div>
            )}

            <form.Field name="username">
              {(field) => (
                <div className="flex flex-col gap-1.5">
                  <Label htmlFor="sec-username">Username</Label>
                  <Input
                    id="sec-username"
                    type="text"
                    autoComplete="username"
                    required
                    value={field.state.value}
                    onChange={(e) => field.handleChange(e.target.value)}
                    onBlur={field.handleBlur}
                  />
                  {field.state.meta.errors.length > 0 && (
                    <p className="text-xs text-red-600 dark:text-red-400">
                      {field.state.meta.errors.join(", ")}
                    </p>
                  )}
                </div>
              )}
            </form.Field>

            <div className="my-1 border-t" />

            <form.Field name="current_password">
              {(field) => (
                <div className="flex flex-col gap-1.5">
                  <Label htmlFor="sec-current-password">Current Password</Label>
                  <Input
                    id="sec-current-password"
                    type="password"
                    autoComplete="current-password"
                    required
                    value={field.state.value}
                    onChange={(e) => field.handleChange(e.target.value)}
                    onBlur={field.handleBlur}
                  />
                  {field.state.meta.errors.length > 0 && (
                    <p className="text-xs text-red-600 dark:text-red-400">
                      {field.state.meta.errors.join(", ")}
                    </p>
                  )}
                </div>
              )}
            </form.Field>

            <form.Field name="new_password">
              {(field) => (
                <div className="flex flex-col gap-1.5">
                  <Label htmlFor="sec-new-password">New Password</Label>
                  <Input
                    id="sec-new-password"
                    type="password"
                    autoComplete="new-password"
                    required
                    value={field.state.value}
                    onChange={(e) => field.handleChange(e.target.value)}
                    onBlur={field.handleBlur}
                  />
                  {field.state.meta.errors.length > 0 && (
                    <p className="text-xs text-red-600 dark:text-red-400">
                      {field.state.meta.errors.join(", ")}
                    </p>
                  )}
                </div>
              )}
            </form.Field>

            <form.Field name="confirm_password">
              {(field) => (
                <div className="flex flex-col gap-1.5">
                  <Label htmlFor="sec-confirm-password">Confirm New Password</Label>
                  <Input
                    id="sec-confirm-password"
                    type="password"
                    autoComplete="new-password"
                    required
                    value={field.state.value}
                    onChange={(e) => field.handleChange(e.target.value)}
                    onBlur={field.handleBlur}
                  />
                  {field.state.meta.errors.length > 0 && (
                    <p className="text-xs text-red-600 dark:text-red-400">
                      {field.state.meta.errors.join(", ")}
                    </p>
                  )}
                </div>
              )}
            </form.Field>

            <form.Subscribe selector={(state) => state.errorMap}>
              {(errorMap) =>
                errorMap.onSubmit && (
                  <div className="flex items-center gap-2 rounded-lg border border-red-200 bg-red-50 px-3 py-2.5 text-sm text-red-700 dark:border-red-900/50 dark:bg-red-950/50 dark:text-red-400">
                    <AlertCircleIcon className="size-4 shrink-0" />
                    {errorMap.onSubmit}
                  </div>
                )
              }
            </form.Subscribe>

            <form.Subscribe selector={(state) => state.isSubmitting}>
              {(isSubmitting) => (
                <Button type="submit" className="w-full" disabled={isSubmitting}>
                  {isSubmitting ? "Updating..." : "Update Credentials"}
                </Button>
              )}
            </form.Subscribe>
          </form>
        </div>
      </section>
    </div>
  );
}
