import { PageScaffold } from "@/components/shell/PageScaffold";
import { PlaceholderPanel } from "@/components/shell/PlaceholderPanel";

export function LogsPage() {
  return (
    <PageScaffold
      eyebrow="Logs"
      title="请求日志"
      description="未来记录请求模型、实际站点、耗时、token、估算花费和 fallback 过程。"
    >
      <PlaceholderPanel
        title="日志表格占位"
        items={["时间", "模型", "站点", "状态与错误"]}
      />
    </PageScaffold>
  );
}
