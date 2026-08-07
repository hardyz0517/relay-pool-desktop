import { useCallback, useEffect, useMemo, useState } from "react";
import { RefreshCcw, Upload } from "lucide-react";
import { usePageQueryEnabled } from "@/app/navigation/PageVisibility";
import { PageScaffold } from "@/components/shell/PageScaffold";
import { Button, SegmentedControl, useToast } from "@/components/ui";
import { startLocalProxy, stopLocalProxy } from "@/lib/api/proxy";
import { importRelayPoolToCCSwitch } from "@/lib/api/settings";
import { readError } from "@/lib/errors";
import { keyPoolQueryOptions, proxyStatusQueryOptions } from "@/lib/query/resourceQueries";
import { queryKeys } from "@/lib/query/queryKeys";
import { refreshRoutingQueries } from "@/lib/query/routingQuerySynchronization";
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
import { toRoutingWorkspaceView } from "@/lib/types/routingWorkspace";
import type { RouteEndpointKind } from "@/lib/types/routing";

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
  const [importingCCSwitch, setImportingCCSwitch] = useState(false);
  const proxyStatusQuery = useActivityQuery(proxyStatusQueryOptions());
  const keyPoolItemsQuery = useActivityQuery({
    ...keyPoolQueryOptions(),
    enabled: activeTab === "edit",
  });
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
  const latestDecision = routeDecisionsQuery.data?.decisions[0] ?? null;
  const workspace = useMemo(() => {
    if (!routingSnapshotQuery.data || !proxyStatusQuery.data) return null;
    const cooldownByKey = new Map(
      (routingRuntimeQuery.data?.candidates ?? []).map((candidate) => [candidate.stationKeyId, candidate.cooldownUntil]),
    );
    const decision = latestDecision
      ? {
          id: latestDecision.requestLogId,
          decidedAt: latestDecision.finishedAt ?? latestDecision.createdAt,
          endpoint: latestDecision.endpoint as RouteEndpointKind,
          model: latestDecision.model,
          selectedStationKeyId: latestDecision.stationKeyId,
          selectedStationId: latestDecision.stationId,
          selectedStationName: null,
          policy: latestDecision.routePolicy ?? "intelligent_planner_v1",
          status: (
            latestDecision.status === "fallback"
              ? "fallback"
              : latestDecision.status === "failed"
                ? "failed"
                : latestDecision.stationKeyId
                  ? "selected"
                  : "unavailable") as "selected" | "fallback" | "failed" | "unavailable",
          reason: latestDecision.routeReason ?? "",
          fallbackCount: latestDecision.fallbackCount,
        }
      : null;
    return toRoutingWorkspaceView(routingSnapshotQuery.data, proxyStatusQuery.data, cooldownByKey, decision);
  }, [latestDecision, proxyStatusQuery.data, routingRuntimeQuery.data, routingSnapshotQuery.data]);
  const loading = routingSnapshotQuery.isPending && routingSnapshotQuery.data === undefined;
  const error = routingSnapshotQuery.error ? readError(routingSnapshotQuery.error) : null;
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
    void refreshRoutingQueries(queryClient);
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
      await refreshRoutingQueries(queryClient);
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
    void refreshRoutingQueries(queryClient);
  }, [queryClient]);

  const handleImportToCCSwitch = useCallback(async () => {
    if (importingCCSwitch) return;
    setImportingCCSwitch(true);
    try {
      const result = await importRelayPoolToCCSwitch();
      toast.success("已唤起 CCSwitch", `${result.providerName} - ${result.endpoint}`);
    } catch (importError) {
      toast.error("导入 CCSwitch 失败", readError(importError));
    } finally {
      setImportingCCSwitch(false);
    }
  }, [importingCCSwitch, toast]);

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
          <Button
            disabled={importingCCSwitch}
            variant="secondary"
            onClick={() => void handleImportToCCSwitch()}
          >
            <Upload className="h-4 w-4" />
            {importingCCSwitch ? "导入中" : "导入到 CCSwitch"}
          </Button>
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
            deepLink={deepLink}
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
        <LocalRoutingEditTab
          loading={loading || keyPoolItemsQuery.isPending}
          keyPoolItems={keyPoolItemsQuery.data}
          workspace={workspace}
        />
      ) : null}
    </PageScaffold>
  );
}
