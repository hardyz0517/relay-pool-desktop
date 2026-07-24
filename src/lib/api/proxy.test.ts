import { beforeEach, describe, expect, it, vi } from "vitest";

const generated = vi.hoisted(() => ({
  clearRequestLogs: vi.fn(),
  getProxyStatus: vi.fn(),
  listRequestLogs: vi.fn(),
  startLocalProxy: vi.fn(),
}));
const transport = vi.hoisted(() => ({ invoke: vi.fn() }));

vi.mock("@/lib/bridge/generated", () => generated);
vi.mock("@/lib/bridge/transport", () => transport);

import { clearRequestLogs, listRequestLogs, startLocalProxy } from "./proxy";

describe("request log generated transport cutover", () => {
  beforeEach(() => {
    vi.stubGlobal("window", { dispatchEvent: vi.fn() });
    vi.stubGlobal("CustomEvent", class {});
    generated.clearRequestLogs.mockReset().mockResolvedValue(undefined);
    generated.listRequestLogs.mockReset().mockResolvedValue([]);
    generated.startLocalProxy.mockReset().mockResolvedValue({
      running: true,
      lifecycle: "running",
      bindAddr: "127.0.0.1",
      port: 8787,
      startedAt: "1700000000000",
      lastError: null,
      activeRequests: 0,
      requestCount: 0,
    });
    transport.invoke.mockReset().mockResolvedValue(undefined);
  });

  it("routes both request-log commands through generated wrappers", async () => {
    await listRequestLogs();
    await clearRequestLogs();
    expect(generated.listRequestLogs).toHaveBeenCalledWith();
    expect(generated.clearRequestLogs).toHaveBeenCalledWith();
  });

  it("routes proxy start through the generated wrapper", async () => {
    await startLocalProxy();
    expect(generated.startLocalProxy).toHaveBeenCalledWith();
    expect(transport.invoke).not.toHaveBeenCalled();
  });
});
