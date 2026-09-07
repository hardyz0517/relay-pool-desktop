import { CornerDownRight } from "lucide-react";
import { cn } from "@/lib/utils";

type ModelMappingDisplayProps = {
  requestedModel: string | null;
  resolvedModel: string | null;
  fallback?: string;
  className?: string;
  layout?: "stacked" | "inline";
};

export function ModelMappingDisplay({
  requestedModel,
  resolvedModel,
  fallback = "未识别",
  className,
  layout = "stacked",
}: ModelMappingDisplayProps) {
  const requested = requestedModel?.trim() || fallback;
  const resolved = resolvedModel?.trim();
  const mapped = Boolean(resolved && resolved !== requestedModel?.trim());
  const label = mapped ? `${requested} → ${resolved}` : requested;

  if (layout === "inline") {
    return (
      <div className={cn("min-w-0 truncate font-medium text-foreground", className)} title={label}>
        {mapped ? (
          <>
            {requested}
            <span className="mx-1 font-normal text-muted-foreground">→</span>
            {resolved}
          </>
        ) : (
          requested
        )}
      </div>
    );
  }

  return (
    <div className={cn("min-w-0", className)} title={label}>
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
