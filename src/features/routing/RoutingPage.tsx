import { Plus } from "lucide-react";
import { PageScaffold } from "@/components/shell/PageScaffold";
import {
  Button,
  InspectorPanel,
  PropertyList,
  PropertyRow,
  SectionCard,
  SegmentedControl,
  StatusBadge,
} from "@/components/ui";
import { mockRoutingSettings, routeStrategyLabels } from "@/lib/mock";

export function RoutingPage() {
  const routing = mockRoutingSettings;

  return (
      <PageScaffold title="路由规则" description="静态规则设置 UI；后续路由将基于 Key 池中的 station key，而不是中转站账号本身。">
      <div className="grid gap-3 xl:grid-cols-[minmax(0,1fr)_360px]">
        <SectionCard title="策略设置" description="规则形态先对齐，真实路由留到后续阶段。">
          <div className="space-y-4">
            <div>
              <div className="mb-2 text-xs font-medium text-muted-foreground">默认策略</div>
              <SegmentedControl
                value={routing.defaultStrategy}
                options={(["manual", "cheapest", "stable"] as const).map((value) => ({
                  value,
                  label: routeStrategyLabels[value],
                }))}
              />
            </div>
            <PropertyList className="overflow-hidden rounded-2xl border border-cyan-100 bg-white/75">
              <PropertyRow
                label="失败自动切换"
                description="上游失败后选择下一个候选站点"
                value={
                  <StatusBadge tone={routing.fallbackEnabled ? "healthy" : "disabled"}>
                    {routing.fallbackEnabled ? "已开启" : "已关闭"}
                  </StatusBadge>
                }
              />
              <PropertyRow label="低余额停用" description="余额低于阈值时不参与路由" value={`¥${routing.lowBalanceThresholdCny}`} />
              <PropertyRow label="熔断时间" description="失败站点冷却时间" value={`${routing.circuitBreakerMinutes} 分钟`} />
              <PropertyRow label="健康缓存" description="健康状态缓存时长" value={`${routing.healthCacheSeconds} 秒`} />
            </PropertyList>
          </div>
        </SectionCard>

        <InspectorPanel
          title="模型固定路由"
          description="紧凑列表占位；不保存。"
          actions={
            <Button variant="secondary">
              <Plus className="h-4 w-4" />
              添加
            </Button>
          }
        >
          <div className="divide-y divide-cyan-100">
            {routing.modelOverrides.map((override) => (
              <div key={override.model} className="px-4 py-3">
                <div className="text-sm font-semibold text-slate-800">{override.model}</div>
                <div className="mt-1 text-xs text-muted-foreground">
                  {override.stationName} · {override.reason}
                </div>
              </div>
            ))}
          </div>
        </InspectorPanel>
      </div>
    </PageScaffold>
  );
}
