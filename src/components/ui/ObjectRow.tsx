import type { ReactNode, Ref } from "react";
import { GripVertical } from "lucide-react";
import type { DraggableAttributes, DraggableSyntheticListeners } from "@dnd-kit/core";
import { cn } from "@/lib/utils";

export type ObjectRowMetric = {
  label: string;
  value: ReactNode;
  tone?: "neutral" | "good" | "warning" | "danger";
};

export type ObjectRowDragHandleProps = {
  attributes?: DraggableAttributes;
  listeners?: DraggableSyntheticListeners;
  ref?: Ref<HTMLButtonElement>;
  disabled?: boolean;
};

type ObjectRowProps = {
  icon?: ReactNode;
  title: ReactNode;
  subtitle?: ReactNode;
  badges?: ReactNode;
  metrics?: ObjectRowMetric[];
  actions?: ReactNode;
  selected?: boolean;
  draggable?: boolean;
  dragHandleProps?: ObjectRowDragHandleProps;
  className?: string;
  onClick?: () => void;
};

function RowContent({
  icon,
  title,
  subtitle,
  badges,
  metrics,
  actions,
  draggable,
  dragHandleProps,
}: ObjectRowProps) {
  const metricToneClassName: Record<
    NonNullable<ObjectRowMetric["tone"]>,
    string
  > = {
    neutral: "text-slate-700",
    good: "text-emerald-700",
    warning: "text-amber-700",
    danger: "text-rose-700",
  };

  return (
    <>
      {draggable && (
        <button
          type="button"
          aria-label="Drag row"
          title="Drag row"
          tabIndex={dragHandleProps?.disabled ? -1 : 0}
          disabled={dragHandleProps?.disabled}
          className={cn(
            "flex h-8 w-5 shrink-0 items-center justify-center text-slate-300",
            dragHandleProps?.disabled
              ? "cursor-not-allowed"
              : "cursor-grab active:cursor-grabbing hover:text-slate-500",
          )}
          ref={dragHandleProps?.ref}
          {...dragHandleProps?.attributes}
          {...dragHandleProps?.listeners}
        >
          <GripVertical className="h-4 w-4" />
        </button>
      )}
      {icon && (
        <div className="flex h-9 w-9 shrink-0 items-center justify-center rounded-[10px] bg-slate-100 text-slate-600">
          {icon}
        </div>
      )}
      <div className="min-w-0 flex-1">
        <div className="flex min-w-0 items-center gap-2">
          <div className="min-w-0 truncate text-[13px] font-semibold text-slate-800">
            {title}
          </div>
          {badges && (
            <div className="flex shrink-0 items-center gap-1">{badges}</div>
          )}
        </div>
        {subtitle && (
          <div className="mt-0.5 truncate text-xs text-muted-foreground">
            {subtitle}
          </div>
        )}
      </div>
      {metrics && metrics.length > 0 && (
        <div className="hidden shrink-0 items-center gap-4 sm:flex">
          {metrics.map(({ label, value, tone = "neutral" }) => (
            <div key={label} className="min-w-[72px] text-right">
              <div className="truncate text-[11px] text-muted-foreground">
                {label}
              </div>
              <div
                className={cn(
                  "truncate text-[13px] font-semibold",
                  metricToneClassName[tone],
                )}
              >
                {value}
              </div>
            </div>
          ))}
        </div>
      )}
      {actions && (
        <div className="flex shrink-0 items-center gap-1 md:opacity-0 md:transition-opacity md:group-hover:opacity-100 md:group-focus-within:opacity-100 md:group-focus-visible:opacity-100">
          {actions}
        </div>
      )}
    </>
  );
}

export function ObjectRow({
  selected = false,
  className,
    onClick,
    dragHandleProps,
    ...props
  }: ObjectRowProps) {
  const rowClassName = cn(
    "group flex min-h-[64px] w-full items-center gap-3 rounded-[var(--surface-radius)] border px-3 py-2 text-left transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[hsl(var(--accent)/0.35)]",
    selected
      ? "border-[hsl(var(--accent)/0.35)] bg-[hsl(var(--accent)/0.06)]"
      : "border-border bg-white hover:bg-slate-50",
    onClick && "cursor-pointer",
    className,
  );

  return (
    <div
      className={rowClassName}
      role={onClick ? "button" : undefined}
      tabIndex={onClick ? 0 : undefined}
      aria-pressed={onClick ? selected : undefined}
      onClick={(event) => {
        if (!onClick || event.target !== event.currentTarget) {
          return;
        }
        onClick();
      }}
      onKeyDown={(event) => {
        if (!onClick) {
          return;
        }
        if (event.currentTarget !== event.target) {
          return;
        }
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          onClick();
        }
      }}
    >
      <RowContent {...props} draggable={props.draggable} dragHandleProps={dragHandleProps} />
    </div>
  );
}
