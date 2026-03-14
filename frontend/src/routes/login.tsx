import { createFileRoute, useSearch } from "@tanstack/react-router";
import { AlertCircleIcon } from "lucide-react";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import { Label } from "@/components/ui/label";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";

interface LoginSearch {
  error?: string;
  next?: string;
}

export const Route = createFileRoute("/login")({
  validateSearch: (search: Record<string, unknown>): LoginSearch => ({
    error: typeof search.error === "string" ? search.error : undefined,
    next: typeof search.next === "string" ? search.next : undefined,
  }),
  component: LoginPage,
});

function LoginPage() {
  const { error, next } = useSearch({ from: "/login" });

  // Sanitize redirect: must start with / and not //
  const safeNext =
    next && next.startsWith("/") && !next.startsWith("//") ? next : "/";

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
            <CardDescription>
              Enter your credentials to access your library
            </CardDescription>
          </CardHeader>
          <CardContent>
            <form
              method="post"
              action="/auth/login"
              className="flex flex-col gap-4"
            >
              <input type="hidden" name="next" value={safeNext} />

              {error && (
                <div className="flex items-center gap-2 rounded-lg border border-red-200 bg-red-50 px-3 py-2.5 text-sm text-red-700 dark:border-red-900/50 dark:bg-red-950/50 dark:text-red-400">
                  <AlertCircleIcon className="size-4 shrink-0" />
                  {error}
                </div>
              )}

              <div className="flex flex-col gap-1.5">
                <Label htmlFor="login-username">Username</Label>
                <Input
                  id="login-username"
                  type="text"
                  name="username"
                  autoComplete="username"
                  required
                />
              </div>

              <div className="flex flex-col gap-1.5">
                <Label htmlFor="login-password">Password</Label>
                <Input
                  id="login-password"
                  type="password"
                  name="password"
                  autoComplete="current-password"
                  required
                />
              </div>

              <Button type="submit" className="w-full">
                Sign In
              </Button>
            </form>
          </CardContent>
        </Card>
      </div>
    </div>
  );
}
