import type { QueryClient } from "@tanstack/react-query";
import { queryKeys } from "./queryKeys";

/**
 * The pricing page owns the monitor-summary projection, while channel pages
 * own monitor mutations. Keep the invalidation boundary in one place so a new
 * mutation cannot accidentally refresh only one of the two views.
 */
export function invalidatePricingMonitoringQueries(queryClient: QueryClient) {
  return Promise.all([
    queryClient.invalidateQueries({ queryKey: queryKeys.pricing }),
    queryClient.invalidateQueries({ queryKey: queryKeys.pricingGroupMonitorStatusPrefix }),
    queryClient.invalidateQueries({ queryKey: queryKeys.channelMonitoring }),
    queryClient.invalidateQueries({ queryKey: queryKeys.channelStatus }),
  ]);
}
