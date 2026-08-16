// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { describe, expect, it, vi } from "vitest";
import type { StationKeyCapabilities } from "@/lib/types/routing";
import type { KeyPoolItem } from "@/lib/types/stationKeys";
import {
  buildConnectivityConsoleLines,
  buildKeyConnectivityModelOptions,
  DEFAULT_KEY_CONNECTIVITY_TEST_MODEL,
  KeyConnectivityTestDialog,
} from "./KeyConnectivityTestDialog";

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function capabilities(overrides: Partial<StationKeyCapabilities> = {}): StationKeyCapabilities {
  return {
    stationKeyId: "key-1",
    supportsChatCompletions: true,
    supportsResponses: true,
    supportsEmbeddings: false,
    supportsStream: true,
    supportsTools: true,
    supportsVision: false,
    supportsReasoning: false,
    modelAllowlist: [],
    modelBlocklist: [],
    preferredModels: [],
    onlyUseAsBackup: false,
    routingTags: [],
    updatedAt: "2026-01-01T00:00:00Z",
    ...overrides,
  };
}

describe("KeyConnectivityTestDialog", () => {
  it("builds connectivity model options from scoped capabilities without duplicates", () => {
    expect(
      buildKeyConnectivityModelOptions(capabilities({
        modelAllowlist: [" custom-model ", "CUSTOM-model", "", "gpt-4.1"],
        preferredModels: ["preferred-ignored"],
      })),
    ).toEqual([
      { value: "custom-model", label: "custom-model" },
      { value: "gpt-4.1", label: "GPT-4.1" },
    ]);

    expect(buildKeyConnectivityModelOptions(null)[0]?.value).toBe(DEFAULT_KEY_CONNECTIVITY_TEST_MODEL);
  });

  it("delegates selected model test requests to the page handler", async () => {
    const onTest = vi.fn();
    const onDisplayedResponseTextChange = vi.fn();
    const host = document.createElement("div");
    const root = createRoot(host);

    await act(async () =>
      root.render(
        <KeyConnectivityTestDialog
          capabilities={capabilities({ modelAllowlist: ["custom-model"] })}
          displayedResponseText=""
          error={null}
          item={{ id: "key-1", name: "Primary", enabled: true } as KeyPoolItem}
          progressLabel={null}
          result={null}
          streamFallbackReason={null}
          testing={false}
          onClose={vi.fn()}
          onDisplayedResponseTextChange={onDisplayedResponseTextChange}
          onTest={onTest}
        />,
      ),
    );

    expect(onDisplayedResponseTextChange).toHaveBeenCalledWith("");

    const buttons = [...document.body.querySelectorAll<HTMLButtonElement>("button")];
    await act(async () => buttons[buttons.length - 1]!.dispatchEvent(new MouseEvent("click", { bubbles: true })));

    expect(onTest).toHaveBeenCalledWith("custom-model", "standard_api");

    await act(async () => root.unmount());
  });

  it("distinguishes Responses stream degradation from a Chat protocol fallback", () => {
    const common = {
      item: { id: "key-1", name: "Primary", enabled: true } as KeyPoolItem,
      model: "test-model",
      selectedModelLabel: "test-model",
      clientProfile: "codex_cli_compat" as const,
      testing: false,
      error: null,
      displayedResponseText: "ok",
      streamFallbackReason: "stream_transport_failed",
      progressLabel: null,
      responseTypingComplete: true,
    };
    const responsesFallback = buildConnectivityConsoleLines({
      ...common,
      result: {
        stationKeyId: "key-1",
        ok: true,
        statusCode: 200,
        durationMs: 42,
        model: "test-model",
        message: "ok",
        validatedProtocol: "responses",
        clientProfile: "codex_cli_compat",
        responseMode: "non_stream_fallback",
        streamFallbackReason: "stream_transport_failed",
      },
    });
    const chatFallback = buildConnectivityConsoleLines({
      ...common,
      result: {
        stationKeyId: "key-1",
        ok: true,
        statusCode: 200,
        durationMs: 42,
        model: "test-model",
        message: "ok",
        validatedProtocol: "chat_completions",
        clientProfile: "standard_api",
        responseMode: "stream",
        streamFallbackReason: null,
      },
    });

    expect(responsesFallback.map((line) => line.text)).toContain("已降级为非流式 Responses，测试成功");
    expect(chatFallback.map((line) => line.text)).toContain("已回退到 Chat Completions，测试成功");
    expect(chatFallback.map((line) => line.text)).toContain("协议 Chat Completions · 标准 API 请求档案 · 42ms");
  });
});
