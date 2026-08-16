import { queryOptions } from "@tanstack/react-query";
import { getActiveBackendClient } from "@/lib/bridge/activeBackendClient";
import { queryKeys } from "@/lib/query/queryKeys";
import type { RuntimeDiagnosticsQueryDto } from "@/lib/bridge/generated";

export function runtimeDiagnosticsQueryOptions(input: RuntimeDiagnosticsQueryDto) {
  return queryOptions({
    queryKey: queryKeys.runtimeDiagnostics(input),
    queryFn: async () => {
      const diagnostics = getActiveBackendClient().runtimeDiagnostics;
      if (!diagnostics) throw new Error("runtime diagnostics unavailable");
      return diagnostics.readRuntimeDiagnostics(input);
    },
    staleTime: 2_000,
    refetchOnWindowFocus: false,
  });
}
