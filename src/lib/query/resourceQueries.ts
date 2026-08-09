import { queryOptions } from "@tanstack/react-query";
import {
  getCaptureSessionStatus,
  getLatestCollectorSnapshot,
  listLatestCollectorSnapshots,
  listCollectorSnapshots,
} from "@/lib/api/collector";
import { listCollectorRuns } from "@/lib/api/collectorRuns";
import { listCurrentStationBalanceSnapshots, listModelBasePrices } from "@/lib/api/economics";
import {
  loadDashboardCumulativeRequestMetrics,
  loadDashboardLiveRequestMetrics,
} from "@/lib/api/dashboardMetrics";
import { getProxyStatus, listRequestLogs } from "@/lib/api/proxy";
import { getSettings } from "@/lib/api/settings";
import { listKeyPoolItems } from "@/lib/api/stationKeys";
import { listStations } from "@/lib/api/stations";
import {
  getChannelMonitorExecution,
  listChannelMonitorAttempts,
  listChannelMonitorExecutions,
  listMonitoringCapabilities,
  loadChannelMonitoringWorkspace,
  loadChannelStatusWorkspace,
} from "@/lib/queries/channelQueries";
import type {
  ChannelMonitorAttemptHistoryInput,
  ChannelMonitorExecutionListInput,
  ChannelStatusWorkspaceInput,
} from "@/lib/types/channelMonitors";
import type { DashboardRequestMetricsInput } from "@/lib/types/dashboardMetrics";
import {
  loadPricingComparisonWorkspace,
  loadPricingGroupMonitorStatus,
} from "@/lib/queries/pricingQueries";
import type { PricingGroupMonitorStatusInput } from "@/lib/types/pricingMonitoring";
import { queryKeys } from "@/lib/query/queryKeys";
import { withQueryTimeout } from "@/lib/query/withQueryTimeout";

export const settingsQueryOptions = () =>
  queryOptions({
    queryKey: queryKeys.settings,
    queryFn: getSettings,
    staleTime: 60_000,
  });

export const proxyStatusQueryOptions = (refetchInterval: number | false = false) =>
  queryOptions({
    queryKey: queryKeys.proxyStatus,
    queryFn: getProxyStatus,
    staleTime: 1_000,
    refetchInterval,
  });

export const requestLogsQueryOptions = (refetchInterval: number | false = false) =>
  queryOptions({
    queryKey: queryKeys.requestLogs,
    queryFn: listRequestLogs,
    staleTime: 2_000,
    refetchInterval,
  });

export const dashboardLiveRequestMetricsQueryOptions = (
  input: DashboardRequestMetricsInput,
  refetchInterval: number | false = false,
) =>
  queryOptions({
    queryKey: queryKeys.dashboardLiveRequestMetrics(input),
    queryFn: () => loadDashboardLiveRequestMetrics(input),
    staleTime: 1_000,
    refetchInterval,
  });

export const dashboardCumulativeRequestMetricsQueryOptions = (
  refetchInterval: number | false = false,
) =>
  queryOptions({
    queryKey: queryKeys.dashboardCumulativeRequestMetrics,
    queryFn: loadDashboardCumulativeRequestMetrics,
    staleTime: 5_000,
    refetchInterval,
  });

export const stationsQueryOptions = (refetchInterval: number | false = false) =>
  queryOptions({
    queryKey: queryKeys.stations,
    queryFn: listStations,
    staleTime: 5_000,
    refetchInterval,
  });

export const stationAssetQueryOptions = (stationId: string) =>
  queryOptions({
    queryKey: queryKeys.stationAsset(stationId),
    queryFn: () =>
      withQueryTimeout(
        getLatestCollectorSnapshot(stationId),
        `station asset snapshot ${stationId}`,
        6_000,
      ),
    staleTime: 30_000,
  });

export const stationAssetsQueryOptions = (stationIds: readonly string[]) =>
  queryOptions({
    queryKey: queryKeys.stationAssetsForStations(stationIds),
    enabled: stationIds.length > 0,
    queryFn: () =>
      withQueryTimeout(
        listLatestCollectorSnapshots([...stationIds]),
        "station asset snapshots",
        6_000,
      ),
    staleTime: 30_000,
  });

