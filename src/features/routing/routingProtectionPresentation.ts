import type { RoutingProtectionStatus } from "@/lib/types/routing";

/**
 * Compatibility health snapshots remain available to the backend for legacy
 * reads and migration safety. They are not a user-facing protection fact;
 * effective impact is shown by the candidate health/eligibility views.
 */
export function userVisibleProtectionEntries(
  status: RoutingProtectionStatus | null | undefined,
) {
  return (status?.entries ?? []).filter(
    (entry) =>
      entry.scopeKind === "station_key" &&
      entry.persistenceKind !== "legacy_compatibility" &&
      entry.state !== "unavailable" &&
      entry.state !== "no_protection",
  );
}
