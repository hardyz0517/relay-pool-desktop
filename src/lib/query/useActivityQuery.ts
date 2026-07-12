import { useEffect } from "react";
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
  enabled?: boolean;
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
  const queryEnabled = active && options.enabled !== false;
  const result = useQuery({
    ...options,
    enabled: queryEnabled,
    subscribed: active,
  });

  useEffect(() => {
    if (!active && result.fetchStatus === "fetching") recordHiddenPageQueryStart();
  }, [active, result.fetchStatus]);

  return result;
}
