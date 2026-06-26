import { PageScaffold } from "@/components/shell/PageScaffold";
import { Button } from "@/components/ui/button";
import { KeyValueRow, MaskedSecret, SectionCard, StatusBadge } from "@/components/ui";
import { mockSettings } from "@/lib/mock";

export function SettingsPage() {
  return (
    <PageScaffold
      eyebrow="Settings"
      title="设置"
      description="静态本地设置表单；不保存、不调用 Tauri commands。"
    >
      <div className="grid gap-4 xl:grid-cols-2">
        <SectionCard
          title="本地代理"
          description="未来用于生成 CCSwitch provider 配置。"
          action={<Button variant="outline">复制配置</Button>}
        >
          <dl>
            <KeyValueRow label="代理端口" value={mockSettings.proxyPort} />
            <KeyValueRow
              label="Base URL"
              value={`http://127.0.0.1:${mockSettings.proxyPort}/v1`}
            />
            <KeyValueRow
              label="Local Key"
              value={<MaskedSecret value={mockSettings.maskedLocalKey} />}
            />
          </dl>
          <div className="mt-3 flex flex-wrap gap-2">
            <Button variant="outline">复制 Local Key</Button>
            <Button variant="outline">重新生成</Button>
          </div>
        </SectionCard>

        <SectionCard title="采集与余额">
          <dl>
            <KeyValueRow
              label="采集频率"
              value={`${mockSettings.collectionIntervalMinutes} 分钟`}
            />
            <KeyValueRow
              label="低余额阈值"
              value={`¥${mockSettings.lowBalanceThresholdCny}`}
            />
            <KeyValueRow label="数据目录" value={mockSettings.dataDir} />
          </dl>
        </SectionCard>

        <SectionCard title="托盘与外观">
          <dl>
            <KeyValueRow
              label="托盘行为"
              value={
                mockSettings.trayBehavior === "minimize-to-tray"
                  ? "最小化到托盘"
                  : mockSettings.trayBehavior === "close-to-tray"
                    ? "关闭到托盘"
                    : "禁用"
              }
            />
            <KeyValueRow
              label="主题"
              value={
                <div className="flex flex-col gap-1">
                  <StatusBadge tone="info">默认浅色</StatusBadge>
                  <span className="text-xs text-muted-foreground">
                    {mockSettings.themeNote}
                  </span>
                </div>
              }
            />
          </dl>
        </SectionCard>

        <SectionCard
          title="导入 / 导出"
          description="Phase 1 只保留入口，不读取或写入本地配置。"
        >
          <div className="flex flex-wrap gap-2">
            <Button variant="outline">导入配置</Button>
            <Button variant="outline">导出配置</Button>
            <Button variant="outline">打开数据目录</Button>
          </div>
          <div className="mt-3 rounded-md border border-border bg-slate-50 px-3 py-2 text-xs text-muted-foreground">
            不提交 key、cookie、日志、本地数据库或用户本地数据。
          </div>
        </SectionCard>
      </div>
    </PageScaffold>
  );
}
