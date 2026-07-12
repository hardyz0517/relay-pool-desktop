import type { AppPageId, AppRouteId } from "@/lib/types/navigation";

export type CommittedNavigation = {
  activeRouteId: AppPageId;
  previousRouteId: AppPageId | null;
  transientParentRouteId: AppRouteId | null;
};

export type NavigationIntent = {
  routeId: AppPageId;
  shellRouteId: AppRouteId;
  transientParentRouteId: AppRouteId | null;
  sequence: number;
};

export function createInitialNavigationIntent(routeId: AppRouteId): NavigationIntent {
  return {
    routeId,
    shellRouteId: routeId,
    transientParentRouteId: null,
    sequence: 0,
  };
}

export function createNavigationIntent(
  routeId: AppPageId,
  shellRouteId: AppRouteId,
  transientParentRouteId: AppRouteId | null,
  sequence: number,
): NavigationIntent {
  return {
    routeId,
    shellRouteId,
    transientParentRouteId,
    sequence,
  };
}

export function commitNavigationIntent(
  current: CommittedNavigation,
  intent: NavigationIntent,
  latestSequence: number,
): CommittedNavigation {
  if (intent.sequence !== latestSequence || current.activeRouteId === intent.routeId) {
    return current;
  }

  return {
    activeRouteId: intent.routeId,
    previousRouteId: current.activeRouteId,
    transientParentRouteId: intent.transientParentRouteId,
  };
}
