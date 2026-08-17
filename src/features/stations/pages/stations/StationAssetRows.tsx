import type { DraggableAttributes } from "@dnd-kit/core";
import { useSortable, type AnimateLayoutChanges } from "@dnd-kit/sortable";
import { CSS } from "@dnd-kit/utilities";
import { Clock3, Edit3, GripVertical, RefreshCw, Route, ShieldCheck, Trash2 } from "lucide-react";
import { IconButton } from "@/components/ui";
import type { RoutingDeepLink } from "@/lib/types/routingDeepLinks";
import { stationTypeLabels, type Station } from "@/lib/types/stations";
import { cn } from "@/lib/utils";
import {
  formatRelativeTime,
  formatStationBalanceParts,
  formatStationDisplayUrl,
  stationAvatarLabel,
  stationIssueTagClassName,
} from "./displayModel";
import { stationIssueTags, type StationAssetRow, type StationIssueTag } from "../../stationAssetViewModels";

const shouldAnimateStationAssetLayoutChanges: AnimateLayoutChanges = ({ isSorting, wasDragging }) =>
  isSorting || wasDragging;

type StationAction = "collect" | "balance" | "authorize";
type StationRoutingDeepLink = Extract<RoutingDeepLink, { kind: "station" }> & {
  source: "station_endpoint_health";
};

export type StationAssetListRowProps = {
  row: StationAssetRow;
  active: boolean;
  actionDisabled: boolean;
  loadingAction: StationAction | null;
  overlay?: boolean;
  dragAttributes?: DraggableAttributes;
  dragListeners?: ReturnType<typeof useSortable>["listeners"];
  onOpen: (station: Station) => void;
  onEdit: (station: Station) => void;
  onAuthorize: (station: Station) => void;
  onCollect: (station: Station) => void;
  onDelete: (station: Station) => void;
  onOpenRoutingDeepLink?: (link: StationRoutingDeepLink) => void;
  onOpenWebsite: (station: Station) => void;
  onRefreshBalance: (station: Station) => void;
};

export function SortableStationAssetListRow(props: StationAssetListRowProps) {
  const { attributes, listeners, setNodeRef, transform, transition, isDragging } = useSortable({
    id: props.row.station.id,
    animateLayoutChanges: shouldAnimateStationAssetLayoutChanges,
  });

  return (
    <div
      ref={setNodeRef}
      style={{ transform: CSS.Transform.toString(transform), transition }}
      className={cn("will-change-transform", isDragging && "opacity-35")}
    >
      <StationAssetListRow
        {...props}
        dragAttributes={attributes}
        dragListeners={listeners}
      />
    </div>
  );
}

