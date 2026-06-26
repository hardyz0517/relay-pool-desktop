import { PageScaffold } from "@/components/shell/PageScaffold";
import { PlaceholderPanel } from "@/components/shell/PlaceholderPanel";

export function PricingPage() {
  return (
    <PageScaffold
      eyebrow="Pricing"
      title="价格表"
      description="未来将模型倍率、分组倍率和充值比例统一换算为人民币 token 成本。"
    >
      <PlaceholderPanel
        title="价格归一化占位"
        items={["模型", "推荐站点", "输入价格", "输出价格"]}
      />
    </PageScaffold>
  );
}
