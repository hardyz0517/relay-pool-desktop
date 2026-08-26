export type ModelBasePrice = {
  id: string;
  provider: string;
  model: string;
  inputPrice: number | null;
  outputPrice: number | null;
  inputPricePriority: number | null;
  outputPricePriority: number | null;
  cacheCreationPrice: number | null;
  cacheCreationPricePriority: number | null;
  cacheCreationPriceAbove1Hr: number | null;
  cacheReadPrice: number | null;
  cacheReadPricePriority: number | null;
  longContextInputTokenThreshold: number | null;
  longContextInputCostMultiplier: number | null;
  longContextOutputCostMultiplier: number | null;
  supportsServiceTier: boolean;
  supportsPromptCaching: boolean;
  currency: string;
  unit: string;
  sourceUrl: string;
  sourceLabel: string;
  sourceCheckedAt: string | null;
  enabled: boolean;
  builtIn: boolean;
  note: string | null;
  createdAt: string;
  updatedAt: string;
};

export type ModelPriceSyncState = {
  sourceUrl: string;
  autoSyncEnabled: boolean;
  includeCommonModels: boolean;
  selectedModelKeys: string[];
  excludedCommonModelKeys: string[];
  lastSyncAt: string | null;
  lastSyncError: string | null;
  modelCount: number;
  commonModelCount: number;
  autoSyncModelCount: number;
  filePath: string;
};

export type ModelPriceCatalogEntry = {
  key: string;
  provider: string;
  model: string;
  name: string;
  common: boolean;
  releaseDate: string | null;
  inputPrice: number | null;
  outputPrice: number | null;
  cacheCreationPrice: number | null;
  cacheReadPrice: number | null;
};

export type ModelPriceSyncResult = {
  state: ModelPriceSyncState;
  importedCount: number;
  skippedCount: number;
};

export type ModelPriceSyncConfig = {
  autoSyncEnabled: boolean;
  includeCommonModels?: boolean;
  selectedModelKeys?: string[];
  excludedCommonModelKeys?: string[];
};

export type BalanceSnapshot = {
  id: string;
  stationId: string;
  stationKeyId: string | null;
  scope: "station" | "station_key" | string;
  value: number | null;
  currency: string;
  creditUnit: string | null;
  usedValue: number | null;
  totalValue: number | null;
  todayRequestCount: number | null;
  totalRequestCount: number | null;
  todayConsumption: number | null;
  totalConsumption: number | null;
  todayBaseConsumption: number | null;
  totalBaseConsumption: number | null;
  todayTokenCount: number | null;
  totalTokenCount: number | null;
  todayInputTokenCount: number | null;
  todayOutputTokenCount: number | null;
  totalInputTokenCount: number | null;
  totalOutputTokenCount: number | null;
  accountConcurrencyLimit: number | null;
  lowBalanceThreshold: number | null;
  status: "unknown" | "normal" | "low" | "depleted" | string;
  source: string;
  confidence: number;
  collectedAt: string | null;
  createdAt: string;
  updatedAt: string;
};

export type PricingStatus =
  | "priced"
  | "base_price_only"
  | "missing_rate"
  | "missing_model_price"
  | "unpriced"
  | "unsupported_billing_mode"
  | "legacy_estimate";

export type RequestKind = "text" | "image" | "video" | "any";

export type UpsertBalanceSnapshotInput = {
  id: string | null;
  stationId: string;
  stationKeyId: string | null;
  scope: "station" | "station_key";
  value: number | null;
  currency: string;
  creditUnit: string | null;
  usedValue: number | null;
  totalValue: number | null;
  todayRequestCount: number | null;
  totalRequestCount: number | null;
  todayConsumption: number | null;
  totalConsumption: number | null;
  todayBaseConsumption: number | null;
  totalBaseConsumption: number | null;
  todayTokenCount: number | null;
  totalTokenCount: number | null;
  todayInputTokenCount: number | null;
  todayOutputTokenCount: number | null;
  totalInputTokenCount: number | null;
  totalOutputTokenCount: number | null;
  accountConcurrencyLimit: number | null;
  lowBalanceThreshold: number | null;
  status: "unknown" | "normal" | "low" | "depleted";
  source: string;
  confidence: number;
  collectedAt: string | null;
};

export type UpsertModelBasePriceInput = {
  id?: string | null;
  provider: string;
  model: string;
  inputPrice: number | null;
  outputPrice: number | null;
  inputPricePriority: number | null;
  outputPricePriority: number | null;
  cacheCreationPrice: number | null;
  cacheCreationPricePriority: number | null;
  cacheCreationPriceAbove1Hr: number | null;
  cacheReadPrice: number | null;
  cacheReadPricePriority: number | null;
  longContextInputTokenThreshold: number | null;
  longContextInputCostMultiplier: number | null;
  longContextOutputCostMultiplier: number | null;
  supportsServiceTier: boolean;
  supportsPromptCaching: boolean;
  currency: string;
  unit: string;
  sourceUrl: string;
  sourceLabel: string;
  sourceCheckedAt: string | null;
  enabled: boolean;
  builtIn: boolean;
  note: string | null;
};

export type ResolvedPricingContext = {
  stationKeyId: string;
  stationId: string;
  requestedModel: string;
  resolvedModel: string;
  requestKind: RequestKind;
  groupBindingId: string | null;
  baseInputPrice: number | null;
  baseOutputPrice: number | null;
  baseCacheCreationPrice: number | null;
  baseCacheReadPrice: number | null;
  currency: string;
  unit: string;
  basePriceSource: string | null;
  effectiveRateMultiplier: number | null;
  rateSource: string | null;
  rateCollectedAt: string | null;
  estimatedInputPrice: number | null;
  estimatedOutputPrice: number | null;
  estimatedCacheCreationPrice: number | null;
  estimatedCacheReadPrice: number | null;
  pricingStatus: PricingStatus;
  confidence: number;
  sourceChain: string[];
  reason: string | null;
  resolvedAt: string;
};

export type RequestCost = {
  promptTokens: number | null;
  completionTokens: number | null;
  totalTokens: number | null;
  estimatedInputCost: number | null;
  estimatedOutputCost: number | null;
  estimatedTotalCost: number | null;
  costCurrency: string | null;
  pricingSource: string | null;
  costStatus: string | null;
};
