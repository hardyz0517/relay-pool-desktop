import type { RoutingProtectionStatus } from "@/lib/types/routing";

export function userVisibleProtectionEntries(
  status: RoutingProtectionStatus | null | undefined,
) {
  return (status?.entries ?? []).filter(
    (entry) =>
      entry.scopeKind === "station_key" &&
      entry.state !== "unavailable" &&
      entry.state !== "no_protection",
  );
}
