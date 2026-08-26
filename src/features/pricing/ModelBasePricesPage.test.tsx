// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ToastProvider } from "@/components/ui";
import type { ModelBasePrice, ModelPriceCatalogEntry, ModelPriceSyncState } from "@/lib/types/economics";
import { ModelBasePricesPage } from "./ModelBasePricesPage";

const mocks = vi.hoisted(() => ({
  deleteModelBasePrice: vi.fn(),
  getModelPriceSyncState: vi.fn(),
  invalidateQueries: vi.fn(async () => undefined),
  listModelPriceSyncCatalog: vi.fn(),
  openModelPriceCatalogDirectory: vi.fn(async () => undefined),
  reloadModelPriceCatalog: vi.fn(),
  resetModelBasePricesToBuiltins: vi.fn(),
  saveModelPriceSyncConfig: vi.fn(),
  setQueryData: vi.fn(),
  syncModelPrices: vi.fn(),
  upsertModelBasePrice: vi.fn(),
}));

const fixtures = vi.hoisted(() => ({
  catalog: [] as unknown[],
  rows: [] as unknown[],
  syncState: null as unknown,
}));

vi.mock("@tanstack/react-query", () => ({
  useQueryClient: () => ({
    invalidateQueries: mocks.invalidateQueries,
    setQueryData: mocks.setQueryData,
  }),
}));

vi.mock("@/lib/api/economics", () => ({
  deleteModelBasePrice: mocks.deleteModelBasePrice,
  getModelPriceSyncState: mocks.getModelPriceSyncState,
  listModelPriceSyncCatalog: mocks.listModelPriceSyncCatalog,
  openModelPriceCatalogDirectory: mocks.openModelPriceCatalogDirectory,
  reloadModelPriceCatalog: mocks.reloadModelPriceCatalog,
  resetModelBasePricesToBuiltins: mocks.resetModelBasePricesToBuiltins,
  saveModelPriceSyncConfig: mocks.saveModelPriceSyncConfig,
  syncModelPrices: mocks.syncModelPrices,
  upsertModelBasePrice: mocks.upsertModelBasePrice,
}));

vi.mock("@/lib/query/resourceQueries", () => ({
  modelBasePricesQueryOptions: () => ({ fixture: "prices" }),
  modelPriceSyncStateQueryOptions: () => ({ fixture: "sync" }),
}));

vi.mock("@/lib/query/useActivityQuery", () => ({
  useActivityQuery: (options: { fixture: "prices" | "sync" }) =>
    options.fixture === "prices"
      ? { data: fixtures.rows, error: null, isPending: false }
      : { data: fixtures.syncState, error: null, isPending: false },
}));

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function model(provider: string, id: string, prices: Partial<ModelBasePrice> = {}): ModelBasePrice {
  return {
    id: `models-dev-${provider}-${id}`,
    provider,
    model: id,
    inputPrice: 1,
    outputPrice: 2,
    inputPricePriority: null,
    outputPricePriority: null,
    cacheCreationPrice: 0.5,
    cacheCreationPricePriority: null,
    cacheCreationPriceAbove1Hr: null,
    cacheReadPrice: 0.1,
    cacheReadPricePriority: null,
    longContextInputTokenThreshold: null,
    longContextInputCostMultiplier: null,
    longContextOutputCostMultiplier: null,
    supportsServiceTier: false,
    supportsPromptCaching: true,
    currency: "USD",
    unit: "M",
    sourceUrl: "https://models.dev/api.json",
    sourceLabel: "models.dev",
    sourceCheckedAt: "2026-08-24T00:00:00Z",
    enabled: true,
    builtIn: false,
    note: null,
    createdAt: "2026-08-24T00:00:00Z",
    updatedAt: "2026-08-24T00:00:00Z",
    ...prices,
  };
}

function catalogEntry(provider: string, id: string): ModelPriceCatalogEntry {
  return {
    key: `${provider}/${id}`,
    provider,
    model: id,
    name: id,
    common: false,
    releaseDate: "2026-08-24",
    inputPrice: 1,
    outputPrice: 2,
    cacheCreationPrice: 0.5,
    cacheReadPrice: 0.1,
  };
}

const syncState: ModelPriceSyncState = {
  sourceUrl: "https://models.dev/api.json",
  autoSyncEnabled: false,
  includeCommonModels: true,
  selectedModelKeys: [],
  excludedCommonModelKeys: [],
  lastSyncAt: null,
  lastSyncError: null,
  modelCount: 2,
  commonModelCount: 1,
  autoSyncModelCount: 1,
  filePath: "C:\\Users\\test\\model-pricing.json",
};

