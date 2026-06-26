import { GripVertical } from "lucide-react";
import { StationStatusDot } from "./StationStatusDot";
import {
  stationStatusLabels,
  stationTypeLabels,
  type Station,
} from "@/lib/types/stations";
import { cn } from "@/lib/utils";

type StationListItemProps = {
  station: Station;
  active?: boolean;
  onSelect: () => void;
};

export function StationListItem({ station, active, onSelect }: StationListItemProps) {
  return (
    <button
      type="button"
      onClick={onSelect}
      className={cn(
        "w-full rounded-lg border px-3 py-2 text-left transition-colors",
        active
          ? "border-blue-200 bg-blue-50/80"
          : "border-border bg-white hover:bg-slate-50",
      )}
    >
      <div className="flex items-start gap-2">
        <GripVertical className="mt-0.5 h-4 w-4 shrink-0 text-slate-300" />
        <div className="min-w-0 flex-1">
          <div className="flex items-center justify-between gap-2">
            <div className="truncate text-sm font-medium text-slate-800">
              {station.name}
            </div>
            <StationStatusDot status={station.status} />
          </div>
          <div className="mt-1 flex items-center gap-2 text-xs text-muted-foreground">
            <span>{stationTypeLabels[station.stationType]}</span>
            <span>
              {station.balanceCny === null ? "余额未采集" : `¥${station.balanceCny.toFixed(2)}`}
            </span>
          </div>
          <div className="mt-2 grid grid-cols-2 gap-2 text-xs text-muted-foreground">
            <span>{station.latencyMs ? `${station.latencyMs} ms` : "--"}</span>
            <span>{station.lastCheckedAt ?? "未检测"}</span>
            <span>{stationStatusLabels[station.status]}</span>
            <span>{station.enabled ? "已启用" : "已禁用"}</span>
          </div>
        </div>
      </div>
    </button>
  );
}
