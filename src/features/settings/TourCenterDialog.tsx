import { useState } from "react";
import { Check, CircleHelp, Clock3, RotateCcw } from "lucide-react";
import { Button, ConfirmDialog, Dialog, StatusBadge } from "@/components/ui";
import type { PublishedTourId, TourDefinition, TourProgressEntry, TourProgressV1 } from "@/app/tours/tourTypes";

type TourCenterDialogProps = {
  open: boolean;
  tours: readonly TourDefinition<PublishedTourId>[];
  progress: TourProgressV1;
  onClose: () => void;
  onExited?: () => void;
  onStart: (tourId: PublishedTourId) => void;
  onReset: () => void;
};

export function TourCenterDialog({
  open,
  tours,
  progress,
  onClose,
  onExited,
  onStart,
  onReset,
}: TourCenterDialogProps) {
  const [confirmReset, setConfirmReset] = useState(false);

  return (
    <>
      <Dialog
        open={open}
        title="使用教程"
        description="完整浏览产品结构，或针对当前工作区查看详细说明。"
        onClose={onClose}
        onExited={onExited}
        className="max-w-[680px]"
        footer={
          <div className="flex items-center justify-between gap-3">
            <Button type="button" variant="ghost" onClick={() => setConfirmReset(true)}>
              <RotateCcw className="h-4 w-4" />
              重置进度
            </Button>
            <Button type="button" variant="outline" onClick={onClose}>关闭</Button>
          </div>
        }
      >
        <div className="grid gap-5 p-4">
          {TOUR_GROUPS.map((group) => {
            const groupedTours = tours
              .filter((tour) => tour.category === group.category)
              .sort((left, right) => left.order - right.order);
            if (groupedTours.length === 0) return null;

            return (
              <section key={group.category} aria-labelledby={`tour-group-${group.category}`}>
                <h3 id={`tour-group-${group.category}`} className="mb-2 text-xs font-semibold text-muted-foreground">
                  {group.label}
                </h3>
                <div className="divide-y divide-border border-y border-border">
                  {groupedTours.map((tour) => {
                    const entry = progress.tours[tour.id];
                    const status = getTourStatus(entry, tour);
                    const actionLabel = entry ? "重新查看" : "开始";

                    return (
                      <div
                        key={tour.id}
                        data-tour-center-id={tour.id}
                        className="grid min-w-0 grid-cols-[auto_minmax(0,1fr)] items-center gap-x-3 gap-y-2 py-3 sm:grid-cols-[auto_minmax(0,1fr)_auto]"
                      >
                        <div className="flex h-8 w-8 items-center justify-center rounded-[var(--surface-radius)] bg-muted text-muted-foreground">
                          {status.label === "已完成" ? <Check className="h-4 w-4" /> : <CircleHelp className="h-4 w-4" />}
                        </div>
                        <div className="min-w-0">
                          <div className="flex min-w-0 flex-wrap items-center gap-2">
                            <div className="break-words text-sm font-medium text-foreground">{tour.title}</div>
                            <StatusBadge tone={status.tone}>{status.label}</StatusBadge>
                            {tour.estimatedMinutes ? (
                              <span className="inline-flex items-center gap-1 text-xs text-muted-foreground">
                                <Clock3 className="h-3.5 w-3.5" />
                                约 {tour.estimatedMinutes} 分钟
                              </span>
                            ) : null}
                          </div>
                          <div className="mt-1 break-words text-xs leading-5 text-muted-foreground">{tour.summary}</div>
                        </div>
                        <Button
                          type="button"
                          variant="outline"
                          className="col-start-2 justify-self-start sm:col-start-3 sm:row-start-1 sm:row-span-2 sm:justify-self-end"
                          onClick={() => onStart(tour.id)}
                        >
                          {actionLabel}
                        </Button>
                      </div>
                    );
                  })}
                </div>
              </section>
            );
          })}
        </div>
      </Dialog>
      <ConfirmDialog
        open={confirmReset}
        title="重置教程进度"
        description="重置后，教程会重新显示为新增状态；不会影响任何业务数据。"
        confirmLabel="重置"
        onCancel={() => setConfirmReset(false)}
        onConfirm={() => {
          setConfirmReset(false);
          onReset();
        }}
      />
    </>
  );
}

const TOUR_GROUPS = [
  { category: "recommended", label: "推荐" },
  { category: "page", label: "页面教程" },
] as const;

function getTourStatus(
  entry: TourProgressEntry | undefined,
  definition: TourDefinition<PublishedTourId>,
) {
  if (!entry) return { label: "新增", tone: "info" as const };
  if (entry.revision < definition.revision) return { label: "有更新", tone: "warning" as const };
  if (entry.state === "completed") return { label: "已完成", tone: "healthy" as const };
  return { label: "未完成", tone: "disabled" as const };
}
