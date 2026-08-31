import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { usePageQueryEnabled } from "@/app/navigation/PageVisibility";
import { PageScaffold } from "@/components/shell/PageScaffold";
import { SegmentedControl, useToast } from "@/components/ui";
import { startLocalProxy, stopLocalProxy } from "@/lib/api/proxy";
import { importRelayPoolToCCSwitch } from "@/lib/api/settings";
import { readError } from "@/lib/errors";
import { keyPoolQueryOptions, proxyStatusQueryOptions, settingsQueryOptions } from "@/lib/query/resourceQueries";
import { queryKeys } from "@/lib/query/queryKeys";
import { refreshRoutingQueries } from "@/lib/query/routingQuerySynchronization";
import { useActivityQuery } from "@/lib/query/useActivityQuery";
import {
  listRecentRouteDecisionsQuery,
  loadRoutingRuntimeOverlayQuery,
  loadRoutingWorkspaceSnapshotQuery,
  routingProtectionStatusQueryOptions,
  routingQueryKeys,
} from "@/lib/queries/routingQueries";
import { useQueryClient } from "@tanstack/react-query";
import { LocalRoutingEditTab } from "./LocalRoutingEditTab";
import { LocalRoutingStatusTab } from "./LocalRoutingStatusTab";
import { RoutingStatusDiagnosticsPanel } from "./RoutingStatusDiagnosticsPanel";
import type { VersionedRoutingDeepLink } from "@/lib/types/routingDeepLinks";
import { useCooldownClock } from "./useCooldownClock";
import { toRoutingWorkspaceView } from "@/lib/types/routingWorkspace";
import type { RouteEndpointKind } from "@/lib/types/routing";
import type { RoutingViewPreparationPort } from "./routingViewPreparation";

type LocalRoutingTab = "status" | "edit";

export function RoutingPage({
  deepLink,
  onOpenRequestLog,
  onViewPreparationPort,
}: {
  deepLink?: VersionedRoutingDeepLink | null;
  onOpenRequestLog?: (requestLogId: string) => void;
  onViewPreparationPort?: (port: RoutingViewPreparationPort | null) => void;
}) {
  const toast = useToast();
  const queryClient = useQueryClient();
  const queryEnabled = usePageQueryEnabled();
  const [activeTab, setActiveTab] = useState<LocalRoutingTab>("status");
  const activeTabRef = useRef<LocalRoutingTab>(activeTab);
  const mountedRef = useRef(true);
  const [proxyActionPending, setProxyActionPending] = useState(false);
  const [importingCCSwitch, setImportingCCSwitch] = useState(false);
  const proxyStatusQuery = useActivityQuery(proxyStatusQueryOptions());
  const settingsQuery = useActivityQuery(settingsQueryOptions());
  const developerModeEnabled = settingsQuery.data?.developerModeEnabled === true;
  const keyPoolItemsQuery = useActivityQuery({
    ...keyPoolQueryOptions(),
    enabled: queryEnabled,
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
  const protectionStatusQuery = useActivityQuery({
    ...routingProtectionStatusQueryOptions(),
    enabled: queryEnabled && activeTab === "status" && developerModeEnabled,
  });
  const workspace = useMemo(() => {
    if (!routingSnapshotQuery.data || !proxyStatusQuery.data) return null;
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
    return toRoutingWorkspaceView(
      routingSnapshotQuery.data,
      proxyStatusQuery.data,
      routingRuntimeQuery.data ?? null,
      decision,
    );
  }, [latestDecision, proxyStatusQuery.data, routingRuntimeQuery.data, routingSnapshotQuery.data]);
  const loading = routingSnapshotQuery.isPending && routingSnapshotQuery.data === undefined;
  const error = routingSnapshotQuery.error ? readError(routingSnapshotQuery.error) : null;
  const cooldownDeadlines = useMemo(
    () =>
      (workspace?.candidates ?? []).flatMap((candidate) => {
        const circuit = candidate.diagnostics?.circuit;
        if (circuit?.persistenceStatus !== "available" || circuit.state !== "open" || circuit.cooldownUntilMs == null || !Number.isFinite(circuit.cooldownUntilMs)) return [];
        return [{ id: candidate.stationKeyId, untilMs: circuit.cooldownUntilMs }];
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

  useEffect(() => {
    activeTabRef.current = activeTab;
  }, [activeTab]);

  useEffect(() => {
    mountedRef.current = true;
    if (!onViewPreparationPort) return () => { mountedRef.current = false; };

    const port: RoutingViewPreparationPort = {
      showStatusView: () => {
        const previous = activeTabRef.current;
        setActiveTab("status");
        let restored = false;
        return () => {
          if (restored) return;
          restored = true;
          if (mountedRef.current) setActiveTab(previous);
        };
      },
      showSettingsView: () => {
        const previous = activeTabRef.current;
        setActiveTab("edit");
        let restored = false;
        return () => {
          if (restored) return;
          restored = true;
          if (mountedRef.current) setActiveTab(previous);
        };
      },
    };
    onViewPreparationPort(port);
    return () => {
      mountedRef.current = false;
      onViewPreparationPort(null);
    };
  }, [onViewPreparationPort]);

  return (
    <PageScaffold
      title="路由规则"
      actions={
        <div className="flex flex-wrap items-center justify-end gap-2">
          <div data-tour="routing-tabs">
            <SegmentedControl
              ariaLabel="本地路由页面"
              value={activeTab}
              options={[
                { value: "status", label: "概览" },
                { value: "edit", label: "设置" },
              ]}
              onChange={setActiveTab}
            />
          </div>
        </div>
      }
    >
      <div
        className="grid gap-4"
        data-tour={activeTab === "status" ? "routing-status" : "routing-settings"}
      >
        {activeTab === "status" ? (
          <>
            <LocalRoutingStatusTab
              loading={loading}
              workspace={workspace}
              keyPoolItems={keyPoolItemsQuery.data}
              maxRateMultiplier={routingSnapshotQuery.data?.maxRateMultiplier}
              nowMs={nowMs}
              proxyActionPending={proxyActionPending}
              onToggleProxy={() => void handleToggleProxy()}
              importingCCSwitch={importingCCSwitch}
              onImportToCCSwitch={() => void handleImportToCCSwitch()}
              deepLink={deepLink}
            />
            <RoutingStatusDiagnosticsPanel
              snapshot={routingSnapshotQuery.data ?? null}
              runtimeOverlay={routingRuntimeQuery.data ?? null}
              decisions={routeDecisionsQuery.data ?? null}
              protectionStatus={protectionStatusQuery.data ?? null}
              loading={routingSnapshotQuery.isPending && routingSnapshotQuery.data === undefined}
              error={error}
              developerModeEnabled={developerModeEnabled}
              deepLink={deepLink}
              onOpenRequestLog={onOpenRequestLog}
            />
          </>
        ) : activeTab === "edit" ? (
          <LocalRoutingEditTab />
        ) : null}
      </div>
    </PageScaffold>
  );
}
