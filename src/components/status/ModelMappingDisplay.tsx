import { CornerDownRight } from "lucide-react";
import { cn } from "@/lib/utils";

type ModelMappingDisplayProps = {
  requestedModel: string | null;
  resolvedModel: string | null;
  fallback?: string;
  className?: string;
};

export function ModelMappingDisplay({
  requestedModel,
  resolvedModel,
  fallback = "未识别",
  className,
}: ModelMappingDisplayProps) {
  const requested = requestedModel?.trim() || fallback;
  const resolved = resolvedModel?.trim();
  const mapped = Boolean(resolved && resolved !== requestedModel?.trim());

  return (
    <div className={cn("min-w-0", className)} title={mapped ? `${requested} -> ${resolved}` : requested}>
      <div className="truncate font-medium text-foreground">{requested}</div>
      {mapped ? (
        <div className="mt-0.5 flex min-w-0 items-center gap-1 pl-2 text-[11px] leading-4 text-muted-foreground">
          <CornerDownRight className="h-3 w-3 shrink-0" aria-hidden="true" />
          <span className="truncate">{resolved}</span>
        </div>
      ) : null}
    </div>
  );
}
