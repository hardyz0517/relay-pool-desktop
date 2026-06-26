import { PageScaffold } from "@/components/shell/PageScaffold";
import { PlaceholderPanel } from "@/components/shell/PlaceholderPanel";

export function RoutingPage() {
  return (
    <PageScaffold
      eyebrow="Routing"
      title="路由规则"
      description="未来支持手动排序优先、最低价优先、失败自动切换和简单模型固定路由。"
    >
      <PlaceholderPanel
        title="规则配置占位"
        items={["默认策略", "失败自动切换", "余额阈值", "熔断时间"]}
      />
    </PageScaffold>
  );
}
