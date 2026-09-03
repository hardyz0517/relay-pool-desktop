import { useEffect, useMemo, useRef } from "react";
import {
  useQueryClient,
  useQuery,
  type DefaultError,
  type QueryKey,
  type UseQueryOptions,
  type UseQueryResult,
} from "@tanstack/react-query";
import { recordHiddenPageQueryStart } from "@/app/navigationPerformance";
import { usePageQueryEnabled } from "@/app/navigation/PageVisibility";
import { recordMonitoringPerformance } from "@/lib/monitoringPerformance";

type ActivityQueryOptions<
  TQueryFnData,
  TError,
  TData,
  TQueryKey extends QueryKey,
> = Omit<UseQueryOptions<TQueryFnData, TError, TData, TQueryKey>, "enabled" | "subscribed"> & {
  enabled?: UseQueryOptions<TQueryFnData, TError, TData, TQueryKey>["enabled"];
};

export function useActivityQuery<
  TQueryFnData,
  TError = DefaultError,
  TData = TQueryFnData,
  TQueryKey extends QueryKey = QueryKey,
>(
  options: ActivityQueryOptions<TQueryFnData, TError, TData, TQueryKey>,
): UseQueryResult<TData, TError> {
  const active = usePageQueryEnabled();
  const queryClient = useQueryClient();
  const previousActive = useRef(active);
  const requestedEnabled = options.enabled !== false;
  const queryEnabled = active && requestedEnabled;
  const guardedQueryFn = useMemo(() => {
    const queryFn = options.queryFn;
    if (typeof queryFn !== "function") {
      return queryFn;
    }
    return (async (context: Parameters<typeof queryFn>[0]) => {
      if (!active) {
        recordHiddenPageQueryStart();
      }
      const collectMetrics = import.meta.env.DEV;
      const started = collectMetrics ? performance.now() : 0;
      const cacheHit = collectMetrics
        ? queryClient.getQueryData(options.queryKey) !== undefined
        : false;
      try {
        const value = await queryFn(context);
        if (collectMetrics) {
          recordMonitoringPerformance({
            name: "monitoring-query-call",
            durationMs: performance.now() - started,
            queryCalls: 1,
            cacheHit,
          });
        }
        return value;
      } catch (error) {
        if (collectMetrics) {
          recordMonitoringPerformance({
            name: "monitoring-query-error",
            durationMs: performance.now() - started,
            queryCalls: 1,
            cacheHit,
          });
        }
        throw error;
      }
    }) as typeof queryFn;
  }, [active, options.queryFn, options.queryKey, queryClient]);
  const result = useQuery({
    ...options,
    queryFn: guardedQueryFn,
    enabled: queryEnabled,
    subscribed: active,
  });
  const { isFetching, refetch } = result;

  useEffect(() => {
    const becameActive = active && !previousActive.current;
    previousActive.current = active;
    if (becameActive && requestedEnabled && !isFetching) {
      void refetch();
    }
  }, [active, requestedEnabled, isFetching, refetch]);

  return result;
}
