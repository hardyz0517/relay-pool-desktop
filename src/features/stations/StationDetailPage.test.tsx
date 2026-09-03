// @vitest-environment jsdom

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { StationDetailReadModelEnvelope } from "@/lib/types/stationAssets";
import type { Station } from "@/lib/types/stations";
import { StationDetailPage } from "./StationDetailPage";

const mocks = vi.hoisted(() => ({
  loadStationDetail: vi.fn(),
}));

vi.mock("@/components/ui", () => ({
  Button: ({ children }: { children: unknown }) => children,
  EmptyState: ({ title }: { title: string }) => <div>{title}</div>,
  useToast: () => ({ info: vi.fn(), error: vi.fn(), success: vi.fn() }),
}));

vi.mock("@/lib/api/collector", () => ({
  collectStationTask: vi.fn(),
  startManualAuthorization: vi.fn(),
}));

vi.mock("@/lib/api/stations", () => ({
  loadStationDetail: mocks.loadStationDetail,
  openStationWebsite: vi.fn(),
}));

vi.mock("@/lib/query/stationCollectionQuerySynchronization", () => ({
  reconcileStationDetailReadModel: vi.fn().mockResolvedValue({
    refreshed: true,
    invalidatedKeys: [],
    ignoredScopes: [],
    errors: [],
  }),
}));

vi.mock("./components/StationDetailContent", () => ({
  StationDetailContent: ({
    viewModel,
  }: {
    viewModel: { station: Station; statusLabel: string };
  }) => (
    <div
      data-testid="station-detail"
      data-station-id={viewModel.station.id}
      data-collection-status={viewModel.station.collectionSummary?.status}
      data-authorization-status={viewModel.station.authorizationSummary?.status}
    >
      {viewModel.station.name} · {viewModel.statusLabel}
    </div>
  ),
}));

vi.mock("./components/StationPublishedStatusSection", () => ({
  StationPublishedStatusSection: () => null,
}));

vi.mock("./components/RechargeDialog", () => ({ RechargeDialog: () => null }));

vi.mock("./useStationPublishedStatus", () => ({
  useStationPublishedStatus: () => ({
    workspace: null,
    isLoading: false,
    isError: false,
    isRefreshing: false,
    isRefreshError: false,
    refresh: vi.fn(),
    retryWorkspace: vi.fn(),
  }),
}));

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let host: HTMLDivElement;
let root: Root;
let queryClient: QueryClient;

beforeEach(() => {
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  mocks.loadStationDetail.mockReset();
});

afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
  queryClient.clear();
});

describe("StationDetailPage", () => {
  it("uses the asset read model and isolates a late response after switching stations", async () => {
    const stationA = deferred<StationDetailReadModelEnvelope>();
    const stationB = deferred<StationDetailReadModelEnvelope>();
    mocks.loadStationDetail
      .mockReturnValueOnce(stationA.promise)
      .mockReturnValueOnce(stationB.promise);

    await renderPage("station-a");
    await renderPage("station-b");

    await act(async () => {
      stationB.resolve(envelope(station("station-b", "站点 B")));
      await stationB.promise;
    });
    await waitForStation("station-b");

    await act(async () => {
      stationA.resolve(envelope(station("station-a", "站点 A")));
      await stationA.promise;
    });
    await waitForStation("station-b");

    expect(host.textContent).toContain("站点 B");
    expect(mocks.loadStationDetail).toHaveBeenNthCalledWith(1, "station-a");
    expect(mocks.loadStationDetail).toHaveBeenNthCalledWith(2, "station-b");
  });

  it("shows a closed error state when the detail command fails", async () => {
    mocks.loadStationDetail.mockRejectedValueOnce(new Error("详情读取失败"));

    await renderPage("station-a");
    await waitForText("详情读取失败");

    expect(host.querySelector('[data-testid="station-detail"]')).toBeNull();
  });

  it("rejects a malformed envelope whose station identity does not match", async () => {
    mocks.loadStationDetail.mockResolvedValueOnce(envelope(station("station-b", "站点 B")));

    await renderPage("station-a");
    await waitForText("未找到中转站");

    expect(host.querySelector('[data-testid="station-detail"]')).toBeNull();
  });

  it("handles an empty station selection without issuing a detail command", async () => {
    await renderPage(null);

    expect(host.textContent).toContain("未选择中转站");
    expect(mocks.loadStationDetail).not.toHaveBeenCalled();
  });

  it("consumes typed summaries from the real asset DTO shape", async () => {
    const value = station("station-a", "站点 A");
    value.status = "warning";
    expect(value.collectionSummary).toBeUndefined();
    expect(value.authorizationSummary).toBeUndefined();
    mocks.loadStationDetail.mockResolvedValueOnce(envelope(value));

    await renderPage("station-a");
    await waitForStation("station-a");

    const detail = host.querySelector('[data-testid="station-detail"]');
    expect(detail?.getAttribute("data-collection-status")).toBe("healthy");
    expect(detail?.getAttribute("data-authorization-status")).toBe("valid");
    expect(detail?.textContent).toContain("采集正常");
    expect(detail?.textContent).not.toContain("采集需关注");
  });
});

