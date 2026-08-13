// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { describe, expect, it, vi } from "vitest";
import type { Station } from "@/lib/types/stations";
import type { StationAssetRow } from "../../stationAssetViewModels";
import { StationAssetListRow } from "./StationAssetRows";

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

function station(overrides: Partial<Station> = {}): Station {
  return {
    id: "station-1",
    name: "Relay",
    stationType: "sub2api",
    websiteUrl: "https://console.example/path",
    apiBaseUrl: "https://api.example/v1",
    endpointRevision: 1,
    collectorProxyMode: "inherit",
    collectorProxyUrl: null,
    apiKeyMasked: "sk-***",
    apiKeyPresent: true,
    keyCount: 1,
    enabled: true,
    priority: 0,
    creditPerCny: 1,
    balanceRaw: null,
    balanceCny: 8,
    lowBalanceThresholdCny: null,
    collectionIntervalMinutes: 5,
    status: "healthy",
    latencyMs: null,
    lastCheckedAt: null,
    lastPricingFetchedAt: null,
    note: null,
    createdAt: "2026-01-01T00:00:00Z",
    updatedAt: "2026-01-01T00:00:00Z",
    ...overrides,
  };
}

function stationAssetRow(overrides: Partial<StationAssetRow> = {}): StationAssetRow {
  const rowStation = station();
  return {
    station: rowStation,
    enabledKeyCount: 1,
    warningKeyCount: 0,
    groupIssueCount: 0,
    groupIssueReasons: [],
    missingRateCount: 0,
    balanceFactsReady: true,
    latestBalance: null,
    currentBalance: {
      stationId: rowStation.id,
      value: 8,
      currency: "CNY",
      lowBalanceThreshold: null,
      snapshotId: null,
      status: null,
      source: "station_cache",
      sourceLabel: "station_cache",
      updatedAt: null,
      collectedAt: null,
      sourceSnapshot: null,
    },
    latestSnapshot: null,
    riskEvents: [],
    rateChips: [],
    participatesInRouting: true,
    ...overrides,
  };
}

describe("StationAssetRows", () => {
  it("delegates row and website actions to page-owned callbacks", async () => {
    const onOpen = vi.fn();
    const onOpenWebsite = vi.fn();
    const host = document.createElement("div");
    const root = createRoot(host);
    const row = stationAssetRow();

    await act(async () =>
      root.render(
        <StationAssetListRow
          actionDisabled={false}
          active={false}
          loadingAction={null}
          row={row}
          onAuthorize={vi.fn()}
          onCollect={vi.fn()}
          onDelete={vi.fn()}
          onEdit={vi.fn()}
          onOpen={onOpen}
          onOpenWebsite={onOpenWebsite}
          onRefreshBalance={vi.fn()}
        />,
      ),
    );

    expect(host.textContent).toContain("8.00USD");

    const rowButton = host.querySelector<HTMLElement>('[role="button"]')!;
    await act(async () => rowButton.dispatchEvent(new MouseEvent("click", { bubbles: true })));
    expect(onOpen).toHaveBeenCalledWith(row.station);

    const websiteButton = host.querySelector<HTMLButtonElement>('button[aria-label="在浏览器打开 Relay"]')!;
    await act(async () => websiteButton.dispatchEvent(new MouseEvent("click", { bubbles: true })));
    expect(onOpenWebsite).toHaveBeenCalledWith(row.station);
    expect(onOpen).toHaveBeenCalledTimes(1);

    await act(async () => root.unmount());
  });

  it("keeps actions on other station rows available while one row is collecting", async () => {
    const busyHost = document.createElement("div");
    const idleHost = document.createElement("div");
    const busyRoot = createRoot(busyHost);
    const idleRoot = createRoot(idleHost);

    await act(async () =>
      busyRoot.render(
        <StationAssetListRow
          actionDisabled
          active={false}
          loadingAction="collect"
          row={stationAssetRow()}
          onAuthorize={vi.fn()}
          onCollect={vi.fn()}
          onDelete={vi.fn()}
          onEdit={vi.fn()}
          onOpen={vi.fn()}
          onOpenWebsite={vi.fn()}
          onRefreshBalance={vi.fn()}
        />,
      ),
    );
    await act(async () =>
      idleRoot.render(
        <StationAssetListRow
          actionDisabled={false}
          active={false}
          loadingAction={null}
          row={stationAssetRow({ station: station({ id: "station-2", name: "Relay Two" }) })}
          onAuthorize={vi.fn()}
          onCollect={vi.fn()}
          onDelete={vi.fn()}
          onEdit={vi.fn()}
          onOpen={vi.fn()}
          onOpenWebsite={vi.fn()}
          onRefreshBalance={vi.fn()}
        />,
      ),
    );

    const busyCollectButton = busyHost.querySelector<HTMLButtonElement>('button[aria-label="采集信息 Relay"]');
    const idleCollectButton = idleHost.querySelector<HTMLButtonElement>('button[aria-label="采集信息 Relay Two"]');

    expect(busyCollectButton?.disabled).toBe(true);
    expect(idleCollectButton?.disabled).toBe(false);

    await act(async () => busyRoot.unmount());
    await act(async () => idleRoot.unmount());
  });
});
