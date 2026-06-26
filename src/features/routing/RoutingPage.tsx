import { PageScaffold } from "@/components/shell/PageScaffold";
import { Button } from "@/components/ui/button";
import { KeyValueRow, SectionCard, StatusBadge } from "@/components/ui";
import { mockRoutingSettings, routeStrategyLabels } from "@/lib/mock";

export function RoutingPage() {
  const routing = mockRoutingSettings;

  return (
    <PageScaffold
      eyebrow="Routing"
      title="路由规则"
      description="静态规则表单 UI：展示策略、fallback、阈值和模型固定路由，不保存设置。"
    >
      <div className="grid gap-4 xl:grid-cols-[minmax(0,1fr)_420px]">
        <SectionCard title="默认策略" description="Phase 1 只做视觉切换，不保存。">
          <div className="grid gap-2 sm:grid-cols-3">
            {(["manual", "cheapest", "stable"] as const).map((strategy) => (
              <button
                key={strategy}
                type="button"
                className={
                  strategy === routing.defaultStrategy
                    ? "rounded-md border border-blue-200 bg-blue-50 px-3 py-2 text-sm font-medium text-blue-700"
                    : "rounded-md border border-border bg-white px-3 py-2 text-sm text-slate-600 hover:bg-slate-50"
                }
              >
                {routeStrategyLabels[strategy]}
              </button>
            ))}
          </div>
        </SectionCard>

        <SectionCard title="运行开关">
          <dl>
            <KeyValueRow
              label="失败自动切换"
              value={
                <StatusBadge tone={routing.fallbackEnabled ? "healthy" : "disabled"}>
                  {routing.fallbackEnabled ? "已开启" : "已关闭"}
                </StatusBadge>
              }
            />
            <KeyValueRow
              label="低余额停用"
              value={`低于 ¥${routing.lowBalanceThresholdCny}`}
            />
            <KeyValueRow
              label="熔断时间"
              value={`${routing.circuitBreakerMinutes} 分钟`}
            />
            <KeyValueRow
              label="健康缓存"
              value={`${routing.healthCacheSeconds} 秒`}
            />
          </dl>
        </SectionCard>

        <SectionCard
          title="模型固定路由"
          description="未来可按模型指定站点，当前仅展示假数据。"
          action={<Button variant="outline">添加规则</Button>}
          className="xl:col-span-2"
        >
          <div className="grid gap-2">
            {routing.modelOverrides.map((override) => (
              <div
                key={override.model}
                className="grid gap-2 rounded-md border border-border bg-slate-50 px-3 py-2 text-sm md:grid-cols-[1fr_1fr_1.4fr_auto]"
              >
                <span className="font-medium text-slate-800">{override.model}</span>
                <span>{override.stationName}</span>
                <span className="text-muted-foreground">{override.reason}</span>
                <Button variant="ghost" className="h-7 px-2 text-xs">
                  移除
                </Button>
              </div>
            ))}
          </div>
        </SectionCard>
      </div>
    </PageScaffold>
  );
}
