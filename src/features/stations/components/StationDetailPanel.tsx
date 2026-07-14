import { Edit3, Power } from "lucide-react";
import { Button } from "@/components/ui/button";
import { KeyValueRow, MaskedSecret, SectionCard, StatusBadge } from "@/components/ui";
import {
  stationStatusLabels,
  stationTypeLabels,
  type Station,
} from "@/lib/types/stations";

type StationDetailPanelProps = {
  station: Station;
  onEdit: () => void;
  onDelete: () => void;
  onToggleEnabled: () => void;
};

const statusTone = {
  healthy: "healthy",
  warning: "warning",
  error: "error",
  disabled: "disabled",
  unchecked: "info",
} as const;

export function StationDetailPanel({
  station,
  onEdit,
  onDelete,
  onToggleEnabled,
}: StationDetailPanelProps) {
  return (
    <div className="space-y-4">
      <SectionCard
        title={station.name}
        action={
          <StatusBadge tone={statusTone[station.status]}>
            {stationStatusLabels[station.status]}
          </StatusBadge>
        }
      >
        <dl>
          <KeyValueRow label="站点类型" value={stationTypeLabels[station.stationType]} />
          <KeyValueRow label="前端网址" value={station.websiteUrl} />
          <KeyValueRow label="API Base URL" value={station.apiBaseUrl} />
          <KeyValueRow
            label="密钥"
            value={<MaskedSecret value={station.apiKeyMasked} present={station.apiKeyPresent} />}
          />
          <KeyValueRow
            label="启用状态"
            value={station.enabled ? "已启用" : "已禁用"}
          />
          <KeyValueRow
            label="余额"
            value={station.balanceCny === null ? "未采集" : `¥${station.balanceCny.toFixed(2)}`}
          />
          <KeyValueRow label="兑换比例" value={`1 元 = ${station.creditPerCny} 点`} />
          <KeyValueRow
            label="低余额阈值"
            value={
              station.lowBalanceThresholdCny === null
                ? "使用全局设置"
                : `¥${station.lowBalanceThresholdCny}`
            }
          />
          <KeyValueRow
            label="延迟"
            value={station.latencyMs ? `${station.latencyMs} ms` : "未检测"}
          />
          <KeyValueRow label="余额刷新" value={station.lastCheckedAt ?? "未检测"} />
          <KeyValueRow label="倍率采集" value={station.lastPricingFetchedAt ?? "未采集"} />
          <KeyValueRow label="备注" value={station.note ?? "无"} />
        </dl>
      </SectionCard>

      <div className="grid gap-4">
        <SectionCard title="采集状态" contentClassName="space-y-2">
          <KeyValueRow label="来源" value={station.lastPricingFetchedAt ? "信息采集快照" : "尚未采集"} />
          <KeyValueRow label="快照" value={station.lastPricingFetchedAt ?? "未采集"} />
        </SectionCard>

        <SectionCard title="健康状态" contentClassName="space-y-2">
          <KeyValueRow label="最近检测" value={station.lastCheckedAt ?? "未检测"} />
          <KeyValueRow label="最近错误" value={station.status === "error" ? "请查看渠道状态或使用记录" : "无"} />
        </SectionCard>
      </div>

      <SectionCard title="支持模型摘要">
        <div className="rounded-md border border-dashed border-border bg-surface-subtle px-3 py-2 text-xs text-muted-foreground">
          模型列表将在采集器或手动模型配置接入后持久化。
        </div>
      </SectionCard>

      <div className="flex flex-wrap gap-2">
        <Button variant="outline" onClick={onEdit}>
          <Edit3 className="h-4 w-4" />
          编辑
        </Button>
        <Button variant="outline" onClick={onToggleEnabled}>
          <Power className="h-4 w-4" />
          {station.enabled ? "禁用" : "启用"}
        </Button>
        <Button variant="outline" onClick={onDelete}>
          删除
        </Button>
      </div>
    </div>
  );
}
