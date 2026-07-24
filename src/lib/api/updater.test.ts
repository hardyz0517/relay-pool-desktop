import { beforeEach, describe, expect, it, vi } from "vitest";

const app = vi.hoisted(() => ({ getVersion: vi.fn() }));
const core = vi.hoisted(() => ({ isTauri: vi.fn() }));
const generated = vi.hoisted(() => ({
  inspectLatestUpdateManifest: vi.fn(),
  updaterNetworkConfig: vi.fn(),
}));
const transport = vi.hoisted(() => ({ invoke: vi.fn() }));
const process = vi.hoisted(() => ({ relaunch: vi.fn() }));
const pluginUpdater = vi.hoisted(() => ({ check: vi.fn() }));
const coordinator = vi.hoisted(() => ({ coordinateUpdateCheck: vi.fn() }));

vi.mock("@tauri-apps/api/app", () => app);
vi.mock("@tauri-apps/api/core", () => core);
vi.mock("@/lib/bridge/generated", () => generated);
vi.mock("@/lib/bridge/transport", () => transport);
vi.mock("@tauri-apps/plugin-process", () => process);
vi.mock("@tauri-apps/plugin-updater", () => pluginUpdater);
vi.mock("@/lib/api/updaterCheckCoordinator", () => coordinator);

import { checkForAppUpdate } from "./updater";

describe("updater generated transport cutover", () => {
  beforeEach(() => {
    vi.stubGlobal("window", {
      clearTimeout,
      setTimeout,
    });
    app.getVersion.mockReset().mockResolvedValue("0.3.2");
    core.isTauri.mockReset().mockReturnValue(true);
    generated.updaterNetworkConfig.mockReset().mockResolvedValue({ proxyUrl: null });
    generated.inspectLatestUpdateManifest.mockReset().mockResolvedValue({
      relation: "current_or_older",
      version: "0.3.2",
      notes: null,
    });
    transport.invoke.mockReset().mockRejectedValue(new Error("legacy transport invoked"));
    coordinator.coordinateUpdateCheck.mockReset().mockImplementation(async (options) => {
      await options.inspectPublished(options.currentVersion);
      return { kind: "current", currentVersion: options.currentVersion };
    });
    pluginUpdater.check.mockReset().mockResolvedValue(null);
  });

  it("routes updater backend reads through generated wrappers", async () => {
    await expect(checkForAppUpdate()).resolves.toEqual({
      kind: "current",
      currentVersion: "0.3.2",
    });

    expect(generated.updaterNetworkConfig).toHaveBeenCalledWith();
    expect(generated.inspectLatestUpdateManifest).toHaveBeenCalledWith({
      currentVersion: "0.3.2",
    });
    expect(transport.invoke).not.toHaveBeenCalled();
  });
});
