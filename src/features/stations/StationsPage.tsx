import { PageScaffold } from "@/components/shell/PageScaffold";
import { PlaceholderPanel } from "@/components/shell/PlaceholderPanel";

export function StationsPage() {
  return (
    <PageScaffold
      eyebrow="Stations"
      title="中转池"
      description="未来用于管理 Sub2API / NewAPI / OpenAI-compatible 站点、启用状态和拖拽优先级。"
    >
      <PlaceholderPanel
        title="站点管理占位"
        items={["站点列表", "站点详情", "余额信息", "健康检测状态"]}
      />
    </PageScaffold>
  );
}