export const collectorSnapshotsQueryOptions = (stationId: string) =>
  queryOptions({
    queryKey: queryKeys.collectorSnapshots(stationId),
    queryFn: () => listCollectorSnapshots(stationId),
    staleTime: 30_000,
  });

export const collectorRunsQueryOptions = (stationId: string) =>
  queryOptions({
    queryKey: queryKeys.collectorRuns(stationId),
    queryFn: () => listCollectorRuns(stationId),
    staleTime: 10_000,
  });

export const captureSessionStatusQueryOptions = (stationId: string) =>
  queryOptions({
    queryKey: queryKeys.captureSessionStatus(stationId),
    queryFn: () => getCaptureSessionStatus(stationId),
    staleTime: 2_000,
  });

export const keyPoolQueryOptions = (refetchInterval: number | false = false) =>
  queryOptions({
    queryKey: queryKeys.keyPool,
    queryFn: listKeyPoolItems,
    staleTime: 5_000,
    refetchInterval,
  });

export const modelBasePricesQueryOptions = () =>
  queryOptions({
    queryKey: queryKeys.modelBasePrices,
    queryFn: listModelBasePrices,
    staleTime: 60_000,
  });

export const currentStationBalanceSnapshotsQueryOptions = (
  refetchInterval: number | false = false,
) =>
  queryOptions({
    queryKey: queryKeys.balanceSnapshots,
    queryFn: listCurrentStationBalanceSnapshots,
    staleTime: 5_000,
    refetchInterval,
  });

export const channelStatusQueryOptions = (
  refetchInterval: number | false = false,
  input: ChannelStatusWorkspaceInput = {},
) =>
  queryOptions({
    queryKey: [...queryKeys.channelStatus, input],
    queryFn: () => loadChannelStatusWorkspace(input),
    staleTime: 5_000,
    refetchInterval,
  });

export const channelMonitorExecutionsQueryOptions = (
  input: ChannelMonitorExecutionListInput = {},
) =>
  queryOptions({
    queryKey: [...queryKeys.channelMonitorExecutions, input],
    queryFn: () => listChannelMonitorExecutions(input),
    staleTime: 5_000,
  });

export const channelMonitorExecutionQueryOptions = (executionId: string | null) =>
  queryOptions({
    queryKey: queryKeys.channelMonitorExecution(executionId ?? ""),
    enabled: Boolean(executionId),
    queryFn: () => getChannelMonitorExecution(executionId ?? ""),
    staleTime: 5_000,
  });

export const channelMonitorAttemptsQueryOptions = (
  input: ChannelMonitorAttemptHistoryInput,
) =>
  queryOptions({
    queryKey: [...queryKeys.channelMonitorAttempts, input],
    enabled: Boolean(input.executionId),
    queryFn: () => listChannelMonitorAttempts(input),
    staleTime: 5_000,
  });

export const monitoringCapabilitiesQueryOptions = () =>
  queryOptions({
    queryKey: queryKeys.monitoringCapabilities,
    queryFn: listMonitoringCapabilities,
    staleTime: 60_000,
  });

export const pricingComparisonQueryOptions = (refetchInterval: number | false = false) =>
  queryOptions({
    queryKey: queryKeys.pricing,
    queryFn: loadPricingComparisonWorkspace,
    staleTime: 0,
    refetchInterval,
  });

export const pricingGroupMonitorStatusQueryOptions = (
  input: PricingGroupMonitorStatusInput,
  enabled: boolean,
) =>
  queryOptions({
    queryKey: queryKeys.pricingGroupMonitorStatus(input),
    enabled,
    queryFn: () => loadPricingGroupMonitorStatus(input),
    meta: { suppressGlobalErrorNotification: true },
    // A failed optional projection must not turn into a permanent request loop.
    // The next explicit refresh/focus can retry it after the underlying runtime
    // or schema issue has been repaired.
    retry: false,
    staleTime: 5_000,
    refetchInterval: (query) => (query.state.status === "error" ? false : 5_000),
  });

export const channelMonitoringQueryOptions = (refetchInterval: number | false = false) =>
  queryOptions({
    queryKey: queryKeys.channelMonitoring,
    queryFn: loadChannelMonitoringWorkspace,
    staleTime: 5_000,
    refetchInterval,
  });
