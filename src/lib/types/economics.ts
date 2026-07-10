export type PricingRule = {
  id: string;
  stationId: string;
  stationKeyId: string | null;
  groupBindingId: string | null;
  groupName: string | null;
  tierLabel: string | null;
  model: string;
  inputPrice: number | null;
  outputPrice: number | null;
  fixedPrice: number | null;
  rateMultiplier: number | null;
  currency: string;
  unit: string;
  priceType: string;
  basePriceSource: string | null;
  normalizationStatus: string;
  source: string;
  confidence: number;
  enabled: boolean;
  note: string | null;
  collectedAt: string | null;
  validFrom: string | null;
  validUntil: string | null;
  createdAt: string;
  updatedAt: string;
};

export type ModelBasePrice = {
  id: string;
  provider: string;
  model: string;
  inputPrice: number | null;
  outputPrice: number | null;
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

export type ResolvedPricingContext = {
  stationKeyId: string;
  stationId: string;
  requestedModel: string;
  resolvedModel: string;
  requestKind: RequestKind;
  groupBindingId: string | null;
  baseInputPrice: number | null;
  baseOutputPrice: number | null;
  baseFixedPrice: number | null;
  currency: string;
  unit: string;
  basePriceSource: string | null;
  effectiveRateMultiplier: number | null;
  rateSource: string | null;
  rateCollectedAt: string | null;
  estimatedInputPrice: number | null;
  estimatedOutputPrice: number | null;
  estimatedFixedPrice: number | null;
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
  pricingRuleId: string | null;
  pricingSource: string | null;
  costStatus: string | null;
};
