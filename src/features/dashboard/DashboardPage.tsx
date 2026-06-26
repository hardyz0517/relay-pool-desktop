import { PageScaffold } from "@/components/shell/PageScaffold";
import { PlaceholderPanel } from "@/components/shell/PlaceholderPanel";

export function DashboardPage() {
  return (
    <PageScaffold
      eyebrow="Overview"
      title="总览"
      description="展示本地代理状态、可用站点、余额告警、今日请求和最近活动。当前仅保留页面骨架。"
    >
      <PlaceholderPanel
        title="待接入模块"
        items={[
          "本地代理状态",
          "Base URL 与 Local Key",
          "当前路由策略",
          "余额告警与最近请求",
        ]}
      />
    </PageScaffold>
  );
}
