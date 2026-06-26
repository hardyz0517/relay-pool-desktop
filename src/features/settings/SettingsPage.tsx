import { PageScaffold } from "@/components/shell/PageScaffold";
import { PlaceholderPanel } from "@/components/shell/PlaceholderPanel";

export function SettingsPage() {
  return (
    <PageScaffold
      eyebrow="Settings"
      title="设置"
      description="未来配置本地代理端口、Local Key、数据目录、采集频率、导入导出和托盘行为。"
    >
      <PlaceholderPanel
        title="本地设置占位"
        items={["代理端口", "Local Key", "数据目录", "外观主题"]}
      />
    </PageScaffold>
  );
}
