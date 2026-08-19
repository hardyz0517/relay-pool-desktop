// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, describe, expect, it, vi } from "vitest";
import { ToastProvider } from "@/components/ui";
import { LocalRoutingEditTab } from "./LocalRoutingEditTab";

vi.mock("./LocalRoutingSettingsEditor", () => ({
  LocalRoutingSettingsEditor: () => <div data-testid="routing-settings" />,
}));

vi.mock("./ModelMappingPanel", () => ({
  ModelMappingPanel: () => <div data-testid="model-mapping" />,
}));

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

afterEach(() => {
  document.body.innerHTML = "";
  vi.restoreAllMocks();
});

describe("LocalRoutingEditTab", () => {
  it("keeps candidate ordering out of the settings page", async () => {
    const queryClient = new QueryClient();
    const host = document.createElement("div");
    document.body.append(host);
    const root = createRoot(host);

    await act(async () => {
      root.render(
        <QueryClientProvider client={queryClient}>
          <ToastProvider>
            <LocalRoutingEditTab />
          </ToastProvider>
        </QueryClientProvider>,
      );
    });

    expect(host.querySelector('[data-testid="routing-settings"]')).toBeTruthy();
    expect(host.querySelector('[data-testid="model-mapping"]')).toBeTruthy();
    expect(host.textContent).not.toContain("候选预览与顺序修正");
    expect(host.querySelector('[aria-label="调整候选顺序"]')).toBeNull();

    await act(async () => root.unmount());
    queryClient.clear();
  });
});