let host: HTMLDivElement;
let root: Root;

beforeEach(async () => {
  vi.clearAllMocks();
  fixtures.rows = [
    model("openai", "gpt-test", { note: "GPT Test; USD per M tokens" }),
    model("relay", "custom-model", { note: "Custom Model; USD per M tokens" }),
  ];
  fixtures.catalog = [catalogEntry("openai", "gpt-test"), catalogEntry("relay", "custom-model")];
  fixtures.syncState = syncState;
  mocks.getModelPriceSyncState.mockResolvedValue(syncState);
  mocks.listModelPriceSyncCatalog.mockImplementation(async () => fixtures.catalog);
  mocks.saveModelPriceSyncConfig.mockImplementation(async (input) => ({ ...syncState, ...input }));
  mocks.syncModelPrices.mockResolvedValue({
    state: syncState,
    importedCount: 2,
    skippedCount: 0,
  });
  host = document.createElement("div");
  document.body.appendChild(host);
  root = createRoot(host);
  await act(async () => {
    root.render(
      <ToastProvider>
        <ModelBasePricesPage backLabel="返回" onBack={vi.fn()} />
      </ToastProvider>,
    );
  });
});

afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
  document.body.replaceChildren();
});

function buttonWithText(text: string) {
  const button = [...document.body.querySelectorAll<HTMLButtonElement>("button")]
    .find((candidate) => candidate.textContent?.includes(text));
  if (!button) {
    throw new Error(`Button not found: ${text}`);
  }
  return button;
}

async function click(element: Element) {
  await act(async () => {
    element.dispatchEvent(new MouseEvent("click", { bubbles: true }));
  });
}

function inputWithLabel(label: string) {
  const input = document.body.querySelector<HTMLInputElement>(`input[aria-label="${label}"]`);
  if (!input) {
    throw new Error(`Input not found: ${label}`);
  }
  return input;
}

