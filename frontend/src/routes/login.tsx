import { createFileRoute, redirect, useSearch } from "@tanstack/react-router";
import { useForm } from "@tanstack/react-form";
import { AlertCircleIcon } from "lucide-react";
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from "@/components/ui/card";
import { Label } from "@/components/ui/label";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { fetchClient } from "@/lib/api";
import { submitForm } from "@/lib/utils";

interface LoginSearch {
  error?: string;
  next?: string;
}

export const Route = createFileRoute("/login")({
  validateSearch: (search: Record<string, unknown>): LoginSearch => ({
    error: typeof search.error === "string" ? search.error : undefined,
    next: typeof search.next === "string" ? search.next : undefined,
  }),
  beforeLoad: async ({ search }) => {
    const { data } = await fetchClient.GET("/api/auth/status");

    // If already authenticated, redirect away from login
    if (data?.authenticated && !data.must_change_password) {
      const next = (search as LoginSearch).next;
      const safeDest = next && next.startsWith("/") && !next.startsWith("//") ? next : "/";
      throw redirect({ to: safeDest });
    }

    if (data?.authenticated && data.must_change_password) {
      throw redirect({ to: "/setup/password" });
    }
  },
  component: LoginPage,
});

function LoginPage() {
  const { error, next } = useSearch({ from: "/login" });

  // Sanitize redirect: must start with / and not //
  const safeNext = next && next.startsWith("/") && !next.startsWith("//") ? next : "/";

  const form = useForm({
    defaultValues: {
      username: "",
      password: "",
    },
    onSubmit: ({ value }) => {
      submitForm("/auth/login", {
        ...value,
        next: safeNext,
      });
    },
    validators: {
      onSubmit: ({ value }) => {
        const errors: string[] = [];
        if (!value.username.trim()) {
          errors.push("Username is required");
        }
        if (!value.password) {
          errors.push("Password is required");
        }
        return errors.length > 0 ? errors.join(", ") : undefined;
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
            <CardTitle>Sign in to yoink</CardTitle>
            <CardDescription>Enter your credentials to access your library</CardDescription>
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
                    <Label htmlFor="login-username">Username</Label>
                    <Input
                      id="login-username"
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

              <form.Field name="password">
                {(field) => (
                  <div className="flex flex-col gap-1.5">
                    <Label htmlFor="login-password">Password</Label>
                    <Input
                      id="login-password"
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
                    {isSubmitting ? "Signing in..." : "Sign In"}
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
