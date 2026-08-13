import type { DraggableAttributes } from "@dnd-kit/core";
import { useSortable } from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import { Activity, Edit3, GripVertical, KeyRound, Loader2, Route, Trash2 } from "lucide-react";
import { IconButton, StatusBadge, SwitchControl } from "@/components/ui";
import type { ChannelMonitor } from "@/lib/types/channelMonitors";
import type { KeyPoolItem } from "@/lib/types/stationKeys";
import { cn } from "@/lib/utils";
import type { KeyPoolMonitorStatus } from "./keyPoolMonitorStatus";

export const keyPoolTableClassName = "min-w-[62rem]";

export const keyPoolGridClassName =
  "grid w-full grid-cols-[2rem_minmax(14rem,1fr)_7rem_5rem_5rem_12rem_10.5rem] items-center gap-3";

export function SortableKeyRow({
  item,
  dragEnabled,
  testing,
  monitor,
  monitorStatus,
  monitoring,
  onEdit,
  onTestConnectivity,
  onToggleEnabled,
  onToggleMonitoring,
  onOpenRoutingImpact,
  onDelete,
}: {
  item: KeyPoolItem;
  dragEnabled: boolean;
  testing: boolean;
  monitor: ChannelMonitor | null;
  monitorStatus: KeyPoolMonitorStatus | null;
  monitoring: boolean;
  onEdit: (item: KeyPoolItem) => void;
  onTestConnectivity: (item: KeyPoolItem) => void;
  onToggleEnabled: (item: KeyPoolItem) => void;
  onToggleMonitoring: (item: KeyPoolItem) => void;
  onOpenRoutingImpact?: (item: KeyPoolItem) => void;
  onDelete: (item: KeyPoolItem) => void;
}) {
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } = useSortable({ id: item.id, disabled: !dragEnabled });
  return (
    <div ref={setNodeRef} style={{ transform: CSS.Transform.toString(transform), transition }} className={cn("w-full will-change-transform", isDragging && "opacity-35")}>
      <KeyRowContent item={item} testing={testing} monitor={monitor} monitorStatus={monitorStatus} monitoring={monitoring} dragAttributes={dragEnabled ? attributes : undefined} dragListeners={dragEnabled ? listeners : undefined} dragDisabled={!dragEnabled} onEdit={onEdit} onTestConnectivity={onTestConnectivity} onToggleEnabled={onToggleEnabled} onToggleMonitoring={onToggleMonitoring} onOpenRoutingImpact={onOpenRoutingImpact} onDelete={onDelete} />
    </div>
  );
}

