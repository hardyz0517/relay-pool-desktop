import { useState } from "react";
import { RefreshCcw } from "lucide-react";
import { PageScaffold } from "@/components/shell/PageScaffold";
import { usePageActivation } from "@/components/shell/PageActivity";
import { Button, SegmentedControl, useToast } from "@/components/ui";
import { readError } from "@/lib/errors";
import { loadLocalRoutingWorkspace } from "@/lib/queries/localRoutingQueries";
import type { LocalRoutingWorkspace } from "@/lib/types/localRouting";
import type { AppRouteId } from "@/lib/types/navigation";
import { LocalRoutingEditTab } from "./LocalRoutingEditTab";
import { LocalRoutingStatusTab } from "./LocalRoutingStatusTab";

type LocalRoutingTab = "status" | "edit";
type LocalRoutingLinkedPage = Extract<AppRouteId, "channels" | "logs">;

type RoutingPageProps = {
  onOpenPage?: (pageId: LocalRoutingLinkedPage) => void;
};

export function RoutingPage({ onOpenPage }: RoutingPageProps) {
  const toast = useToast();
  const [activeTab, setActiveTab] = useState<LocalRoutingTab>("status");
  const [workspace, setWorkspace] = useState<LocalRoutingWorkspace | null>(null);
  const [loading, setLoading] = useState(true);

  usePageActivation(({ isInitial }) => {
    void refresh(isInitial);
  });

  async function refresh(showLoading = true) {
    if (showLoading) {
      setLoading(true);
    }
    try {
      setWorkspace(await loadLocalRoutingWorkspace());
    } catch (requestError) {
      setWorkspace(null);
      toast.error("刷新本地路由状态失败", readError(requestError));
    } finally {
      if (showLoading) {
        setLoading(false);
      }
    }
  }

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
        <LocalRoutingStatusTab loading={loading} workspace={workspace} onOpenPage={onOpenPage} />
      ) : (
        <LocalRoutingEditTab loading={loading} workspace={workspace} />
      )}
    </PageScaffold>
  );
}
