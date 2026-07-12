export type StationGroupCategory =
  | "gpt"
  | "claude"
  | "gemini"
  | "grok"
  | "image_generation"
  | "embedding"
  | "rerank"
  | "unknown";

export type StationGroupCategoryDefinition = {
  value: StationGroupCategory;
  label: string;
  visualPlatform: "anthropic" | "openai" | "gemini" | "grok" | "image" | "generic";
};

export const groupCategoryDefinitions: StationGroupCategoryDefinition[] = [
  { value: "gpt", label: "GPT", visualPlatform: "openai" },
  { value: "claude", label: "Claude", visualPlatform: "anthropic" },
  { value: "gemini", label: "Gemini", visualPlatform: "gemini" },
  { value: "grok", label: "Grok", visualPlatform: "grok" },
  { value: "image_generation", label: "生成图片", visualPlatform: "image" },
  { value: "embedding", label: "Embedding", visualPlatform: "generic" },
  { value: "rerank", label: "Rerank", visualPlatform: "generic" },
  { value: "unknown", label: "未知", visualPlatform: "generic" },
];

const groupCategorySet = new Set(groupCategoryDefinitions.map((definition) => definition.value));

export function normalizeGroupCategory(value: unknown): StationGroupCategory | null {
  if (typeof value !== "string") {
    return null;
  }
  const normalized = value.trim().toLowerCase();
  return groupCategorySet.has(normalized as StationGroupCategory)
    ? (normalized as StationGroupCategory)
    : null;
}

export function effectiveGroupCategory(
  inferredGroupCategory: unknown,
  groupCategoryOverride: unknown,
): StationGroupCategory {
  return (
    normalizeGroupCategory(groupCategoryOverride) ??
    normalizeGroupCategory(inferredGroupCategory) ??
    "unknown"
  );
}

export function inferGroupCategoryFromEvidence(input: {
  groupName?: string | null;
  rawJsonRedacted?: Record<string, unknown> | null;
}): StationGroupCategory {
  const groupName = input.groupName ?? "";
  if (isImageGenerationGroupName(groupName)) {
    return "image_generation";
  }

  const platform = stringFieldFromRecord(input.rawJsonRedacted, [
    "platform",
    "provider",
    "model_provider",
    "modelProvider",
  ]);
  const platformCategory = groupCategoryFromPlatform(platform);
  if (platformCategory) {
    return platformCategory;
  }

  const text = [groupName, searchableJsonText(input.rawJsonRedacted)].join(" ");
  return groupCategoryFromText(text) ?? "unknown";
}

export function groupCategoryFromPlatform(value: string | null | undefined): StationGroupCategory | null {
  const normalized = normalizeText(value ?? "");
  if (["openai", "gpt"].includes(normalized)) {
    return "gpt";
  }
  if (["anthropic", "claude"].includes(normalized)) {
    return "claude";
  }
  if (["google", "gemini"].includes(normalized)) {
    return "gemini";
  }
  if (["grok", "xai", "x-ai"].includes(normalized)) {
    return "grok";
  }
  return null;
}

function groupCategoryFromText(value: string): StationGroupCategory | null {
  if (textMatchesAnyMatcher(value, ["claude", "anthropic", "sonnet", "opus", "haiku"])) {
    return "claude";
  }
  if (textMatchesAnyMatcher(value, ["gemini", "google"])) {
    return "gemini";
  }
  if (textMatchesAnyMatcher(value, ["grok", "xai", "x-ai"])) {
    return "grok";
  }
  if (textMatchesAnyMatcher(value, ["embedding", "embed", "向量"])) {
    return "embedding";
  }
  if (textMatchesAnyMatcher(value, ["rerank", "重排"])) {
    return "rerank";
  }
  if (textMatchesAnyMatcher(value, ["openai", "gpt", "codex"])) {
    return "gpt";
  }
  return null;
}

export function isImageGenerationGroupName(value: string) {
  return textMatchesAnyMatcher(value, [
    "图",
    "生图",
    "绘图",
    "image",
    "images",
    "picture",
    "pictures",
    "dall-e",
    "dalle",
    "midjourney",
  ]);
}

function textMatchesAnyMatcher(value: string, matchers: string[]) {
  const normalizedValue = normalizeText(value);
  return matchers.map(normalizeText).filter(Boolean).some((matcher) => normalizedValue.includes(matcher));
}

function normalizeText(value: string) {
  return value.trim().toLowerCase().replace(/[_\s]+/g, "-");
}

function searchableJsonText(value: Record<string, unknown> | null | undefined) {
  if (!value) {
    return "";
  }
  return collectJsonText(value).join(" ");
}

function collectJsonText(value: unknown): string[] {
  if (value === null || value === undefined) {
    return [];
  }
  if (typeof value === "string" || typeof value === "number" || typeof value === "boolean") {
    return [String(value)];
  }
  if (Array.isArray(value)) {
    return value.flatMap(collectJsonText);
  }
  if (typeof value === "object") {
    return Object.entries(value).flatMap(([key, nestedValue]) => [key, ...collectJsonText(nestedValue)]);
  }
  return [];
}

function stringFieldFromRecord(value: Record<string, unknown> | null | undefined, keys: string[]) {
  if (!value) {
    return null;
  }
  for (const key of keys) {
    const fieldValue = value[key];
    if (typeof fieldValue === "string" && fieldValue.trim()) {
      return fieldValue;
    }
  }
  return null;
}
