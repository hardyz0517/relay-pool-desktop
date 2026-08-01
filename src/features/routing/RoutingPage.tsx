import { useCallback, useEffect, useMemo, useState } from "react";
import { RefreshCcw } from "lucide-react";
import { usePageQueryEnabled } from "@/app/navigation/PageVisibility";
import { PageScaffold } from "@/components/shell/PageScaffold";
import { Button, SegmentedControl, useToast } from "@/components/ui";
import { startLocalProxy, stopLocalProxy } from "@/lib/api/proxy";
import { readError } from "@/lib/errors";
import { queryKeys } from "@/lib/query/queryKeys";
import { localRoutingWorkspaceQueryOptions } from "@/lib/query/resourceQueries";
import { useActivityQuery } from "@/lib/query/useActivityQuery";
import {
  listRecentRouteDecisionsQuery,
  loadRoutingRuntimeOverlayQuery,
  loadRoutingWorkspaceSnapshotQuery,
  routingQueryKeys,
} from "@/lib/queries/routingQueries";
import { toTimestampMillis } from "@/lib/time";
import { useQueryClient } from "@tanstack/react-query";
import { LocalRoutingEditTab } from "./LocalRoutingEditTab";
import { LocalRoutingStatusTab } from "./LocalRoutingStatusTab";
import { RoutingStatusDiagnosticsPanel } from "./RoutingStatusDiagnosticsPanel";
import type { VersionedRoutingDeepLink } from "@/lib/types/routingDeepLinks";
import { useCooldownClock } from "./useCooldownClock";

type LocalRoutingTab = "status" | "edit";

export function RoutingPage({
  deepLink,
  onOpenRequestLog,
}: {
  deepLink?: VersionedRoutingDeepLink | null;
  onOpenRequestLog?: (requestLogId: string) => void;
}) {
  const toast = useToast();
  const queryClient = useQueryClient();
  const queryEnabled = usePageQueryEnabled();
  const [activeTab, setActiveTab] = useState<LocalRoutingTab>("status");
  const [proxyActionPending, setProxyActionPending] = useState(false);
  const workspaceQuery = useActivityQuery(localRoutingWorkspaceQueryOptions());
  const routingSnapshotQuery = useActivityQuery({
    queryKey: routingQueryKeys.workspaceSnapshot({ limit: 50 }),
    queryFn: () => loadRoutingWorkspaceSnapshotQuery({ limit: 50 }),
    staleTime: 5_000,
  });
  const routingRuntimeQuery = useActivityQuery({
    queryKey: routingQueryKeys.runtimeOverlay(),
    queryFn: loadRoutingRuntimeOverlayQuery,
    staleTime: 1_000,
    refetchInterval: queryEnabled && activeTab === "status" ? 1_000 : false,
  });
  const routeDecisionsQuery = useActivityQuery({
    queryKey: routingQueryKeys.recentDecisions({ limit: 8 }),
    queryFn: () => listRecentRouteDecisionsQuery({ limit: 8 }),
    staleTime: 5_000,
  });
  const workspace = workspaceQuery.data ?? null;
  const loading = workspaceQuery.isPending && workspaceQuery.data === undefined;
  const error = workspaceQuery.error ? readError(workspaceQuery.error) : null;
  const cooldownDeadlines = useMemo(
    () =>
      (workspace?.candidates ?? []).flatMap((candidate) => {
        if (candidate.healthState !== "cooldown" || candidate.cooldownUntil == null) return [];
        const untilMs = toTimestampMillis(candidate.cooldownUntil);
        return Number.isFinite(untilMs) ? [{ id: candidate.stationKeyId, untilMs }] : [];
      }),
    [workspace?.candidates],
  );

  const handleCooldownExpired = useCallback(() => {
    void queryClient.invalidateQueries({ queryKey: queryKeys.localRoutingWorkspace });
  }, [queryClient]);

  const nowMs = useCooldownClock({
    active: queryEnabled && activeTab === "status" && cooldownDeadlines.length > 0,
    deadlines: cooldownDeadlines,
    onExpired: handleCooldownExpired,
  });

  const handleToggleProxy = useCallback(async () => {
    if (!workspace || proxyActionPending) return;
    setProxyActionPending(true);
    try {
      if (workspace.proxyStatus.running) {
        const nextStatus = await stopLocalProxy();
        queryClient.setQueryData(queryKeys.proxyStatus, nextStatus);
        toast.success("本地路由已停止");
      } else {
        const nextStatus = await startLocalProxy();
        queryClient.setQueryData(queryKeys.proxyStatus, nextStatus);
        toast.success("本地路由已启动", `监听 ${nextStatus.bindAddr}:${nextStatus.port}`);
      }
      await queryClient.invalidateQueries({ queryKey: queryKeys.localRoutingWorkspace });
    } catch (actionError) {
      toast.error(
        workspace.proxyStatus.running ? "停止本地路由失败" : "启动本地路由失败",
        readError(actionError),
      );
    } finally {
      setProxyActionPending(false);
    }
  }, [proxyActionPending, queryClient, toast, workspace]);

  const handleRefresh = useCallback(() => {
    void Promise.all([
      queryClient.invalidateQueries({ queryKey: queryKeys.localRoutingWorkspace }),
      queryClient.invalidateQueries({ queryKey: routingQueryKeys.workspaceSnapshot({ limit: 50 }) }),
      queryClient.invalidateQueries({ queryKey: routingQueryKeys.runtimeOverlay() }),
      queryClient.invalidateQueries({ queryKey: routingQueryKeys.recentDecisions({ limit: 8 }) }),
    ]);
  }, [queryClient]);

  useEffect(() => {
    if (error) toast.error("刷新本地路由状态失败", error);
  }, [error, toast]);

  useEffect(() => {
    if (deepLink) {
      setActiveTab("status");
    }
  }, [deepLink?.sequence]);

  return (
    <PageScaffold
      title="路由规则"
      actions={
        <div className="flex flex-wrap items-center justify-end gap-2">
          <SegmentedControl
            ariaLabel="本地路由页面"
            value={activeTab}
            options={[
              { value: "status", label: "状态" },
              { value: "edit", label: "编辑" },
            ]}
            onChange={setActiveTab}
          />
          <Button
            disabled={loading || proxyActionPending}
            variant="secondary"
            onClick={handleRefresh}
          >
            <RefreshCcw className="h-4 w-4" />
            刷新
          </Button>
        </div>
      }
    >
      {activeTab === "status" ? (
        <div className="grid gap-4">
          <LocalRoutingStatusTab
            loading={loading}
            workspace={workspace}
            maxRateMultiplier={routingSnapshotQuery.data?.maxRateMultiplier}
            nowMs={nowMs}
            proxyActionPending={proxyActionPending}
            onToggleProxy={() => void handleToggleProxy()}
          />
          <RoutingStatusDiagnosticsPanel
            snapshot={routingSnapshotQuery.data ?? null}
            runtimeOverlay={routingRuntimeQuery.data ?? null}
            decisions={routeDecisionsQuery.data ?? null}
            loading={routingSnapshotQuery.isPending && routingSnapshotQuery.data === undefined}
            deepLink={deepLink}
            onOpenRequestLog={onOpenRequestLog}
          />
        </div>
      ) : activeTab === "edit" ? (
        <LocalRoutingEditTab loading={loading} workspace={workspace} />
      ) : null}
    </PageScaffold>
  );
}
