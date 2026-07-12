import { queryOptions } from "@tanstack/react-query";
import { listChangeEvents } from "@/lib/api/changeEvents";
import { listBalanceSnapshots } from "@/lib/api/economics";
import { getProxyStatus, listRequestLogs } from "@/lib/api/proxy";
import { getSettings } from "@/lib/api/settings";
import { listKeyPoolItems } from "@/lib/api/stationKeys";
import { listStations } from "@/lib/api/stations";
import { queryKeys } from "@/lib/query/queryKeys";

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

export const keyPoolQueryOptions = (refetchInterval: number | false = false) =>
  queryOptions({
    queryKey: queryKeys.keyPool,
    queryFn: listKeyPoolItems,
    staleTime: 5_000,
    refetchInterval,
  });

export const balanceSnapshotsQueryOptions = (refetchInterval: number | false = false) =>
  queryOptions({
    queryKey: queryKeys.balanceSnapshots,
    queryFn: listBalanceSnapshots,
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
