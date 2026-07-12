import type { AppPageId, AppRouteId } from "@/lib/types/navigation";

export type PageTransitionKind = "shell" | "transient";

export type PageTransitionPolicy = {
  pageId: AppPageId;
  kind: PageTransitionKind;
  parentRouteId: AppRouteId;
  retention: "keep";
  prewarmPriority: number | null;
};

const shellPagePolicies: Record<AppRouteId, PageTransitionPolicy> = {
  dashboard: {
    pageId: "dashboard",
    kind: "shell",
    parentRouteId: "dashboard",
    retention: "keep",
    prewarmPriority: null,
  },
  stations: {
    pageId: "stations",
    kind: "shell",
    parentRouteId: "stations",
    retention: "keep",
    prewarmPriority: 2,
  },
  keyPool: {
    pageId: "keyPool",
    kind: "shell",
    parentRouteId: "keyPool",
    retention: "keep",
    prewarmPriority: null,
  },
  routing: {
    pageId: "routing",
    kind: "shell",
    parentRouteId: "routing",
    retention: "keep",
    prewarmPriority: null,
  },
  pricing: {
    pageId: "pricing",
    kind: "shell",
    parentRouteId: "pricing",
    retention: "keep",
    prewarmPriority: null,
  },
  channels: {
    pageId: "channels",
    kind: "shell",
    parentRouteId: "channels",
    retention: "keep",
    prewarmPriority: null,
  },
  collectors: {
    pageId: "collectors",
    kind: "shell",
    parentRouteId: "collectors",
    retention: "keep",
    prewarmPriority: null,
  },
  changes: {
    pageId: "changes",
    kind: "shell",
    parentRouteId: "changes",
    retention: "keep",
    prewarmPriority: 3,
  },
  logs: {
    pageId: "logs",
    kind: "shell",
    parentRouteId: "logs",
    retention: "keep",
    prewarmPriority: null,
  },
  settings: {
    pageId: "settings",
    kind: "shell",
    parentRouteId: "settings",
    retention: "keep",
    prewarmPriority: 1,
  },
};

const transientPagePolicies = {
  addProvider: {
    pageId: "addProvider",
    kind: "transient",
    parentRouteId: "stations",
    retention: "keep",
    prewarmPriority: null,
  },
  editProvider: {
    pageId: "editProvider",
    kind: "transient",
    parentRouteId: "stations",
    retention: "keep",
    prewarmPriority: null,
  },
  stationDetail: {
    pageId: "stationDetail",
    kind: "transient",
    parentRouteId: "stations",
    retention: "keep",
    prewarmPriority: null,
  },
  addKey: {
    pageId: "addKey",
    kind: "transient",
    parentRouteId: "keyPool",
    retention: "keep",
    prewarmPriority: null,
  },
  editKey: {
    pageId: "editKey",
    kind: "transient",
    parentRouteId: "keyPool",
    retention: "keep",
    prewarmPriority: null,
  },
  modelBasePrices: {
    pageId: "modelBasePrices",
    kind: "transient",
    parentRouteId: "pricing",
    retention: "keep",
    prewarmPriority: null,
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
