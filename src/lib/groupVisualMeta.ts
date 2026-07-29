import {
  effectiveGroupCategory,
  groupCategoryDefinitions,
  groupCategoryFromPlatform,
  inferGroupCategoryFromEvidence,
  type StationGroupCategory,
} from "@/lib/groupCategories";

export type StationGroupVisualPlatform = "anthropic" | "openai" | "gemini" | "grok" | "image" | "generic";

export type StationGroupVisualMeta = {
  platform: StationGroupVisualPlatform;
  label: string;
};

const groupVisualMetaByPlatform: Record<StationGroupVisualPlatform, StationGroupVisualMeta> = {
  anthropic: {
    platform: "anthropic",
    label: "Claude",
  },
  openai: {
    platform: "openai",
    label: "OpenAI",
  },
  gemini: {
    platform: "gemini",
    label: "Gemini",
  },
  grok: {
    platform: "grok",
    label: "Grok",
  },
  image: {
    platform: "image",
    label: "生图",
  },
  generic: {
    platform: "generic",
    label: "分组",
  },
};

export function groupVisualMetaFor(
  groupName: string,
  rawJsonRedacted?: Record<string, unknown> | null,
  groupCategory?: StationGroupCategory | null,
): StationGroupVisualMeta {
  const effectiveCategory = effectiveGroupCategory(
    inferGroupCategoryFromEvidence({ groupName, rawJsonRedacted }),
    groupCategory ?? platformCategoryFromGroupEvidence(rawJsonRedacted),
  );
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
