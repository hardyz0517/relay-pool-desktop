import { cn } from "@/lib/utils";
import type { StationStatus } from "@/lib/types/stations";

type StationStatusDotProps = {
  status: StationStatus;
};

const statusClassName: Record<StationStatus, string> = {
  healthy: "bg-emerald-500",
  warning: "bg-amber-500",
  error: "bg-rose-500",
  disabled: "bg-slate-400",
  unchecked: "bg-blue-400",
};

export function StationStatusDot({ status }: StationStatusDotProps) {
  return (
    <span
      className={cn("inline-block h-2 w-2 rounded-full", statusClassName[status])}
    />
  );
}
