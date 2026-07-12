import { useCallback, useEffect, useRef, useState } from "react";
import { RefreshCcw } from "lucide-react";
import { PageScaffold } from "@/components/shell/PageScaffold";
import { usePageActivation } from "@/components/shell/PageActivity";
import { SETTINGS_UPDATED_EVENT } from "@/lib/api/settings";
import { Button, SegmentedControl, useToast } from "@/components/ui";
import { readError } from "@/lib/errors";
import { loadLocalRoutingWorkspace } from "@/lib/queries/localRoutingQueries";
import type { LocalRoutingWorkspace } from "@/lib/types/localRouting";
import { LocalRoutingEditTab } from "./LocalRoutingEditTab";
import { LocalRoutingStatusTab } from "./LocalRoutingStatusTab";

type LocalRoutingTab = "status" | "edit";

export function RoutingPage() {
  const toast = useToast();
  const [activeTab, setActiveTab] = useState<LocalRoutingTab>("status");
  const [workspace, setWorkspace] = useState<LocalRoutingWorkspace | null>(null);
  const [loading, setLoading] = useState(true);
  const refreshOperationRef = useRef(0);

  const refresh = useCallback(async (showLoading = true) => {
    const operationId = refreshOperationRef.current + 1;
    refreshOperationRef.current = operationId;
    if (showLoading) {
      setLoading(true);
    }
    try {
      const nextWorkspace = await loadLocalRoutingWorkspace();
      if (operationId !== refreshOperationRef.current) {
        return;
      }
      setWorkspace(nextWorkspace);
    } catch (requestError) {
      if (operationId !== refreshOperationRef.current) {
        return;
      }
      setWorkspace(null);
      toast.error("刷新本地路由状态失败", readError(requestError));
    } finally {
      if (operationId === refreshOperationRef.current && showLoading) {
        setLoading(false);
      }
    }
  }, [toast]);

  usePageActivation(({ isInitial }) => {
    void refresh(isInitial);
  });

  useEffect(() => {
    const handleSettingsUpdated = () => {
      void refresh(false);
    };
    window.addEventListener(SETTINGS_UPDATED_EVENT, handleSettingsUpdated);
    return () => {
      window.removeEventListener(SETTINGS_UPDATED_EVENT, handleSettingsUpdated);
    };
  }, [refresh]);

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
          <Button disabled={loading} variant="secondary" onClick={() => void refresh()}>
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
