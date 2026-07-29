import type { AppRouteId } from "@/lib/types/navigation";

export type PageRetentionDecision = {
  retain: boolean;
  reason: "active" | "transition" | "default-unmounted";
};

export const MAX_RETAINED_SHELL_PAGES = 2;

export function getPageRetentionDecision({
  routeId,
  activeRouteId,
  previousRouteId,
}: {
  routeId: AppRouteId;
  activeRouteId: AppRouteId;
  previousRouteId: AppRouteId | null;
}): PageRetentionDecision {
  if (routeId === activeRouteId) return { retain: true, reason: "active" };
  if (routeId === previousRouteId) return { retain: true, reason: "transition" };
  return {
    retain: false,
    reason: "default-unmounted",
  };
}
