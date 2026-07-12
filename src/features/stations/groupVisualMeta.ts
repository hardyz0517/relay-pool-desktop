import {
  effectiveGroupCategory,
  groupCategoryDefinitions,
  groupCategoryFromPlatform,
  type StationGroupCategory,
} from "@/lib/groupCategories";

export type StationGroupVisualPlatform = "anthropic" | "openai" | "gemini" | "grok" | "image" | "generic";

export type StationGroupVisualMeta = {
  platform: StationGroupVisualPlatform;
  label: string;
  badgeClassName: string;
  iconClassName: string;
  rateBadgeClassName: string;
};

const groupVisualMetaByPlatform: Record<StationGroupVisualPlatform, StationGroupVisualMeta> = {
  anthropic: {
    platform: "anthropic",
    label: "Claude",
    badgeClassName: "border-orange-100 bg-orange-100 text-orange-700",
    iconClassName: "text-orange-600",
    rateBadgeClassName: "bg-orange-50 text-orange-700",
  },
  openai: {
    platform: "openai",
    label: "OpenAI",
    badgeClassName: "border-emerald-100 bg-emerald-100 text-emerald-700",
    iconClassName: "text-emerald-600",
    rateBadgeClassName: "bg-emerald-50 text-emerald-700",
  },
  gemini: {
    platform: "gemini",
    label: "Gemini",
    badgeClassName: "border-blue-100 bg-blue-100 text-blue-700",
    iconClassName: "text-sky-600",
    rateBadgeClassName: "bg-sky-50 text-sky-700",
  },
  grok: {
    platform: "grok",
    label: "Grok",
    badgeClassName: "border-zinc-200 bg-zinc-200 text-zinc-800",
    iconClassName: "text-zinc-800",
    rateBadgeClassName: "bg-zinc-100 text-zinc-800",
  },
  image: {
    platform: "image",
    label: "生图",
    badgeClassName: "border-violet-100 bg-violet-100 text-violet-700",
    iconClassName: "text-violet-600",
    rateBadgeClassName: "bg-violet-50 text-violet-700",
  },
  generic: {
    platform: "generic",
    label: "分组",
    badgeClassName: "border-slate-200 bg-slate-50 text-slate-700",
    iconClassName: "text-slate-500",
    rateBadgeClassName: "bg-slate-100 text-slate-700",
  },
};

export function groupVisualMetaFor(
  _groupName: string,
  rawJsonRedacted?: Record<string, unknown> | null,
  groupCategory?: StationGroupCategory | null,
): StationGroupVisualMeta {
  const effectiveCategory = effectiveGroupCategory(platformCategoryFromGroupEvidence(rawJsonRedacted), groupCategory);
  const platform =
    groupCategoryDefinitions.find((definition) => definition.value === effectiveCategory)?.visualPlatform ??
    "generic";
  return groupVisualMetaByPlatform[platform];
}

function platformCategoryFromGroupEvidence(
  rawJsonRedacted?: Record<string, unknown> | null,
): StationGroupCategory | null {
  const platform = stringField(rawJsonRedacted, ["platform", "provider", "model_provider", "modelProvider"]);
  return groupCategoryFromPlatform(platform);
}

function stringField(value: unknown, keys: string[]): string | null {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return null;
  }
  const record = value as Record<string, unknown>;
  for (const key of keys) {
    const fieldValue = record[key];
    if (typeof fieldValue === "string" && fieldValue.trim()) {
      return fieldValue;
    }
  }
  return null;
}
