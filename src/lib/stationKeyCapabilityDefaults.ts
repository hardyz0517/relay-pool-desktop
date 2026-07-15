import type { StationKeyCapabilities } from "@/lib/types/stationKeys";

export const OPENAI_COMPATIBLE_CAPABILITY_DEFAULTS = {
  supportsChatCompletions: true,
  supportsResponses: true,
  supportsEmbeddings: false,
  supportsStream: true,
  supportsTools: false,
  supportsVision: false,
  supportsReasoning: false,
  modelAllowlist: [],
  modelBlocklist: [],
  preferredModels: [],
  onlyUseAsBackup: false,
  routingTags: [],
} satisfies Omit<StationKeyCapabilities, "stationKeyId" | "updatedAt">;
