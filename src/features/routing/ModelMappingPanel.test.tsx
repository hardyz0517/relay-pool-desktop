// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ToastProvider } from "@/components/ui";
import type { ModelMappingWorkspaceDto } from "@/lib/types/modelMapping";
import { ModelMappingPanel } from "./ModelMappingPanel";

const mocks = vi.hoisted(() => ({
  apply: vi.fn(),
  simulate: vi.fn(),
  workspace: null as ModelMappingWorkspaceDto | null,
  keyPool: [] as Array<{ id: string }>,
  capabilities: [] as Array<{
    modelAllowlist: string[];
    modelBlocklist: string[];
    preferredModels: string[];
  }>,
}));

vi.mock("@/lib/api/modelMapping", () => ({
  applyModelMappingDocument: mocks.apply,
  getModelMappingWorkspace: vi.fn(async () => mocks.workspace),
  getModelMappingDocument: vi.fn(),
  validateModelMappingDocument: vi.fn(),
  restoreModelMappingRevision: vi.fn(),
  simulateModelMapping: mocks.simulate,
  resolveRequestMappingTrace: vi.fn(),
}));

vi.mock("@/lib/query/useActivityQuery", () => ({
  useActivityQuery: (options: { queryKey?: readonly unknown[] }) => ({
    isPending: false,
    error: null,
    data: options.queryKey?.includes("keyPool")
      ? mocks.keyPool
      : options.queryKey?.includes("keyCapabilities")
        ? mocks.keyPool.map((item, index) => ({ stationKeyId: item.id, ...(mocks.capabilities[index] ?? { modelAllowlist: [], modelBlocklist: [], preferredModels: [] }) }))
        : options.queryKey?.includes("modelMapping") ? mocks.workspace : [],
  }),
}));

vi.mock("@/lib/api/routing", () => ({
  getStationKeyCapabilities: vi.fn(async (stationKeyId: string) => {
    const index = mocks.keyPool.findIndex((item) => item.id === stationKeyId);
    return mocks.capabilities[index] ?? { modelAllowlist: [], modelBlocklist: [], preferredModels: [] };
  }),
}));

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

afterEach(() => {
  document.body.innerHTML = "";
  mocks.apply.mockReset();
  mocks.simulate.mockReset();
  mocks.workspace = null;
  mocks.keyPool = [];
  mocks.capabilities = [];
  vi.restoreAllMocks();
});

describe("ModelMappingPanel", () => {
  it("separates the default mapping from an empty rule state", async () => {
    const workspace = emptyWorkspace();
    mocks.workspace = workspace;
    const { host, root, queryClient } = renderPanel();

    expect(host.textContent).toContain("默认映射");
    expect(host.textContent).toContain("默认目标模型");
    expect(host.textContent).toContain("保存默认值");
    expect(host.textContent).toContain("映射规则");
    expect(host.textContent).toContain("还没有映射规则");
    expect(host.textContent).not.toContain("配置未保存");
    expect(findButton(host, "预览")).toBeUndefined();
    expect(findButton(host, "保存规则")).toBeUndefined();

    await act(async () => findButton(host, "新增第一条规则")?.click());
    expect(host.querySelector('[aria-label="规则编辑器"]')).toBeTruthy();
    expect(findButton(host, "取消")).toBeTruthy();
    expect(findButton(host, "预览")).toBeTruthy();
    expect(findButton(host, "保存规则")).toBeTruthy();

    await act(async () => root.unmount());
    queryClient.clear();
  });

  it("saves the default mapping independently of the rule list", async () => {
    const workspace = emptyWorkspace();
    mocks.workspace = workspace;
    mocks.apply.mockImplementation(async (input: { document: ModelMappingWorkspaceDto["document"] }) => ({ ...workspace, document: input.document }));
    const { host, root, queryClient } = renderPanel();
    setInput(host, '[aria-label="默认目标模型"]', "deepseek-v4-flash");

    await act(async () => findButton(host, "保存默认值")?.click());

    expect(mocks.apply).toHaveBeenCalledTimes(1);
    const savedDocument = mocks.apply.mock.calls[0][0].document;
    expect(savedDocument.rules).toHaveLength(1);
    expect(savedDocument.rules[0].matcher).toEqual({ kind: "default" });
    expect(savedDocument.rules[0].action).toEqual({ kind: "map_fixed", target: { kind: "literal", upstreamModel: "deepseek-v4-flash" } });
    expect(host.textContent).toContain("还没有映射规则");
    expect(host.textContent).toContain("默认值已保存");

    await act(async () => root.unmount());
    queryClient.clear();
  });

  it("opens one editor for a new rule and returns to the list after saving", async () => {
    const workspace = emptyWorkspace();
    mocks.workspace = workspace;
    mocks.apply.mockImplementation(async (input: { document: ModelMappingWorkspaceDto["document"] }) => ({ ...workspace, document: input.document }));
    const { host, root, queryClient } = renderPanel();
    await act(async () => findButton(host, "新增第一条规则")?.click());
    setInput(host, '[aria-label="编辑请求模型"]', "gpt-4o-mini");
    setInput(host, '[aria-label$="目标模型"]', "deepseek-v4-flash");

    await act(async () => findButton(host, "保存规则")?.click());

    expect(mocks.apply).toHaveBeenCalledTimes(1);
    expect(host.querySelector('[aria-label="规则编辑器"]')).toBeNull();
    expect(host.textContent).toContain("gpt-4o-mini");
    expect(host.textContent).toContain("deepseek-v4-flash");
    expect(host.textContent).toContain("规则已保存");

    await act(async () => root.unmount());
    queryClient.clear();
  });

  it("edits an existing rule and supports cancel without a second editor", async () => {
    const workspace = phase2Workspace();
    mocks.workspace = workspace;
    const { host, root, queryClient } = renderPanel();
    await act(async () => findButton(host, "编辑")?.click());
    expect(host.querySelectorAll('[aria-label="规则编辑器"]').length).toBe(1);
    expect(host.textContent).not.toContain("还没有映射规则");

    await act(async () => findButton(host, "取消")?.click());
    expect(host.querySelector('[aria-label="规则编辑器"]')).toBeNull();
    expect(host.textContent).toContain("fallback-a");
    expect(host.textContent).toContain("fallback-b");

    await act(async () => root.unmount());
    queryClient.clear();
  });

  it("keeps preview errors local to the rule editor", async () => {
    const workspace = phase2Workspace();
    mocks.workspace = workspace;
    mocks.simulate.mockRejectedValue(new Error("The requested model is invalid."));
    const { host, root, queryClient } = renderPanel();
    await act(async () => findButton(host, "编辑")?.click());
    await act(async () => findButton(host, "预览")?.click());

    expect(host.querySelector('[role="alert"]')?.textContent).toContain("The requested model is invalid.");
    expect(host.textContent).not.toContain("模型映射预览不可用");

    await act(async () => root.unmount());
    queryClient.clear();
  });

  it("offers the union of models from every key in both pickers", async () => {
    const workspace = emptyWorkspace();
    mocks.workspace = workspace;
    mocks.keyPool = [{ id: "key-a" }, { id: "key-b" }];
    mocks.capabilities = [
      { modelAllowlist: ["gpt-4o-mini"], modelBlocklist: [], preferredModels: ["shared-model"] },
      { modelAllowlist: [], modelBlocklist: ["claude-sonnet"], preferredModels: ["shared-model"] },
    ];
    const { host, root, queryClient } = renderPanel();

    const defaultPicker = host.querySelector('[aria-label="默认目标模型 候选模型"]') as HTMLSelectElement;
    expect(defaultPicker).toBeTruthy();
    expect(Array.from(defaultPicker.options).map((option) => option.value)).toEqual(["", "claude-sonnet", "gpt-4o-mini", "shared-model"]);

    await act(async () => findButton(host, "新增第一条规则")?.click());
    const requestPicker = host.querySelector('[aria-label="编辑请求模型 候选模型"]') as HTMLSelectElement;
    const targetPicker = host.querySelector('[aria-label$="目标模型 候选模型"]') as HTMLSelectElement;
    expect(requestPicker).toBeTruthy();
    expect(targetPicker).toBeTruthy();

    await act(async () => {
      requestPicker.value = "gpt-4o-mini";
      requestPicker.dispatchEvent(new Event("change", { bubbles: true }));
    });
    expect((host.querySelector('[aria-label="编辑请求模型"]') as HTMLInputElement).value).toBe("gpt-4o-mini");

    await act(async () => {
      targetPicker.value = "shared-model";
      targetPicker.dispatchEvent(new Event("change", { bubbles: true }));
    });
    expect((host.querySelector('[aria-label$="目标模型"]') as HTMLInputElement).value).toBe("shared-model");

    await act(async () => root.unmount());
    queryClient.clear();
  });
});

