import type { ReactNode } from "react";
import type { StationGroupCategory } from "@/lib/groupCategories";
import type { StationGroupOption } from "@/lib/types/groupFacts";
import { cn } from "@/lib/utils";
import { formatMultiplier } from "../groupOptionViewModels";
import { groupVisualMetaFor } from "../groupVisualMeta";
import { groupVisualClassNames } from "../groupVisualStyles";
import { Sub2ApiPlatformIcon } from "./Sub2ApiPlatformIcon";

type StationGroupVisualInput = {
  groupName: string;
  rawJsonRedacted?: Record<string, unknown> | null;
  effectiveGroupCategory?: StationGroupCategory | null;
};

type StationGroupOptionLike = Pick<StationGroupOption, "groupName" | "rateMultiplier"> &
  Partial<Pick<StationGroupOption, "effectiveGroupCategory">> & {
    rawJsonRedacted?: Record<string, unknown> | null;
  };

export function StationGroupNameBadge({
  groupName,
  rawJsonRedacted = null,
  effectiveGroupCategory = null,
}: StationGroupVisualInput) {
  const visualMeta = groupVisualMetaFor(groupName, rawJsonRedacted, effectiveGroupCategory);
  const visualClassNames = groupVisualClassNames[visualMeta.platform];

  return (
    <span
      className={cn(
        "inline-flex h-6 max-w-full items-center gap-1.5 rounded-md border px-2 text-xs font-semibold",
        visualClassNames.badge,
      )}
      title={`${visualMeta.label} · ${groupName}`}
    >
      <Sub2ApiPlatformIcon platform={visualMeta.platform} className={visualClassNames.icon} />
      <span className="truncate">{groupName}</span>
    </span>
  );
}

export function StationGroupRateBadge({
  groupName,
  rawJsonRedacted = null,
  effectiveGroupCategory = null,
  rateMultiplier,
  label,
  fallback = "倍率未知",
}: StationGroupVisualInput & {
  rateMultiplier?: number | null;
  label?: string;
  fallback?: string;
}) {
  const visualMeta = groupVisualMetaFor(groupName, rawJsonRedacted, effectiveGroupCategory);
  const visualClassNames = groupVisualClassNames[visualMeta.platform];
  const rateLabel =
    label ??
    (rateMultiplier === null || rateMultiplier === undefined
      ? fallback
      : `${formatMultiplier(rateMultiplier)}x`);

  return (
    <span
      className={cn(
        "inline-flex h-6 shrink-0 items-center rounded-[calc(var(--surface-radius)-3px)] px-2 text-[11px] font-semibold leading-none",
        visualClassNames.rateBadge,
      )}
    >
      {rateLabel}
    </span>
  );
}

export function StationGroupInlineBadge({
  groupName,
  rawJsonRedacted = null,
  effectiveGroupCategory = null,
  rateMultiplier,
  label,
  fallback = "倍率未知",
}: StationGroupVisualInput & {
  rateMultiplier?: number | null;
  label?: string;
  fallback?: string;
}) {
  const visualMeta = groupVisualMetaFor(groupName, rawJsonRedacted, effectiveGroupCategory);
  const visualClassNames = groupVisualClassNames[visualMeta.platform];
  const rateLabel =
    label ??
    (rateMultiplier === null || rateMultiplier === undefined
      ? fallback
      : `${formatMultiplier(rateMultiplier)}x`);
  const inlineBadgeClassName = visualClassNames.rateBadge;
  const inlineIconClassName = visualClassNames.icon;

  return (
    <span
      className={cn(
        "inline-flex h-6 max-w-full items-center gap-2 rounded-md px-2 text-xs font-medium",
        inlineBadgeClassName,
      )}
      title={`${visualMeta.label} · ${groupName} · ${rateLabel}`}
    >
      <span className="inline-flex min-w-0 items-center gap-1.5">
        <Sub2ApiPlatformIcon platform={visualMeta.platform} className={inlineIconClassName} />
        <span className="truncate">{groupName}</span>
      </span>
      <span className="inline-flex h-5 shrink-0 items-center rounded-md bg-muted px-1.5 text-[10px] font-semibold leading-none">
        {rateLabel}
      </span>
    </span>
  );
}

export function StationGroupOptionLabel({
  option,
  suffix,
}: {
  option: StationGroupOptionLike;
  suffix?: ReactNode;
}) {
  const groupName = option.groupName || "当前绑定";

  return (
    <span className="inline-flex min-w-0 max-w-full items-center gap-1.5">
      <StationGroupInlineBadge
        groupName={groupName}
        rawJsonRedacted={option.rawJsonRedacted}
        effectiveGroupCategory={option.effectiveGroupCategory}
        rateMultiplier={option.rateMultiplier}
      />
      {suffix ? (
        <span className="shrink-0 text-[11px] font-medium text-muted-foreground">{suffix}</span>
      ) : null}
    </span>
  );
}
