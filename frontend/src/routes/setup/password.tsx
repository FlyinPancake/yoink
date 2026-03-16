import { createFileRoute, redirect, useSearch } from "@tanstack/react-router";
import { useForm } from "@tanstack/react-form";
import { AlertCircleIcon } from "lucide-react";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Label } from "@/components/ui/label";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { fetchClient } from "@/lib/api";
import { submitForm } from "@/lib/utils";

interface SetupPasswordSearch {
  error?: string;
}

export const Route = createFileRoute("/setup/password")({
  validateSearch: (search: Record<string, unknown>): SetupPasswordSearch => ({
    error: typeof search.error === "string" ? search.error : undefined,
  }),
  beforeLoad: async () => {
    const { data } = await fetchClient.GET("/api/auth/status");

    // If auth is disabled or user is not authenticated, go home / login
    if (!data || !data.auth_enabled) {
      throw redirect({ to: "/" });
    }

    if (!data.authenticated) {
      throw redirect({ to: "/login" });
    }

    // If user doesn't need to change password, they shouldn't be here
    if (!data.must_change_password) {
      throw redirect({ to: "/" });
    }

    return { authStatus: data };
  },
  component: SetupPasswordPage,
});

function SetupPasswordPage() {
  const { error } = useSearch({ from: "/setup/password" });

  const form = useForm({
    defaultValues: {
      username: "",
      new_password: "",
      confirm_password: "",
    },
    onSubmit: ({ value }) => {
      submitForm("/auth/update-credentials", value);
    },
    validators: {
      onSubmit: ({ value }) => {
        const errors: Record<string, string> = {};
        if (!value.username.trim()) {
          errors.username = "Username is required";
        }
        if (!value.new_password) {
          errors.new_password = "Password is required";
        } else if (value.new_password.length < 8) {
          errors.new_password = "Password must be at least 8 characters";
        }
        if (value.new_password !== value.confirm_password) {
          errors.confirm_password = "Passwords do not match";
        }
        return Object.keys(errors).length > 0 ? `${Object.values(errors).join(", ")}` : undefined;
      },
    },
  });

  return (
    <div className="flex min-h-screen items-center justify-center bg-[radial-gradient(circle_at_top,rgba(59,130,246,.12),transparent_34%),linear-gradient(180deg,rgba(255,255,255,.96),rgba(244,244,245,.92))] px-4 py-10 dark:bg-[radial-gradient(circle_at_top,rgba(59,130,246,.18),transparent_28%),linear-gradient(180deg,rgba(9,9,11,.98),rgba(17,24,39,.96))]">
      <div className="flex w-full max-w-md flex-col gap-6">
        <Card>
          <CardHeader>
            <img
              src="/yoink.svg"
              alt="yoink"
              className="size-10 rounded-xl shadow-[0_8px_20px_rgba(59,130,246,.12)]"
            />
            <CardTitle>Set up your credentials</CardTitle>
            <CardDescription>
              Choose a username and password to secure your yoink instance.
            </CardDescription>
          </CardHeader>
          <CardContent>
            <form
              onSubmit={(e) => {
                e.preventDefault();
                e.stopPropagation();
                void form.handleSubmit();
              }}
              className="flex flex-col gap-4"
            >
              {error && (
                <div className="flex items-center gap-2 rounded-lg border border-red-200 bg-red-50 px-3 py-2.5 text-sm text-red-700 dark:border-red-900/50 dark:bg-red-950/50 dark:text-red-400">
                  <AlertCircleIcon className="size-4 shrink-0" />
                  {error}
                </div>
              )}

              <form.Field name="username">
                {(field) => (
                  <div className="flex flex-col gap-1.5">
                    <Label htmlFor="setup-username">Username</Label>
                    <Input
                      id="setup-username"
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

              <form.Field name="new_password">
                {(field) => (
                  <div className="flex flex-col gap-1.5">
                    <Label htmlFor="setup-new-password">New Password</Label>
                    <Input
                      id="setup-new-password"
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
                    <Label htmlFor="setup-confirm-password">Confirm Password</Label>
                    <Input
                      id="setup-confirm-password"
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
                    {isSubmitting ? "Saving..." : "Set Credentials"}
                  </Button>
                )}
              </form.Subscribe>
            </form>
          </CardContent>
        </Card>
      </div>
    </div>
  );
}
