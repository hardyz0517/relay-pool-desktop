import { cn } from "@/lib/utils";
import type { MockStationStatus } from "@/lib/mock";

type StationStatusDotProps = {
  status: MockStationStatus;
};

const statusClassName: Record<MockStationStatus, string> = {
  healthy: "bg-emerald-500",
  warning: "bg-amber-500",
  error: "bg-rose-500",
  disabled: "bg-slate-400",
};

export function StationStatusDot({ status }: StationStatusDotProps) {
  return (
    <span
      className={cn("inline-block h-2 w-2 rounded-full", statusClassName[status])}
    />
  );
}
