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
  simulateModelMapping: vi.fn(),
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
  mocks.workspace = null;
  mocks.keyPool = [];
  mocks.capabilities = [];
  vi.restoreAllMocks();
});

describe("ModelMappingPanel", () => {
  it("shows an empty inline mapping state", async () => {
    const workspace = emptyWorkspace();
    mocks.workspace = workspace;
    const { host, root, queryClient } = renderPanel();

    expect(host.textContent).not.toContain("实际请求模型");
    expect(host.textContent).not.toContain("上游目标模型");
    expect(host.textContent).not.toContain("为每个模型填写实际请求模型和上游目标模型，填写完成并离开后自动用于路由。");
    expect(host.textContent).not.toContain("菜单显示名");
    expect(host.textContent).toContain("还没有模型映射");
    expect(Array.from(host.querySelectorAll("button")).filter((button) => button.textContent?.includes("添加模型"))).toHaveLength(1);
    expect(host.textContent).not.toContain("配置未保存");
    expect(host.querySelector('[aria-label^="保存模型映射"]')).toBeNull();

    await act(async () => findButton(host, "添加模型")?.click());
    expect(host.querySelector('[aria-label^="模型映射行"]')).toBeTruthy();
    expect(host.querySelector('[aria-label^="保存模型映射"]')).toBeNull();
    expect(host.querySelector('[aria-label^="启用模型映射"]')).toBeNull();

    await act(async () => root.unmount());
    queryClient.clear();
  });

  it("autosaves one completed row when focus leaves it", async () => {
    const workspace = emptyWorkspace();
    mocks.workspace = workspace;
    mocks.apply.mockImplementation(async (input: { document: ModelMappingWorkspaceDto["document"] }) => ({ ...workspace, document: input.document }));
    const { host, root, queryClient } = renderPanel();
    await act(async () => findButton(host, "添加模型")?.click());
    const row = host.querySelector('[aria-label^="模型映射行"]') as HTMLElement;
    setInput(row, '[aria-label^="实际请求模型"]', "gpt-4o-mini");
    setInput(row, '[aria-label^="上游目标模型"]', "deepseek-v4-flash");

    const outside = document.createElement("button");
    document.body.append(outside);
    await act(async () => {
      const targetInput = row.querySelector('[aria-label^="上游目标模型"]') as HTMLInputElement;
      targetInput.dispatchEvent(new FocusEvent("focusout", { bubbles: true, relatedTarget: outside }));
    });

    expect(mocks.apply).toHaveBeenCalledTimes(1);
    const savedDocument = mocks.apply.mock.calls[0][0].document;
    expect(savedDocument.profiles).toEqual([]);
    expect(savedDocument.rules[0].enabled).toBe(true);
    expect(savedDocument.rules[0].matcher).toEqual({ kind: "exact", model: "gpt-4o-mini" });
    expect(savedDocument.rules[0].conditions).toEqual({ endpointKinds: [], stream: "any", tools: "any", vision: "any", reasoning: "any" });
    expect(savedDocument.rules[0].action).toEqual({ kind: "map_fixed", target: { kind: "literal", upstreamModel: "deepseek-v4-flash" } });
    expect(host.querySelector('[aria-label="规则编辑器"]')).toBeNull();
    expect((host.querySelector('[aria-label^="实际请求模型"]') as HTMLInputElement).value).toBe("gpt-4o-mini");
    expect((host.querySelector('[aria-label^="上游目标模型"]') as HTMLInputElement).value).toBe("deepseek-v4-flash");
    expect(host.querySelector('[aria-label^="菜单显示名"]')).toBeNull();
    expect(host.querySelector('[aria-label^="保存模型映射"]')).toBeNull();
    expect(host.querySelector('[aria-label^="启用模型映射"]')).toBeNull();

    await act(async () => root.unmount());
    queryClient.clear();
  });

  it("keeps an incomplete new row quiet when focus leaves it", async () => {
    const workspace = emptyWorkspace();
    mocks.workspace = workspace;
    const { host, root, queryClient } = renderPanel();
    await act(async () => findButton(host, "添加模型")?.click());
    const row = host.querySelector('[aria-label^="模型映射行"]') as HTMLElement;
    const outside = document.createElement("button");
    document.body.append(outside);

    await act(async () => {
      const requestInput = row.querySelector('[aria-label^="实际请求模型"]') as HTMLInputElement;
      requestInput.dispatchEvent(new FocusEvent("focusout", { bubbles: true, relatedTarget: outside }));
    });

    expect(mocks.apply).not.toHaveBeenCalled();
    expect(host.textContent).not.toContain("请填写实际请求模型");
    expect(host.textContent).not.toContain("请填写上游目标模型");

    await act(async () => root.unmount());
    queryClient.clear();
  });

  it("does not expose old complex rule controls and requires an explicit cleanup", async () => {
    const workspace = phase2Workspace();
    mocks.workspace = workspace;
    const { host, root, queryClient } = renderPanel();
    expect(host.textContent).toContain("检测到 1 条旧版复杂规则");
    expect(findButton(host, "清理旧规则")).toBeTruthy();
    expect(host.querySelector('[aria-label^="模型映射行"]')).toBeNull();
    expect(host.querySelector('[aria-label^="更多模型映射设置"]')).toBeNull();
    expect(host.querySelector('[aria-label^="匹配方式"]')).toBeNull();
    expect(host.querySelector('[aria-label^="优先级"]')).toBeNull();
    expect(host.textContent).not.toContain("映射到目标模型");
    expect(host.textContent).not.toContain("保留原模型");
    expect(host.textContent).not.toContain("拒绝请求");
    expect(host.textContent).not.toContain("多个目标回退");

    await act(async () => root.unmount());
    queryClient.clear();
  });

  it("cleans old complex rules only after confirmation", async () => {
    const workspace = phase2Workspace();
    mocks.workspace = workspace;
    mocks.apply.mockImplementation(async (input: { document: ModelMappingWorkspaceDto["document"] }) => ({ ...workspace, document: input.document }));
    const { host, root, queryClient } = renderPanel();

    await act(async () => findButton(host, "清理旧规则")?.click());
    expect(mocks.apply).not.toHaveBeenCalled();
    await act(async () => findButton(document.body, "清理规则")?.click());

    expect(mocks.apply).toHaveBeenCalledTimes(1);
    expect(mocks.apply.mock.calls[0][0].document.rules).toEqual([]);
    expect(mocks.apply.mock.calls[0][0].document.profiles).toEqual(workspace.document.profiles);

    await act(async () => root.unmount());
    queryClient.clear();
  });

  it("does not alter old rules while saving a simple mapping", async () => {
    const workspace = phase2Workspace();
    workspace.document.rules[0].enabled = false;
    mocks.workspace = workspace;
    mocks.apply.mockImplementation(async (input: { document: ModelMappingWorkspaceDto["document"] }) => ({ ...workspace, document: input.document }));
    const { host, root, queryClient } = renderPanel();

    await act(async () => findButton(host, "添加模型")?.click());
    const row = host.querySelector('[aria-label^="模型映射行"]') as HTMLElement;
    setInput(row, '[aria-label^="实际请求模型"]', "gpt-4o-mini");
    setInput(row, '[aria-label^="上游目标模型"]', "deepseek-v4-flash");
    const outside = document.createElement("button");
    document.body.append(outside);
    await act(async () => {
      const targetInput = row.querySelector('[aria-label^="上游目标模型"]') as HTMLInputElement;
      targetInput.dispatchEvent(new FocusEvent("focusout", { bubbles: true, relatedTarget: outside }));
    });

    const savedRules = mocks.apply.mock.calls[0][0].document.rules;
    expect(savedRules.find((rule: { id: string }) => rule.id === "rule-fallback")).toMatchObject({ enabled: false });

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

    await act(async () => findButton(host, "添加模型")?.click());
    const requestPicker = host.querySelector('button[aria-label^="实际请求模型"][aria-label$="候选模型"]') as HTMLButtonElement;
    const targetPicker = host.querySelector('button[aria-label^="上游目标模型"][aria-label$="候选模型"]') as HTMLButtonElement;
    expect(requestPicker).toBeTruthy();
    expect(targetPicker).toBeTruthy();
    await act(async () => requestPicker.click());
    const requestOptions = Array.from(document.body.querySelectorAll('[role="listbox"] [role="option"]'));
    expect(requestOptions.map((option) => option.textContent)).toEqual(["claude-sonnet", "gpt-4o-mini", "shared-model"]);

    await act(async () => {
      requestOptions.find((option) => option.textContent === "gpt-4o-mini")?.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    });
    expect((host.querySelector('[aria-label^="实际请求模型"]') as HTMLInputElement).value).toBe("gpt-4o-mini");

    await act(async () => targetPicker.click());
    const targetOptions = Array.from(document.body.querySelectorAll('[role="listbox"] [role="option"]'));
    await act(async () => targetOptions.find((option) => option.textContent === "shared-model")?.dispatchEvent(new MouseEvent("click", { bubbles: true })));
    expect((host.querySelector('[aria-label^="上游目标模型"]') as HTMLInputElement).value).toBe("shared-model");

    await act(async () => requestPicker.click());
    const searchInput = document.body.querySelector('input[aria-label$="候选模型 搜索"]') as HTMLInputElement;
    expect(searchInput).toBeTruthy();
    setInput(document.body, 'input[aria-label$="候选模型 搜索"]', "gpt-4o");
    expect(Array.from(document.body.querySelectorAll('[role="listbox"] [role="option"]')).map((option) => option.textContent)).toEqual(["gpt-4o-mini"]);

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
