import { queryOptions } from "@tanstack/react-query";
import { listChangeEvents } from "@/lib/api/changeEvents";
import {
  getCaptureSessionStatus,
  getLatestCollectorSnapshot,
  listLatestCollectorSnapshots,
  listCollectorSnapshots,
} from "@/lib/api/collector";
import { listCollectorRuns } from "@/lib/api/collectorRuns";
import { listCurrentStationBalanceSnapshots, listModelBasePrices } from "@/lib/api/economics";
import { getProxyStatus, listRequestLogs } from "@/lib/api/proxy";
import { getSettings } from "@/lib/api/settings";
import { listKeyPoolItems } from "@/lib/api/stationKeys";
import { listStations } from "@/lib/api/stations";
import { loadChannelMonitoringWorkspace, loadChannelStatusWorkspace } from "@/lib/queries/channelQueries";
import { loadLocalRoutingWorkspace } from "@/lib/queries/localRoutingQueries";
import { loadPricingComparisonWorkspace } from "@/lib/queries/pricingQueries";
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

export const changeEventsQueryOptions = (refetchInterval: number | false = false) =>
  queryOptions({
    queryKey: queryKeys.changeEvents,
    queryFn: listChangeEvents,
    staleTime: 2_000,
    refetchInterval,
  });

export const localRoutingWorkspaceQueryOptions = () =>
  queryOptions({
    queryKey: queryKeys.localRoutingWorkspace,
    queryFn: loadLocalRoutingWorkspace,
    staleTime: 2_000,
  });

export const channelStatusQueryOptions = (refetchInterval: number | false = false) =>
  queryOptions({
    queryKey: queryKeys.channelStatus,
    queryFn: loadChannelStatusWorkspace,
    staleTime: 5_000,
    refetchInterval,
  });

export const pricingComparisonQueryOptions = (refetchInterval: number | false = false) =>
  queryOptions({
    queryKey: queryKeys.pricing,
    queryFn: loadPricingComparisonWorkspace,
    staleTime: 0,
    refetchInterval,
  });

export const channelMonitoringQueryOptions = () =>
  queryOptions({
    queryKey: queryKeys.channelMonitoring,
    queryFn: loadChannelMonitoringWorkspace,
    staleTime: 5_000,
  });
