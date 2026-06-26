import { Activity, Edit3, Power, RefreshCcw, Wifi } from "lucide-react";
import { Button } from "@/components/ui/button";
import { KeyValueRow, SectionCard, StatusBadge } from "@/components/ui";
import {
  stationStatusLabels,
  stationTypeLabels,
  type MockStation,
} from "@/lib/mock";

type StationDetailPanelProps = {
  station: MockStation;
};

const statusTone = {
  healthy: "healthy",
  warning: "warning",
  error: "error",
  disabled: "disabled",
} as const;

export function StationDetailPanel({ station }: StationDetailPanelProps) {
  return (
    <div className="space-y-4">
      <SectionCard
        title={station.name}
        description="站点详情为 Phase 1 假数据，仅展示未来管理入口。"
        action={
          <StatusBadge tone={statusTone[station.status]}>
            {stationStatusLabels[station.status]}
          </StatusBadge>
        }
      >
        <dl>
          <KeyValueRow label="站点类型" value={stationTypeLabels[station.type]} />
          <KeyValueRow label="连接摘要" value={station.baseUrlHost} />
          <KeyValueRow
            label="启用状态"
            value={station.enabled ? "已启用" : "已禁用"}
          />
          <KeyValueRow label="余额" value={`¥${station.balanceCny.toFixed(2)}`} />
          <KeyValueRow
            label="延迟"
            value={station.latencyMs > 0 ? `${station.latencyMs} ms` : "未检测"}
          />
          <KeyValueRow label="余额刷新" value={station.lastCheckedAt} />
          <KeyValueRow label="倍率采集" value={station.lastPricingFetchedAt} />
        </dl>
      </SectionCard>

      <div className="grid gap-4 lg:grid-cols-2">
        <SectionCard title="采集状态" contentClassName="space-y-2">
          <KeyValueRow label="来源" value={station.collectorSource} />
          <KeyValueRow label="快照" value={station.lastPricingFetchedAt} />
        </SectionCard>

        <SectionCard title="健康状态" contentClassName="space-y-2">
          <KeyValueRow label="最近检测" value={station.lastCheckedAt} />
          <KeyValueRow
            label="最近错误"
            value={station.recentError ?? "暂无错误"}
          />
        </SectionCard>
      </div>

      <SectionCard title="支持模型摘要">
        <div className="flex flex-wrap gap-2">
          {station.supportedModels.map((model) => (
            <span
              key={model}
              className="rounded-md border border-border bg-slate-50 px-2 py-1 text-xs text-slate-700"
            >
              {model}
            </span>
          ))}
        </div>
      </SectionCard>

      <div className="flex flex-wrap gap-2">
        <Button variant="outline">
          <Wifi className="h-4 w-4" />
          测试连接
        </Button>
        <Button variant="outline">
          <RefreshCcw className="h-4 w-4" />
          刷新余额
        </Button>
        <Button variant="outline">
          <Activity className="h-4 w-4" />
          刷新倍率
        </Button>
        <Button variant="outline">
          <Edit3 className="h-4 w-4" />
          编辑
        </Button>
        <Button variant="outline">
          <Power className="h-4 w-4" />
          禁用
        </Button>
      </div>
    </div>
  );
}