export function StationAssetListRow({
  row,
  active,
  actionDisabled,
  loadingAction,
  overlay = false,
  dragAttributes,
  dragListeners,
  onOpen,
  onEdit,
  onAuthorize,
  onCollect,
  onDelete,
  onOpenRoutingDeepLink,
  onOpenWebsite,
  onRefreshBalance,
}: StationAssetListRowProps) {
  const station = row.station;
  const issueTags = stationIssueTags(row);
  const balance = formatStationBalanceParts(row);
  const lastCollectText = formatRelativeTime(
    row.latestBalance?.updatedAt ?? row.latestBalance?.collectedAt ?? station.lastCheckedAt ?? station.updatedAt,
  );

  return (
    <div
      role="button"
      tabIndex={0}
      aria-pressed={active}
      className={cn(
        "group flex min-h-[78px] w-full cursor-pointer flex-wrap items-center gap-3 rounded-[14px] border px-4 py-3 text-left transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/30 md:flex-nowrap",
        active
          ? "border-primary/45 bg-selected"
          : "border-border bg-surface hover:border-info-border hover:bg-surface-subtle",
        overlay && "shadow-surface",
      )}
      onClick={() => onOpen(station)}
      onKeyDown={(event) => {
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          onOpen(station);
        }
      }}
    >
      <div
        className="shrink-0"
        onClick={(event) => event.stopPropagation()}
        onKeyDown={(event) => event.stopPropagation()}
      >
        <button
          type="button"
          aria-label={`拖拽排序 ${station.name}`}
          className="inline-flex h-7 w-5 cursor-grab items-center justify-center rounded-[6px] text-muted-foreground/45 transition-colors hover:bg-muted hover:text-muted-foreground active:cursor-grabbing focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/30"
          {...dragAttributes}
          {...dragListeners}
        >
          <GripVertical className="h-4 w-4" />
        </button>
      </div>
      <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-full border border-border bg-surface text-xs font-semibold text-muted-foreground shadow-surface">
        {stationAvatarLabel(station.name)}
      </div>

      <div className="min-w-0 flex-[1_1_calc(100%-5rem)] md:flex-1">
        <div className="flex min-w-0 items-center gap-2">
          <div className="truncate text-[15px] font-semibold leading-5 text-foreground">{station.name}</div>
          <span className="hidden rounded-full border border-border bg-surface/80 px-2 py-0.5 text-[11px] font-medium leading-4 text-muted-foreground sm:inline-flex">
            {stationTypeLabels[station.stationType]}
          </span>
          {issueTags.map((tag) => (
            <StationIssueTagBadge key={tag.kind} tag={tag} />
          ))}
        </div>
        <button
          type="button"
          aria-label={`在浏览器打开 ${station.name}`}
          title={station.websiteUrl}
          className="mt-1 block max-w-full truncate text-left text-xs font-medium text-primary hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/30"
          onClick={(event) => {
            event.stopPropagation();
            onOpenWebsite(station);
          }}
          onKeyDown={(event) => event.stopPropagation()}
        >
          {formatStationDisplayUrl(station.websiteUrl)}
        </button>
      </div>

      <div className="hidden shrink-0 items-center gap-5 md:flex">
        <div className="min-w-[78px] text-right">
          <div className="flex items-center justify-end gap-1 text-[11px] leading-4 text-muted-foreground/70">
            <Clock3 className="h-3 w-3" />
            <span>{lastCollectText}</span>
            <button
              type="button"
              aria-label={`刷新余额 ${station.name}`}
              title={`刷新余额 ${station.name}`}
              disabled={actionDisabled || !station.enabled}
              className="ml-0.5 inline-flex h-4 w-4 cursor-pointer items-center justify-center rounded-[5px] text-muted-foreground/70 transition-colors hover:bg-muted hover:text-muted-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring/30 disabled:cursor-default disabled:opacity-40"
              onClick={(event) => {
                event.stopPropagation();
                onRefreshBalance(station);
              }}
              onKeyDown={(event) => {
                event.stopPropagation();
              }}
            >
              <RefreshCw className={cn("h-3 w-3", loadingAction === "balance" && "animate-spin")} />
            </button>
          </div>
          <div className="mt-1 text-xs leading-4 text-muted-foreground">
            余额：
            <span className={cn("font-semibold", balance.amount === "未采集" ? "text-muted-foreground" : "text-success-foreground")}>
              {balance.amount}
            </span>
            {balance.currency && <span className="ml-1 text-muted-foreground">{balance.currency}</span>}
          </div>
        </div>
      </div>

      <div
        data-station-action-strip
        className="ml-auto flex shrink-0 items-center gap-1 opacity-100 transition-opacity md:opacity-0 md:group-hover:opacity-100 md:focus-within:opacity-100"
        onClick={(event) => event.stopPropagation()}
        onKeyDown={(event) => event.stopPropagation()}
      >
        <IconButton className="text-muted-foreground hover:text-foreground" label={`编辑 ${station.name}`} onClick={() => onEdit(station)}>
          <Edit3 className="h-4 w-4" />
        </IconButton>
        {supportsManualAuthorization(station) && (
          <IconButton
            className={cn(
              "text-muted-foreground hover:text-foreground",
              rowNeedsManualAuthorization(row) && "text-warning-foreground hover:bg-warning-surface hover:text-warning-foreground",
            )}
            disabled={actionDisabled || !station.enabled}
            label={`重新授权 ${station.name}`}
            onClick={() => onAuthorize(station)}
          >
            <ShieldCheck className={cn("h-4 w-4", loadingAction === "authorize" && "animate-pulse")} />
          </IconButton>
        )}
        <IconButton
          className="text-muted-foreground hover:text-foreground"
          disabled={actionDisabled || !station.enabled}
          label={`采集信息 ${station.name}`}
          onClick={() => onCollect(station)}
        >
          <RefreshCw className={cn("h-4 w-4", loadingAction === "collect" && "animate-spin")} />
        </IconButton>
        {onOpenRoutingDeepLink ? (
          <IconButton
            className="text-muted-foreground hover:bg-selected hover:text-primary"
            label={`查看路由影响 ${station.name}`}
            onClick={() =>
              onOpenRoutingDeepLink({
                kind: "station",
                stationId: station.id,
                source: "station_endpoint_health",
              })
            }
          >
            <Route className="h-4 w-4" />
          </IconButton>
        ) : null}
        <IconButton
          className="text-muted-foreground/70 hover:bg-danger-surface hover:text-danger-foreground"
          label={`删除 ${station.name}`}
          onClick={() => onDelete(station)}
        >
          <Trash2 className="h-4 w-4" />
        </IconButton>
      </div>
    </div>
  );
}

function supportsManualAuthorization(station: Station) {
  return station.stationType === "sub2api" || station.stationType === "newapi";
}

function rowNeedsManualAuthorization(row: StationAssetRow) {
  const summary = row.latestSnapshot?.summaryJson ?? {};
  return (
    row.latestSnapshot?.status === "manual_required" ||
    summary.loginRequired === true ||
    summary.loginStatus === "manual_required"
  );
}

function StationIssueTagBadge({ tag }: { tag: StationIssueTag }) {
  return (
    <span
      className="hidden sm:inline-flex"
      title={tag.title ?? tag.label}
      onKeyDown={(event) => event.stopPropagation()}
    >
      <span
        className={cn(
          "inline-flex rounded-full border px-2 py-0.5 text-[11px] font-medium leading-4",
          "transition-colors group-focus/tag:outline-none group-focus/tag:ring-2 group-focus/tag:ring-ring/30",
          stationIssueTagClassName(tag.tone),
        )}
      >
        {tag.label}
      </span>
    </span>
  );
}
