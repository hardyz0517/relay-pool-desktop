import type { StationKeyCapabilities } from "@/lib/types/stationKeys";

export const OPENAI_COMPATIBLE_CAPABILITY_DEFAULTS = {
  supportsChatCompletions: true,
  supportsResponses: true,
  supportsEmbeddings: false,
  supportsStream: true,
  supportsTools: true,
  supportsVision: false,
  supportsReasoning: true,
  modelAllowlist: [],
  modelBlocklist: [],
  preferredModels: [],
  onlyUseAsBackup: false,
  routingTags: [],
} satisfies Omit<StationKeyCapabilities, "stationKeyId" | "updatedAt">;
