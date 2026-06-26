import { PageScaffold } from "@/components/shell/PageScaffold";
import { PlaceholderPanel } from "@/components/shell/PlaceholderPanel";

export function CollectorsPage() {
  return (
    <PageScaffold
      eyebrow="Collectors"
      title="Sub2API 采集"
      description="未来承载登录态采集、XHR 捕获、余额字段识别、分组和倍率快照。"
    >
      <PlaceholderPanel
        title="采集器占位"
        items={["登录状态", "捕获接口", "识别字段", "最近采集快照"]}
      />
    </PageScaffold>
  );
}
