export type OfficialModelProvider = "openai" | "anthropic" | "google";

export type OfficialModelCatalogEntry = {
  provider: OfficialModelProvider;
  modelId: string;
  displayName: string;
  officialInputPrice: number;
  officialOutputPrice: number;
  currency: "USD";
  unit: "per_1m_tokens";
  aliases: string[];
  groupMatchers: string[];
  enabledByDefault: boolean;
  priceSourceUrl: string;
  priceSourceLabel: string;
};

const openAiPriceSource = "https://developers.openai.com/api/docs/pricing";
const anthropicPriceSource = "https://platform.claude.com/docs/en/about-claude/pricing";
const geminiPriceSource = "https://ai.google.dev/gemini-api/docs/pricing";

export const officialModelCatalog: OfficialModelCatalogEntry[] = [
  {
    provider: "openai",
    modelId: "gpt-5.5",
    displayName: "GPT-5.5",
    officialInputPrice: 2.5,
    officialOutputPrice: 15,
    currency: "USD",
    unit: "per_1m_tokens",
    aliases: ["gpt-5.5"],
    groupMatchers: ["openai", "gpt", "default", "green"],
    enabledByDefault: true,
    priceSourceUrl: openAiPriceSource,
    priceSourceLabel: "OpenAI API pricing, short-context standard rate",
  },
  {
    provider: "openai",
    modelId: "gpt-5.3-codex",
    displayName: "GPT-5.3 Codex",
    officialInputPrice: 1.75,
    officialOutputPrice: 14,
    currency: "USD",
    unit: "per_1m_tokens",
    aliases: ["gpt-5.3-codex", "codex"],
    groupMatchers: ["openai", "gpt", "codex", "default", "green"],
    enabledByDefault: true,
    priceSourceUrl: openAiPriceSource,
    priceSourceLabel: "OpenAI API pricing, standard Codex rate",
  },
  {
    provider: "openai",
    modelId: "gpt-5-mini",
    displayName: "GPT-5 mini",
    officialInputPrice: 0.25,
    officialOutputPrice: 2,
    currency: "USD",
    unit: "per_1m_tokens",
    aliases: ["gpt-5-mini"],
    groupMatchers: ["openai", "gpt", "default", "green"],
    enabledByDefault: true,
    priceSourceUrl: "https://developers.openai.com/api/docs/models/gpt-5-mini",
    priceSourceLabel: "OpenAI GPT-5 mini model pricing",
  },
  {
    provider: "anthropic",
    modelId: "claude-sonnet-5",
    displayName: "Claude Sonnet 5",
    officialInputPrice: 1,
    officialOutputPrice: 5,
    currency: "USD",
    unit: "per_1m_tokens",
    aliases: ["claude-sonnet-5", "sonnet-5"],
    groupMatchers: ["anthropic", "claude", "sonnet", "yellow", "amber"],
    enabledByDefault: true,
    priceSourceUrl: anthropicPriceSource,
    priceSourceLabel: "Anthropic Sonnet 5 pricing through 2026-08-31",
  },
  {
    provider: "google",
    modelId: "gemini-3.5-flash",
    displayName: "Gemini 3.5 Flash",
    officialInputPrice: 1.5,
    officialOutputPrice: 9,
    currency: "USD",
    unit: "per_1m_tokens",
    aliases: ["gemini-3.5-flash"],
    groupMatchers: ["google", "gemini", "flash"],
    enabledByDefault: true,
    priceSourceUrl: geminiPriceSource,
    priceSourceLabel: "Gemini Developer API pricing, paid tier standard rate",
  },
  {
    provider: "google",
    modelId: "gemini-3.1-flash-lite",
    displayName: "Gemini 3.1 Flash-Lite",
    officialInputPrice: 0.25,
    officialOutputPrice: 1.5,
    currency: "USD",
    unit: "per_1m_tokens",
    aliases: ["gemini-3.1-flash-lite", "gemini-flash-lite"],
    groupMatchers: ["google", "gemini", "flash-lite", "flash_lite"],
    enabledByDefault: true,
    priceSourceUrl: geminiPriceSource,
    priceSourceLabel: "Gemini Developer API pricing, paid tier standard rate",
  },
];

export function enabledOfficialModelCatalog() {
  return officialModelCatalog.filter((model) => model.enabledByDefault);
}

export function normalizeCatalogText(value: string) {
  return value.trim().toLowerCase().replace(/[_\s]+/g, "-");
}
