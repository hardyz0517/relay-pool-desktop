import type { AppRouteId } from "@/lib/types/navigation";

export type PageRetentionDecision = {
  retain: boolean;
  reason: "active" | "transition" | "legacy-allowlist";
};

const retainedDuringStage3Migration = new Set<AppRouteId>([
  "dashboard",
  "stations",
  "keyPool",
  "routing",
  "pricing",
  "channels",
  "collectors",
  "changes",
  "logs",
  "settings",
]);

export const MAX_RETAINED_SHELL_PAGES = retainedDuringStage3Migration.size;

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
    retain: retainedDuringStage3Migration.has(routeId),
    reason: "legacy-allowlist",
  };
}
