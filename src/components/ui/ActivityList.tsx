import type { ReactNode } from "react";

type ActivityListProps = {
  children: ReactNode;
};

type ActivityItemProps = {
  title: ReactNode;
  meta?: ReactNode;
  detail?: ReactNode;
  marker?: ReactNode;
};

export function ActivityList({ children }: ActivityListProps) {
  return <div className="divide-y divide-border">{children}</div>;
}

export function ActivityItem({ title, meta, detail, marker }: ActivityItemProps) {
  return (
    <div className="grid grid-cols-[minmax(0,1fr)_auto] gap-3 px-3 py-2 text-[13px]">
      <div className="min-w-0">
        <div className="flex min-w-0 items-center gap-2">
          {marker}
          <div className="truncate font-medium text-foreground">{title}</div>
        </div>
        {detail && (
          <div className="mt-0.5 truncate text-xs text-muted-foreground">{detail}</div>
        )}
      </div>
      {meta && <div className="shrink-0 text-xs text-muted-foreground">{meta}</div>}
    </div>
  );
}
