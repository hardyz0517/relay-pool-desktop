import { useEffect, useState } from "react";
import { RefreshCcw } from "lucide-react";
import { PageScaffold } from "@/components/shell/PageScaffold";
import { usePageActivity } from "@/components/shell/PageActivity";
import { SETTINGS_UPDATED_EVENT } from "@/lib/api/settings";
import { Button, SegmentedControl, useToast } from "@/components/ui";
import { readError } from "@/lib/errors";
import { queryKeys } from "@/lib/query/queryKeys";
import { localRoutingWorkspaceQueryOptions } from "@/lib/query/resourceQueries";
import { useActivityQuery } from "@/lib/query/useActivityQuery";
import { useQueryClient } from "@tanstack/react-query";
import { LocalRoutingEditTab } from "./LocalRoutingEditTab";
import { LocalRoutingStatusTab } from "./LocalRoutingStatusTab";

type LocalRoutingTab = "status" | "edit";

export function RoutingPage() {
  const toast = useToast();
  const queryClient = useQueryClient();
  const { refreshEnabled } = usePageActivity();
  const [activeTab, setActiveTab] = useState<LocalRoutingTab>("status");
  const workspaceQuery = useActivityQuery(refreshEnabled, localRoutingWorkspaceQueryOptions());
  const workspace = workspaceQuery.data ?? null;
  const loading = workspaceQuery.isPending && workspaceQuery.data === undefined;
  const error = workspaceQuery.error ? readError(workspaceQuery.error) : null;

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
          <Button disabled={loading} variant="secondary" onClick={() => void queryClient.invalidateQueries({ queryKey: queryKeys.localRoutingWorkspace })}>
            <RefreshCcw className="h-4 w-4" />
            刷新
          </Button>
        </div>
      }
    >
      {activeTab === "status" ? (
        <LocalRoutingStatusTab loading={loading} workspace={workspace} />
      ) : (
        <LocalRoutingEditTab loading={loading} workspace={workspace} />
      )}
    </PageScaffold>
  );
}