function renderPanel() {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const host = document.createElement("div");
  document.body.append(host);
  const root = createRoot(host);
  act(() => {
    root.render(
      <QueryClientProvider client={queryClient}>
        <ToastProvider>
          <ModelMappingPanel />
        </ToastProvider>
      </QueryClientProvider>,
    );
  });
  return { host, root, queryClient };
}

function findButton(host: HTMLElement, text: string): HTMLButtonElement | undefined {
  return Array.from(host.querySelectorAll("button")).find((button) => button.textContent?.includes(text)) as HTMLButtonElement | undefined;
}

function setInput(host: HTMLElement, selector: string, value: string) {
  const input = host.querySelector(selector) as HTMLInputElement;
  expect(input).toBeTruthy();
  act(() => {
    const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")?.set;
    setter?.call(input, value);
    input.dispatchEvent(new Event("input", { bubbles: true }));
    input.dispatchEvent(new Event("change", { bubbles: true }));
  });
}

function emptyWorkspace(): ModelMappingWorkspaceDto {
  return {
    document: {
      formatVersion: 1,
      baseRevision: 1,
      policy: { unmatchedModelBehavior: "preserve" },
      rules: [],
      profiles: [],
      bindings: [],
    },
    status: {
      activeRevision: 1,
      syncState: "synchronized",
      source: "ui",
      filePresent: false,
      lastErrorCode: null,
    },
    knownModelOptions: [],
    legacyReviews: [],
    diagnostics: [],
    candidateCount: 0,
  };
}

function phase2Workspace(): ModelMappingWorkspaceDto {
  const workspace = emptyWorkspace();
  workspace.document.profiles = [{
    id: "profile-a",
    canonicalModel: "codex-5.4",
    displayName: "Codex",
    defaultUpstreamModel: "native-default",
    status: "active",
    note: null,
    revision: 1,
    createdAtMs: 1,
    updatedAtMs: 1,
  }];
  workspace.document.rules = [{
    id: "rule-fallback",
    priority: 10,
    enabled: true,
    matcher: { kind: "exact", model: "codex-5.4" },
    conditions: { endpointKinds: [], stream: "any", tools: "any", vision: "any", reasoning: "any" },
    action: {
      kind: "map_fallback_chain",
      targets: [
        { kind: "literal", upstreamModel: "fallback-a" },
        { kind: "literal", upstreamModel: "fallback-b" },
      ],
      fallbackTrigger: "no_eligible_target",
    },
    note: null,
    revision: 1,
    createdAtMs: 1,
    updatedAtMs: 1,
  }];
  return workspace;
}