async function changeInput(input: HTMLInputElement, value: string) {
  await act(async () => {
    const valueSetter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")?.set;
    valueSetter?.call(input, value);
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

describe("ModelBasePricesPage models.dev synchronization", () => {
  it("renders one CCS-style effective-price table without provider grouping", () => {
    const headers = [...document.body.querySelectorAll("table thead th")]
      .map((header) => header.textContent?.trim());
    const priceRows = document.body.querySelectorAll("table tbody tr");

    expect(headers).toEqual(["模型", "显示名称", "输入成本", "输出成本", "缓存命中", "缓存创建", "操作"]);
    expect(priceRows).toHaveLength(2);
    expect(document.body.textContent).toContain("GPT Test");
    expect(document.body.textContent).toContain("Custom Model");
    expect(document.body.querySelector('[aria-label="按厂商筛选模型基准价格"]')).toBeNull();
    expect(document.body.querySelector('[aria-label="编辑 gpt-test"]')).not.toBeNull();
  });

  it("offers CCS-style filtering and keeps common models as an implicit automatic range", async () => {
    await click(buttonWithText("选择自动同步模型"));

    expect(document.body.textContent).toContain("选择自动同步定价的模型");
    expect(document.body.textContent).toContain("全选筛选结果（2）");
    expect(document.body.textContent).toContain("输入成本");
    expect(document.body.textContent).toContain("缓存创建");

    const commonSwitch = document.body.querySelector('[aria-label="自动包含常用模型"]');
    expect(commonSwitch).not.toBeNull();
    await click(buttonWithText("保存选择"));

    expect(mocks.saveModelPriceSyncConfig).toHaveBeenCalledWith({
      autoSyncEnabled: false,
      includeCommonModels: true,
      selectedModelKeys: [],
      excludedCommonModelKeys: [],
    });
  });

  it("always sends a forced full-sync request from the outer immediate-sync button", async () => {
    await click(buttonWithText("立即同步"));

    expect(mocks.syncModelPrices).toHaveBeenCalledWith(true);
  });

  it("shows the persisted backend reason when synchronization exhausts its retries", async () => {
    const failedState = {
      ...syncState,
      lastSyncError: "连接 models.dev 超时，请检查系统代理或网络设置",
    };
    mocks.syncModelPrices.mockRejectedValue(new Error("The external provider is unavailable."));
    mocks.getModelPriceSyncState.mockResolvedValue(failedState);

    await click(buttonWithText("立即同步"));

    expect(mocks.getModelPriceSyncState).toHaveBeenCalledTimes(1);
    expect(document.body.textContent).toContain(failedState.lastSyncError);
    expect(document.body.textContent).not.toContain("The external provider is unavailable.");
  });

  it("uses the complete local catalog instead of the 200-row price-table query", async () => {
    fixtures.catalog = Array.from({ length: 480 }, (_, index) =>
      catalogEntry(`official-provider-${index % 20}`, `model-${index}`),
    );

    await click(buttonWithText("选择自动同步模型"));

    expect(mocks.listModelPriceSyncCatalog).toHaveBeenCalledTimes(1);
    expect(document.body.textContent).toContain("全选筛选结果（480）");
    expect(document.body.textContent).toContain("共 480 条");
  });

  it("uses a compact add page and imports a models.dev quote into its six visible fields", async () => {
    await click(buttonWithText("新增"));

    expect(document.body.textContent).toContain("新增定价");
    expect(inputWithLabel("模型 ID").value).toBe("");
    expect(inputWithLabel("显示名称").value).toBe("");
    expect(inputWithLabel("输入成本（每百万 tokens, USD）").value).toBe("0");
    expect(inputWithLabel("输出成本（每百万 tokens, USD）").value).toBe("0");
    expect(inputWithLabel("缓存读取成本（每百万 tokens, USD）").value).toBe("0");
    expect(inputWithLabel("缓存写入成本（每百万 tokens, USD）").value).toBe("0");
    expect(document.body.textContent).not.toContain("优先级输入价");
    expect(document.body.textContent).not.toContain("来源 URL");

    await click(buttonWithText("从 models.dev 导入"));
    expect(mocks.listModelPriceSyncCatalog).toHaveBeenCalledTimes(1);
    expect(document.body.textContent).toContain("从 models.dev 导入定价");

    await click(buttonWithText("gpt-test"));

    expect(inputWithLabel("模型 ID").value).toBe("gpt-test");
    expect(inputWithLabel("显示名称").value).toBe("gpt-test");
    expect(inputWithLabel("输入成本（每百万 tokens, USD）").value).toBe("1");
    expect(inputWithLabel("输出成本（每百万 tokens, USD）").value).toBe("2");
    expect(inputWithLabel("缓存读取成本（每百万 tokens, USD）").value).toBe("0.1");
    expect(inputWithLabel("缓存写入成本（每百万 tokens, USD）").value).toBe("0.5");
  });

  it("uses the same full-page editor and field labels for existing prices", async () => {
    const editButton = document.body.querySelector('[aria-label="编辑 gpt-test"]');
    expect(editButton).not.toBeNull();
    await click(editButton!);

    expect(document.body.textContent).toContain("编辑定价");
    expect(document.body.textContent).toContain("可从 models.dev 重新选择模型并覆盖当前定价");
    expect(document.body.querySelector('[role="dialog"]')).toBeNull();
    expect(inputWithLabel("模型 ID").value).toBe("gpt-test");
    expect(inputWithLabel("显示名称").value).toBe("GPT Test");
    expect(inputWithLabel("输入成本（每百万 tokens, USD）").value).toBe("1");
    expect(inputWithLabel("输出成本（每百万 tokens, USD）").value).toBe("2");
    expect(inputWithLabel("缓存读取成本（每百万 tokens, USD）").value).toBe("0.1");
    expect(inputWithLabel("缓存写入成本（每百万 tokens, USD）").value).toBe("0.5");
    expect(buttonWithText("从 models.dev 导入").disabled).toBe(false);
    expect(buttonWithText("保存").disabled).toBe(false);
  });

  it("saves hidden pricing metadata with safe defaults from the compact add page", async () => {
    mocks.upsertModelBasePrice.mockResolvedValue(model("custom", "my-model", {
      id: "builtin-my-model",
      inputPrice: 0,
      outputPrice: 0,
      cacheReadPrice: 0,
      cacheCreationPrice: 0,
      note: "My Model",
    }));

    await click(buttonWithText("新增"));
    await changeInput(inputWithLabel("模型 ID"), "my-model");
    await changeInput(inputWithLabel("显示名称"), "My Model");
    await click(buttonWithText("添加"));

    expect(mocks.upsertModelBasePrice).toHaveBeenCalledWith(expect.objectContaining({
      id: "builtin-my-model",
      provider: "custom",
      model: "my-model",
      inputPrice: 0,
      outputPrice: 0,
      cacheReadPrice: 0,
      cacheCreationPrice: 0,
      currency: "USD",
      unit: "M",
      sourceLabel: "Manual",
      enabled: true,
      builtIn: false,
      note: "My Model",
    }));
  });
});
