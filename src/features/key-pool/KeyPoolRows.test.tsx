// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { describe, expect, it, vi } from "vitest";
import type { KeyPoolItem } from "@/lib/types/stationKeys";
import {
  formatStationBaseUrl,
  KeyRowContent,
  keyPoolGridClassName,
  keyPoolTableClassName,
} from "./KeyPoolRows";

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function keyPoolItem(overrides: Partial<KeyPoolItem> = {}): KeyPoolItem {
  return {
    id: "key-1",
    stationId: "station-1",
    name: "Primary",
    apiKeyMasked: "sk-***",
    apiKeyPresent: true,
    enabled: true,
    priority: 0,
    maxConcurrency: 4,
    loadFactor: null,
    schedulable: true,
    groupBindingId: null,
    groupIdHash: null,
    groupName: null,
    tierLabel: null,
    rateMultiplier: null,
    manualRateMultiplier: null,
    manualRateUpdatedAt: null,
    rateSource: null,
    rateCollectedAt: null,
    balanceScope: null,
    status: "healthy",
    lastCheckedAt: null,
    lastUsedAt: null,
    note: null,
    createdAt: "2026-01-01T00:00:00Z",
    updatedAt: "2026-01-01T00:00:00Z",
    stationName: "Relay",
    stationType: "sub2api",
    stationApiBaseUrl: "https://api.example/v1",
    stationEndpointRevision: 1,
    stationUpstreamApiFormat: "auto",
    capabilitySummary: [],
    modelScopeSummary: "",
    onlyUseAsBackup: false,
    cooldownUntil: null,
    successRate: null,
    avgLatencyMs: null,
    consecutiveFailures: 0,
    lastErrorSummary: null,
    endpointPingStatus: "unchecked",
    endpointPingMs: null,
    endpointPingCheckedAt: null,
    endpointPingError: null,
    ...overrides,
  };
}

describe("KeyPoolRows", () => {
  it("reserves one full-width table canvas and enough room for row actions", () => {
    expect(keyPoolTableClassName).toContain("min-w-[62rem]");
    expect(keyPoolGridClassName).toContain("w-full");
    expect(keyPoolGridClassName).toContain("10.5rem");
    expect(keyPoolGridClassName).not.toContain("min-w-[780px]");
  });

  it("formats row base URLs", () => {
    expect(formatStationBaseUrl("https://api.example/v1/chat")).toBe("https://api.example");
    expect(formatStationBaseUrl("not-a-url///")).toBe("not-a-url");
  });

  it("shows status only when an enabled monitor provides monitoring status", async () => {
    const host = document.createElement("div");
    const root = createRoot(host);
    const item = keyPoolItem();

    await act(async () => root.render(<KeyRowContent item={item} monitor={null} />));
    expect(host.querySelector('[aria-label="未开启监控 Primary"]')).not.toBeNull();
    expect(host.textContent).not.toContain("正常");

    await act(async () =>
      root.render(
        <KeyRowContent
          item={item}
          monitor={{ enabled: true } as never}
          monitorStatus={{ label: "正常", tone: "healthy" }}
        />,
      ),
    );
    expect(host.textContent).toContain("正常");

    await act(async () => root.unmount());
  });

  it("delegates row switches and icon actions to page handlers", async () => {
    const onToggleEnabled = vi.fn();
    const onToggleMonitoring = vi.fn();
    const onTestConnectivity = vi.fn();
    const onEdit = vi.fn();
    const onDelete = vi.fn();
    const host = document.createElement("div");
    const root = createRoot(host);
    const item = keyPoolItem();

    await act(async () =>
      root.render(
        <KeyRowContent
          item={item}
          monitor={{ enabled: true } as never}
          onDelete={onDelete}
          onEdit={onEdit}
          onTestConnectivity={onTestConnectivity}
          onToggleEnabled={onToggleEnabled}
          onToggleMonitoring={onToggleMonitoring}
        />,
      ),
    );

    const switches = host.querySelectorAll<HTMLButtonElement>('[role="switch"]');
    await act(async () => switches[0]!.dispatchEvent(new MouseEvent("click", { bubbles: true })));
    await act(async () => switches[1]!.dispatchEvent(new MouseEvent("click", { bubbles: true })));

    expect(onToggleEnabled).toHaveBeenCalledWith(item);
    expect(onToggleMonitoring).toHaveBeenCalledWith(item);

    const buttons = [...host.querySelectorAll<HTMLButtonElement>("button")];
    const [testButton, editButton, deleteButton] = buttons.slice(-3);
    await act(async () => testButton!.dispatchEvent(new MouseEvent("click", { bubbles: true })));
    await act(async () => editButton!.dispatchEvent(new MouseEvent("click", { bubbles: true })));
    await act(async () => deleteButton!.dispatchEvent(new MouseEvent("click", { bubbles: true })));

    expect(onTestConnectivity).toHaveBeenCalledWith(item);
    expect(onEdit).toHaveBeenCalledWith(item);
    expect(onDelete).toHaveBeenCalledWith(item);

    await act(async () => root.unmount());
  });
});
