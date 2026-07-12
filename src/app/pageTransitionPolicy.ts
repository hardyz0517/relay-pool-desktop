import type { AppPageId, AppRouteId } from "@/lib/types/navigation";

export type PageTransitionKind = "shell" | "transient";

export type PageTransitionPolicy = {
  pageId: AppPageId;
  kind: PageTransitionKind;
  parentRouteId: AppRouteId;
};

const shellPagePolicies: Record<AppRouteId, PageTransitionPolicy> = {
  dashboard: {
    pageId: "dashboard",
    kind: "shell",
    parentRouteId: "dashboard",
  },
  stations: {
    pageId: "stations",
    kind: "shell",
    parentRouteId: "stations",
  },
  keyPool: {
    pageId: "keyPool",
    kind: "shell",
    parentRouteId: "keyPool",
  },
  routing: {
    pageId: "routing",
    kind: "shell",
    parentRouteId: "routing",
  },
  pricing: {
    pageId: "pricing",
    kind: "shell",
    parentRouteId: "pricing",
  },
  channels: {
    pageId: "channels",
    kind: "shell",
    parentRouteId: "channels",
  },
  collectors: {
    pageId: "collectors",
    kind: "shell",
    parentRouteId: "collectors",
  },
  changes: {
    pageId: "changes",
    kind: "shell",
    parentRouteId: "changes",
  },
  logs: {
    pageId: "logs",
    kind: "shell",
    parentRouteId: "logs",
  },
  settings: {
    pageId: "settings",
    kind: "shell",
    parentRouteId: "settings",
  },
};

const transientPagePolicies = {
  addProvider: {
    pageId: "addProvider",
    kind: "transient",
    parentRouteId: "stations",
  },
  editProvider: {
    pageId: "editProvider",
    kind: "transient",
    parentRouteId: "stations",
  },
  stationDetail: {
    pageId: "stationDetail",
    kind: "transient",
    parentRouteId: "stations",
  },
  addKey: {
    pageId: "addKey",
    kind: "transient",
    parentRouteId: "keyPool",
  },
  editKey: {
    pageId: "editKey",
    kind: "transient",
    parentRouteId: "keyPool",
  },
  modelBasePrices: {
    pageId: "modelBasePrices",
    kind: "transient",
    parentRouteId: "pricing",
  },
} satisfies Record<string, PageTransitionPolicy>;

const pageTransitionPolicies = {
  ...shellPagePolicies,
  ...transientPagePolicies,
} satisfies Record<AppPageId, PageTransitionPolicy>;

export function getPageTransitionPolicy(pageId: AppPageId): PageTransitionPolicy {
  return pageTransitionPolicies[pageId];
}

export function isShellPage(pageId: AppPageId): pageId is AppRouteId {
  return getPageTransitionPolicy(pageId).kind === "shell";
}

export function getShellRouteId(pageId: AppPageId): AppRouteId {
  return getPageTransitionPolicy(pageId).parentRouteId;
}

export function resolveTransientParentRouteId(
  currentPageId: AppPageId,
  targetPageId: AppPageId,
  transientParentRouteId: AppRouteId | null,
): AppRouteId | null {
  if (isShellPage(targetPageId)) {
    return null;
  }
  if (isShellPage(currentPageId)) {
    return currentPageId;
  }
  return transientParentRouteId ?? getShellRouteId(currentPageId);
}

export function resolveActiveShellRouteId(
  pageId: AppPageId,
  transientParentRouteId: AppRouteId | null,
): AppRouteId {
  if (isShellPage(pageId)) {
    return pageId;
  }
  return transientParentRouteId ?? getShellRouteId(pageId);
}
