import type { AnyRouteMatch } from "@tanstack/react-router";

export type BreadcrumbLabel = string | null | undefined;
export type BreadcrumbResolver = (match: AnyRouteMatch) => BreadcrumbLabel;
export type BreadcrumbValue = BreadcrumbLabel | BreadcrumbResolver;

export type ResolvedBreadcrumb = {
  id: string;
  pathname: string;
  label: string;
};

declare module "@tanstack/react-router" {
  interface StaticDataRouteOption {
    breadcrumb?: BreadcrumbValue;
  }
}

function normalizeBreadcrumbLabel(label: BreadcrumbLabel): string | null {
  if (typeof label !== "string") {
    return null;
  }

  const trimmed = label.trim();
  return trimmed.length > 0 ? trimmed : null;
}

export function resolveBreadcrumbLabel(match: AnyRouteMatch): string | null {
  const breadcrumb = match.staticData?.breadcrumb;

  if (typeof breadcrumb === "function") {
    return normalizeBreadcrumbLabel(breadcrumb(match));
  }

  return normalizeBreadcrumbLabel(breadcrumb);
}

export function buildBreadcrumbs(matches: ReadonlyArray<AnyRouteMatch>): Array<ResolvedBreadcrumb> {
  return matches.flatMap((match) => {
    const label = resolveBreadcrumbLabel(match);

    if (!label) {
      return [];
    }

    return [
      {
        id: match.id,
        pathname: match.pathname,
        label,
      },
    ];
  });
}
