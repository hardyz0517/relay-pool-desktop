import { useCallback, useEffect, useMemo, useState } from "react";
import { RefreshCcw } from "lucide-react";
import { PageScaffold } from "@/components/shell/PageScaffold";
import { usePageRefreshEnabled } from "@/components/shell/PageActivity";
import { startLocalProxy, stopLocalProxy } from "@/lib/api/proxy";
import { SETTINGS_UPDATED_EVENT } from "@/lib/api/settings";
import { Button, SegmentedControl, useToast } from "@/components/ui";
import { readError } from "@/lib/errors";
import { queryKeys } from "@/lib/query/queryKeys";
import { localRoutingWorkspaceQueryOptions } from "@/lib/query/resourceQueries";
import { useActivityQuery } from "@/lib/query/useActivityQuery";
import { toTimestampMillis } from "@/lib/time";
import { useQueryClient } from "@tanstack/react-query";
import { LocalRoutingEditTab } from "./LocalRoutingEditTab";
import { LocalRoutingStatusTab } from "./LocalRoutingStatusTab";
import { useCooldownClock } from "./useCooldownClock";

type LocalRoutingTab = "status" | "edit";

export function RoutingPage() {
  const toast = useToast();
  const queryClient = useQueryClient();
  const refreshEnabled = usePageRefreshEnabled();
  const [activeTab, setActiveTab] = useState<LocalRoutingTab>("status");
  const [proxyActionPending, setProxyActionPending] = useState(false);
  const workspaceQuery = useActivityQuery(refreshEnabled, localRoutingWorkspaceQueryOptions());
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
    active: refreshEnabled && activeTab === "status" && cooldownDeadlines.length > 0,
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

  useEffect(() => {
    if (error) toast.error("刷新本地路由状态失败", error);
  }, [error, toast]);

  useEffect(() => {
    const handleSettingsUpdated = () => {
      void queryClient.invalidateQueries({ queryKey: queryKeys.localRoutingWorkspace });
    };
    window.addEventListener(SETTINGS_UPDATED_EVENT, handleSettingsUpdated);
    return () => {
      window.removeEventListener(SETTINGS_UPDATED_EVENT, handleSettingsUpdated);
    };
  }, [queryClient]);

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
            onClick={() => void queryClient.invalidateQueries({ queryKey: queryKeys.localRoutingWorkspace })}
          >
            <RefreshCcw className="h-4 w-4" />
            刷新
          </Button>
        </div>
      }
    >
      {activeTab === "status" ? (
        <LocalRoutingStatusTab
          loading={loading}
          workspace={workspace}
          nowMs={nowMs}
          proxyActionPending={proxyActionPending}
          onToggleProxy={() => void handleToggleProxy()}
        />
      ) : (
        <LocalRoutingEditTab loading={loading} workspace={workspace} />
      )}
    </PageScaffold>
  );
}
