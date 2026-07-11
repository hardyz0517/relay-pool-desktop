import type { AppPageId, AppRouteId } from "@/lib/types/navigation";

export type PageTransitionKind = "shell" | "transient";
export type PageTransitionDirection = "none" | "forward" | "back";

export type PageTransitionPolicy = {
  pageId: AppPageId;
  kind: PageTransitionKind;
  parentRouteId: AppRouteId;
  enterDirection: PageTransitionDirection;
  exitDirection: PageTransitionDirection;
};

const shellPagePolicies: Record<AppRouteId, PageTransitionPolicy> = {
  dashboard: {
    pageId: "dashboard",
    kind: "shell",
    parentRouteId: "dashboard",
    enterDirection: "none",
    exitDirection: "none",
  },
  stations: {
    pageId: "stations",
    kind: "shell",
    parentRouteId: "stations",
    enterDirection: "none",
    exitDirection: "none",
  },
  keyPool: {
    pageId: "keyPool",
    kind: "shell",
    parentRouteId: "keyPool",
    enterDirection: "none",
    exitDirection: "none",
  },
  routing: {
    pageId: "routing",
    kind: "shell",
    parentRouteId: "routing",
    enterDirection: "none",
    exitDirection: "none",
  },
  pricing: {
    pageId: "pricing",
    kind: "shell",
    parentRouteId: "pricing",
    enterDirection: "none",
    exitDirection: "none",
  },
  channels: {
    pageId: "channels",
    kind: "shell",
    parentRouteId: "channels",
    enterDirection: "none",
    exitDirection: "none",
  },
  collectors: {
    pageId: "collectors",
    kind: "shell",
    parentRouteId: "collectors",
    enterDirection: "none",
    exitDirection: "none",
  },
  changes: {
    pageId: "changes",
    kind: "shell",
    parentRouteId: "changes",
    enterDirection: "none",
    exitDirection: "none",
  },
  logs: {
    pageId: "logs",
    kind: "shell",
    parentRouteId: "logs",
    enterDirection: "none",
    exitDirection: "none",
  },
  settings: {
    pageId: "settings",
    kind: "shell",
    parentRouteId: "settings",
    enterDirection: "none",
    exitDirection: "none",
  },
};

const transientPagePolicies = {
  addProvider: {
    pageId: "addProvider",
    kind: "transient",
    parentRouteId: "stations",
    enterDirection: "forward",
    exitDirection: "back",
  },
  editProvider: {
    pageId: "editProvider",
    kind: "transient",
    parentRouteId: "stations",
    enterDirection: "forward",
    exitDirection: "back",
  },
  stationDetail: {
    pageId: "stationDetail",
    kind: "transient",
    parentRouteId: "stations",
    enterDirection: "forward",
    exitDirection: "back",
  },
  addKey: {
    pageId: "addKey",
    kind: "transient",
    parentRouteId: "keyPool",
    enterDirection: "forward",
    exitDirection: "back",
  },
  editKey: {
    pageId: "editKey",
    kind: "transient",
    parentRouteId: "keyPool",
    enterDirection: "forward",
    exitDirection: "back",
  },
  modelBasePrices: {
    pageId: "modelBasePrices",
    kind: "transient",
    parentRouteId: "pricing",
    enterDirection: "forward",
    exitDirection: "back",
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