export function KeyRowContent({
  item,
  overlay = false,
  testing = false,
  monitor,
  monitorStatus = null,
  monitoring = false,
  dragDisabled = false,
  dragAttributes,
  dragListeners,
  onEdit,
  onTestConnectivity,
  onToggleEnabled,
  onToggleMonitoring,
  onOpenRoutingImpact,
  onDelete,
}: {
  item: KeyPoolItem;
  overlay?: boolean;
  testing?: boolean;
  monitor?: ChannelMonitor | null;
  monitorStatus?: KeyPoolMonitorStatus | null;
  monitoring?: boolean;
  dragDisabled?: boolean;
  dragAttributes?: DraggableAttributes;
  dragListeners?: ReturnType<typeof useSortable>["listeners"];
  onEdit?: (item: KeyPoolItem) => void;
  onTestConnectivity?: (item: KeyPoolItem) => void;
  onToggleEnabled?: (item: KeyPoolItem) => void;
  onToggleMonitoring?: (item: KeyPoolItem) => void;
  onOpenRoutingImpact?: (item: KeyPoolItem) => void;
  onDelete?: (item: KeyPoolItem) => void;
}) {
  return (
    <div
      className={cn(
        keyPoolGridClassName,
        "group min-h-[66px] px-3 py-2.5 text-left transition-colors hover:bg-surface-subtle",
        overlay && "bg-surface-subtle",
      )}
    >
      <button
        type="button"
        aria-label="拖拽排序"
        title="拖拽排序"
        tabIndex={dragDisabled ? -1 : 0}
        disabled={dragDisabled}
        className={cn(
          "flex h-7 w-5 shrink-0 items-center justify-center text-muted-foreground/60",
          dragDisabled ? "cursor-not-allowed" : "cursor-grab hover:text-muted-foreground active:cursor-grabbing",
        )}
        {...dragAttributes}
        {...dragListeners}
      >
        <GripVertical className="h-4 w-4" />
      </button>

      <div className="flex min-w-0 items-center gap-2.5">
        <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-[9px] bg-muted text-muted-foreground">
          <KeyRound className="h-4 w-4" />
        </div>
        <div className="min-w-0">
          <div className="min-w-0 truncate text-[13px] font-semibold text-foreground">{item.name}</div>
          <div className="mt-0.5 truncate text-xs text-muted-foreground">{formatStationBaseUrl(item.stationApiBaseUrl)}</div>
        </div>
      </div>

      <div className="flex min-w-0 justify-center">
        {testing ? (
          <span
            className="inline-flex h-6 min-w-[4.75rem] items-center justify-center gap-1.5 rounded-full border border-primary bg-selected px-2 text-xs font-medium text-primary shadow-surface"
            aria-live="polite"
          >
            <Loader2 className="h-3.5 w-3.5 animate-spin motion-reduce:animate-none" />
            测试中
          </span>
        ) : monitorStatus ? (
          <StatusBadge tone={monitorStatus.tone}>{monitorStatus.label}</StatusBadge>
        ) : (
          <span aria-label={`未开启监控 ${item.name}`} className="text-sm text-muted-foreground">—</span>
        )}
      </div>

      <div className="flex min-w-0 items-center justify-center">
        <SwitchControl
          checked={item.enabled}
          ariaLabel={item.enabled ? `关闭调度 ${item.name}` : `打开调度 ${item.name}`}
          className="h-7 w-10 justify-center border-transparent bg-transparent px-0 shadow-none"
          disabled={overlay}
          onCheckedChange={() => onToggleEnabled?.(item)}
          showLabel={false}
        />
      </div>

      <div className="flex min-w-0 items-center justify-center">
        <SwitchControl
          checked={Boolean(monitor?.enabled)}
          ariaLabel={monitor?.enabled ? `关闭监控 ${item.name}` : `打开监控 ${item.name}`}
          className={cn(
            "h-7 w-10 justify-center border-transparent bg-transparent px-0 shadow-none",
            monitoring && "animate-pulse",
          )}
          disabled={overlay || monitoring}
          onCheckedChange={() => onToggleMonitoring?.(item)}
          showLabel={false}
        />
      </div>

      <div className="flex min-w-0 justify-center">
        <span className="inline-flex max-w-full items-center rounded-full bg-success-surface px-2 py-1 text-xs font-medium text-success-foreground ring-1 ring-success-border/40">
          <span className="truncate">{item.stationName}</span>
        </span>
      </div>

      <div
        className="flex shrink-0 items-center justify-end gap-3 md:opacity-0 md:transition-opacity md:group-hover:opacity-100 md:group-focus-within:opacity-100"
        onClick={(event) => event.stopPropagation()}
        onKeyDown={(event) => event.stopPropagation()}
      >
        <IconButton
          className={cn(
            "text-muted-foreground hover:bg-selected hover:text-primary",
            testing && "animate-pulse text-primary",
          )}
          disabled={overlay || testing || !item.apiKeyPresent}
          label={`测试连通性 ${item.name}`}
          onClick={() => onTestConnectivity?.(item)}
        >
          <Activity className="h-4 w-4" />
        </IconButton>
        {onOpenRoutingImpact ? (
          <IconButton
            className="text-muted-foreground hover:bg-info-surface hover:text-info-foreground"
            label={`查看路由影响 ${item.name}`}
            onClick={() => onOpenRoutingImpact(item)}
          >
            <Route className="h-4 w-4" />
          </IconButton>
        ) : null}
        <IconButton className="text-muted-foreground hover:bg-muted hover:text-foreground" label={`编辑 ${item.name}`} onClick={() => onEdit?.(item)}>
          <Edit3 className="h-4 w-4" />
        </IconButton>
        <IconButton className="text-muted-foreground hover:bg-danger-surface hover:text-danger-foreground" label={`删除 ${item.name}`} onClick={() => onDelete?.(item)}>
          <Trash2 className="h-4 w-4" />
        </IconButton>
      </div>
    </div>
  );
}

export function TableHeadCell({
  align = "start",
  children,
}: {
  align?: "start" | "center";
  children: string;
}) {
  return (
    <div
      className={cn(
        "min-w-0 truncate",
        align === "center" && "text-center",
      )}
    >
      {children}
    </div>
  );
}

export function formatStationBaseUrl(value: string) {
  try {
    const url = new URL(value);
    return `${url.protocol}//${url.host}`;
  } catch {
    return value.replace(/\/+$/, "");
  }
}
