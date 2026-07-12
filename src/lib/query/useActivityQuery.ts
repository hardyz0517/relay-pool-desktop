import { useMemo } from "react";
import {
  useQuery,
  type DefaultError,
  type QueryKey,
  type UseQueryOptions,
  type UseQueryResult,
} from "@tanstack/react-query";
import { recordHiddenPageQueryStart } from "@/app/navigationPerformance";

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
  active: boolean,
  options: ActivityQueryOptions<TQueryFnData, TError, TData, TQueryKey>,
): UseQueryResult<TData, TError> {
  const requestedEnabled = options.enabled !== false;
  const queryEnabled = active && requestedEnabled;
  const guardedQueryFn = useMemo(() => {
    const queryFn = options.queryFn;
    if (typeof queryFn !== "function") {
      return queryFn;
    }
    return ((context: Parameters<typeof queryFn>[0]) => {
      if (!active) {
        recordHiddenPageQueryStart();
      }
      return queryFn(context);
    }) as typeof queryFn;
  }, [active, options.queryFn]);
  const result = useQuery({
    ...options,
    queryFn: guardedQueryFn,
    enabled: queryEnabled,
    subscribed: active,
  });

  return result;
}