async function renderPage(stationId: string | null) {
  await act(async () => {
    root.render(
      <QueryClientProvider client={queryClient}>
        <StationDetailPage
          stationId={stationId}
          onBack={vi.fn()}
          onEditProvider={vi.fn()}
        />
      </QueryClientProvider>,
    );
  });
}

async function waitForText(expected: string) {
  for (let attempt = 0; attempt < 10; attempt += 1) {
    if (host.textContent?.includes(expected)) {
      return;
    }
    await act(async () => {
      await new Promise<void>((resolve) => setTimeout(resolve, 0));
    });
  }
  expect(host.textContent).toContain(expected);
}

async function waitForStation(stationId: string) {
  for (let attempt = 0; attempt < 10; attempt += 1) {
    if (host.querySelector(`[data-station-id="${stationId}"]`)) {
      return;
    }
    await act(async () => {
      await new Promise<void>((resolve) => setTimeout(resolve, 0));
    });
  }
  expect(host.querySelector(`[data-station-id="${stationId}"]`)).not.toBeNull();
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((resolvePromise) => {
    resolve = resolvePromise;
  });
  return { promise, resolve };
}

function envelope(value: Station): StationDetailReadModelEnvelope {
  return {
    schemaVersion: 1,
    generatedAtMs: 1,
    domainRevision: 1,
    page: { limit: 1, returned: 1, nextCursor: null },
    data: {
      asset: {
        station: value,
        keys: [],
        groupIdentityHashes: [],
        collectionSummary: { status: "healthy", reasonCodes: [], revision: 1 },
        authorizationSummary: {
          status: "valid",
          credentialRevision: 1,
          reasonCode: null,
          revision: 1,
        },
      },
      credentials: {
        stationId: value.id,
        loginUsername: null,
        passwordPresent: false,
        rememberPassword: false,
        loginStatus: "unknown",
        loginError: null,
        lastLoginAt: null,
        sessionStatus: "missing",
        sessionExpiresAt: null,
        accessTokenPresent: false,
        refreshTokenPresent: false,
        cookiePresent: false,
        sessionSource: null,
        newapiUserId: null,
        tokenExpiresAt: null,
        tokenRefreshedAt: null,
        updatedAt: null,
      },
      groupBindings: [],
      groupRates: [],
      collectorRuns: [],
      latestSnapshot: null,
      balances: [],
      incidents: [],
      limits: {
        groupBindings: 500,
        groupRates: 500,
        collectorRuns: 100,
        balances: 200,
        incidents: 100,
      },
    },
  };
}

function station(id: string, name: string): Station {
  return {
    id,
    name,
    stationType: "sub2api",
    websiteUrl: "https://example.test",
    apiBaseUrl: "https://example.test/v1",
    endpointRevision: 1,
    collectorProxyMode: "inherit",
    collectorProxyUrl: null,
    apiKeyMasked: "sk-test...fake",
    apiKeyPresent: true,
    keyCount: 0,
    enabled: true,
    priority: 0,
    creditPerCny: 1,
    balanceRaw: null,
    balanceCny: null,
    lowBalanceThresholdCny: null,
    collectionIntervalMinutes: 30,
    status: "unchecked",
    latencyMs: null,
    lastCheckedAt: null,
    lastPricingFetchedAt: null,
    note: null,
    createdAt: "2026-01-01T00:00:00Z",
    updatedAt: "2026-01-01T00:00:00Z",
  };
}
