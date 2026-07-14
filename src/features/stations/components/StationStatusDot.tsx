import { cn } from "@/lib/utils";
import type { StationStatus } from "@/lib/types/stations";

type StationStatusDotProps = {
  status: StationStatus;
};

const statusClassName: Record<StationStatus, string> = {
  healthy: "bg-success-foreground",
  warning: "bg-warning-foreground",
  error: "bg-danger-solid",
  disabled: "bg-muted-foreground",
  unchecked: "bg-info-foreground",
};

export function StationStatusDot({ status }: StationStatusDotProps) {
  return (
    <span
      className={cn("inline-block h-2 w-2 rounded-full", statusClassName[status])}
    />
  );
}
