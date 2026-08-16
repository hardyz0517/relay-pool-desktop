// @vitest-environment jsdom

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ToastProvider } from "@/components/ui/ToastProvider";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

const useActivityQuery = vi.hoisted(() => vi.fn());
const getActiveBackendClient = vi.hoisted(() => vi.fn());

vi.mock("@/lib/query/useActivityQuery", () => ({ useActivityQuery }));
vi.mock("@/lib/bridge/activeBackendClient", () => ({ getActiveBackendClient }));

import { RuntimeDiagnosticsPage } from "./RuntimeDiagnosticsPage";

function renderPage() {
  const host = document.createElement("div");
  document.body.appendChild(host);
  const root: Root = createRoot(host);
  return { host, root };
}

function TestShell() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return (
    <QueryClientProvider client={client}>
      <ToastProvider>
        <RuntimeDiagnosticsPage />
      </ToastProvider>
    </QueryClientProvider>
  );
}

describe("RuntimeDiagnosticsPage", () => {
  let host: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    ({ host, root } = renderPage());
    useActivityQuery.mockReset();
    getActiveBackendClient.mockReset();
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
  });

  it("fails closed when the active backend has no diagnostics capability", async () => {
    getActiveBackendClient.mockReturnValue({ runtimeDiagnostics: undefined });
    useActivityQuery.mockReturnValue({ isPending: false, data: undefined, isError: false });
    await act(async () => root.render(<TestShell />));
    expect(host.textContent).toContain("当前运行模式不支持诊断");
    expect(host.textContent).toContain("开发者模式");
  });

  it("renders safe event fields and keeps filters bounded", async () => {
    getActiveBackendClient.mockReturnValue({
      runtimeDiagnostics: {
        readRuntimeDiagnostics: vi.fn(),
        exportRuntimeSupportBundle: vi.fn(),
      },
    });
    useActivityQuery.mockReturnValue({
      isPending: false,
      isError: false,
      data: {
        events: [{
          atMs: 1_700_000_000_000,
          sequence: 7,
          level: "warn",
          component: "runtime",
          eventCode: "runtime.log_event.dropped",
          outcome: "degraded",
          durationMs: 12,
          sessionId: "ses_0123456789abcdef0123456789abcdef",
        }],
        nextSegmentIndex: null,
        nextLineIndex: null,
        issueCount: 0,
        sinkDegraded: false,
      },
    });
    await act(async () => root.render(<TestShell />));
    expect(host.textContent).toContain("runtime.log_event.dropped");
    expect(host.textContent).toContain("12 ms");
    const inputs = host.querySelectorAll("input");
    expect(inputs.length).toBe(3);
    for (const input of inputs) expect(input.getAttribute("maxlength")).toBe("96");
  });

  it("reports support bundle success and treats save-dialog cancellation as a no-op", async () => {
    const exportRuntimeSupportBundle = vi.fn().mockResolvedValueOnce({
      eventCount: 1,
      issueCount: 0,
    }).mockResolvedValueOnce(null);
    getActiveBackendClient.mockReturnValue({ runtimeDiagnostics: { exportRuntimeSupportBundle } });
    useActivityQuery.mockReturnValue({ isPending: false, isError: false, data: { events: [], nextSegmentIndex: null, nextLineIndex: null, issueCount: 0, sinkDegraded: false } });
    await act(async () => root.render(<TestShell />));
    const exportButton = Array.from(host.querySelectorAll("button")).find((button) => button.textContent?.includes("导出诊断包"));
    expect(exportButton).toBeDefined();

    await act(async () => exportButton!.dispatchEvent(new MouseEvent("click", { bubbles: true })));
    expect(exportRuntimeSupportBundle).toHaveBeenCalledTimes(1);
    expect(host.textContent).toContain("诊断包已导出");

    await act(async () => exportButton!.dispatchEvent(new MouseEvent("click", { bubbles: true })));
    expect(exportRuntimeSupportBundle).toHaveBeenCalledTimes(2);
    expect(host.textContent).not.toContain("导出诊断包失败");
  });

  it("shows a bounded failure message when support bundle export fails", async () => {
    const exportRuntimeSupportBundle = vi.fn().mockRejectedValue(new Error("authorization: sk-secret"));
    getActiveBackendClient.mockReturnValue({ runtimeDiagnostics: { exportRuntimeSupportBundle } });
    useActivityQuery.mockReturnValue({ isPending: false, isError: false, data: { events: [], nextSegmentIndex: null, nextLineIndex: null, issueCount: 0, sinkDegraded: false } });
    await act(async () => root.render(<TestShell />));
    const exportButton = Array.from(host.querySelectorAll("button")).find((button) => button.textContent?.includes("导出诊断包"));
    await act(async () => exportButton?.dispatchEvent(new MouseEvent("click", { bubbles: true })));
    expect(host.textContent).toContain("导出诊断包失败");
    expect(host.textContent).toContain("请确认开发者模式仍处于开启状态");
    expect(host.textContent).not.toContain("sk-secret");
  });
});
