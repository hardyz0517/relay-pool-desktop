// @vitest-environment jsdom
import { act, type FormEvent } from "react";
import { createRoot } from "react-dom/client";
import { describe, expect, it, vi } from "vitest";
import { ToastProvider } from "@/components/ui/ToastProvider";
import type { StationKey } from "@/lib/types/stationKeys";
import type { Station } from "@/lib/types/stations";
import { emptyForm, emptyKeyForm } from "./formModel";
import { DetailBody, StationDialogs } from "./StationDialogs";

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

function stationKey(overrides: Partial<StationKey> = {}): StationKey {
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
    ...overrides,
  };
}

describe("StationDialogs", () => {
  it("delegates station form changes and submit to page handlers", async () => {
    const onChange = vi.fn();
    const onSubmit = vi.fn((event: FormEvent<HTMLFormElement>) => event.preventDefault());
    const host = document.createElement("div");
    const root = createRoot(host);

    await act(async () =>
      root.render(
        <StationDialogs
          activeDialogStation={null}
          actionSaving={false}
          credentials={null}
          dialogMode="create"
          form={emptyForm}
          keyDialogOpen={false}
          keyForm={emptyKeyForm}
          saving={false}
          onChange={onChange}
          onClose={vi.fn()}
          onKeyDialogOpenChange={vi.fn()}
          onKeyFormChange={vi.fn()}
          onKeySave={vi.fn()}
          onRemoveLoginInfo={vi.fn()}
          onSubmit={onSubmit}
        />,
      ),
    );

    const nameInput = document.body.querySelector<HTMLInputElement>("#station-form input")!;
    const valueSetter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
    valueSetter.call(nameInput, "Relay Next");
    await act(async () => nameInput.dispatchEvent(new Event("input", { bubbles: true })));

    expect(onChange).toHaveBeenCalledWith({
      ...emptyForm,
      name: "Relay Next",
    });

    const form = document.body.querySelector<HTMLFormElement>("#station-form")!;
    await act(async () => form.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true })));
    expect(onSubmit).toHaveBeenCalledOnce();

    await act(async () => root.unmount());
  });

  it("delegates detail key actions to page handlers", async () => {
    const onDeleteKey = vi.fn();
    const onEditKey = vi.fn();
    const host = document.createElement("div");
    const root = createRoot(host);
    const key = stationKey();

    await act(async () =>
      root.render(
        <ToastProvider>
          <DetailBody
            activeDialogStation={station()}
            changeEvents={[]}
            collectorRuns={[]}
            credentials={null}
            groupBindings={[]}
            keyCountLabel="1 把"
            rateRecords={[]}
            snapshot={null}
            snapshots={[]}
            stationKeys={[key]}
            onDeleteKey={onDeleteKey}
            onEditKey={onEditKey}
          />
        </ToastProvider>,
      ),
    );

    const buttons = host.querySelectorAll<HTMLButtonElement>("button");
    await act(async () => buttons[0].dispatchEvent(new MouseEvent("click", { bubbles: true })));
    await act(async () => buttons[1].dispatchEvent(new MouseEvent("click", { bubbles: true })));

    expect(onEditKey).toHaveBeenCalledWith(key);
    expect(onDeleteKey).toHaveBeenCalledWith(key);

    await act(async () => root.unmount());
  });
});
