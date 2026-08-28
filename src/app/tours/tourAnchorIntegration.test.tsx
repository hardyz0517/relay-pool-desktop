// @vitest-environment jsdom

import { act, type ReactNode } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { AppPageId, AppRouteId } from "@/lib/types/navigation";
import type { RoutingViewPreparationPort } from "@/features/routing/routingViewPreparation";
import { PUBLISHED_TOURS } from "./tourCatalog";
import { TourTargetResolver } from "./tourTargetResolver";

// The page components are intentionally real. Only their data/control-plane
// boundaries are replaced so the smoke test remains deterministic and local.
vi.mock("@/lib/query/useActivityQuery", () => ({
  useActivityQuery: () => ({
    data: undefined,
    error: null,
    isError: false,
    isFetching: false,
    isLoading: false,
    isPending: false,
  }),
}));

vi.mock("@/lib/updater/UpdaterProvider", () => ({
  useUpdater: () => ({
    state: { phase: "idle" },
    checkNow: vi.fn(),
    showUpdateDialog: vi.fn(),
  }),
}));

const queryClient = {
  invalidateQueries: vi.fn(async () => undefined),
  setQueryData: vi.fn(),
};

vi.mock("@tanstack/react-query", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@tanstack/react-query")>()),
  useQuery: () => ({ data: undefined }),
  useQueryClient: () => queryClient,
  useMutation: () => ({
    mutate: vi.fn(),
    mutateAsync: vi.fn(async () => undefined),
    isPending: false,
  }),
}));

vi.mock("@/components/ui/ToastProvider", () => ({
  ToastProvider: ({ children }: { children: unknown }) => children,
  useToast: () => ({
    show: vi.fn(),
    success: vi.fn(),
    error: vi.fn(),
    info: vi.fn(),
    loading: vi.fn(),
    dismiss: vi.fn(),
  }),
}));

vi.mock("@/features/settings/ThemeSettings", () => ({
  ThemeSettings: () => null,
}));
vi.mock("@/features/settings/CommonLoginProfilesSettings", () => ({
  CommonLoginProfilesSettings: () => null,
}));
vi.mock("@/features/settings/data-migration/DataMigrationSection", () => ({
  DataMigrationSection: () => null,
}));

function controllerDefaults(loading: boolean) {
  const noop = vi.fn();
  const arrays = new Set([
    "filteredStationAssetRows",
    "filteredStationIds",
    "snapshots",
    "stationKeys",
    "stations",
    "filteredItems",
    "groupOptionsForEdit",
    "stationOptions",
  ]);
  const maps = new Set([
    "collectorRunsByStation",
    "groupBindingsByStation",
    "rateRecordsByStation",
    "stationActions",
    "monitorByKey",
    "monitorStatusByKey",
  ]);
  return new Proxy({ loading }, {
    get(target, property: string | symbol) {
      if (property in target) return target[property as keyof typeof target];
      if (arrays.has(String(property))) return [];
      if (maps.has(String(property))) return new Map();
      if (/^(handle|open|close|set|refresh)/.test(String(property))) return noop;
      if (property === "issueFilter" || property === "filterMode") return "all";
      if (property === "selectedStationId") return "all";
      if (property === "query") return "";
      if (property === "keyDialogOpen" || property === "drawerVisible") return false;
      if (property === "saving" || property === "actionSaving" || property === "dragEnabled") return false;
      if (property === "attentionCount" || property === "collectedBalanceCount" || property === "filteredEnabledCount") return 0;
      return null;
    },
  });
}

vi.mock("@/features/stations/useStationsPageController", () => ({
  useStationsPageController: () => controllerDefaults(true),
}));
vi.mock("@/features/key-pool/useKeyPoolPageController", () => ({
  useKeyPoolPageController: () => controllerDefaults(true),
}));

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const pageAnchors: Readonly<Record<AppRouteId, readonly string[]>> = {
  dashboard: [
    "dashboard-metrics",
    "dashboard-station-metrics",
    "dashboard-risk",
    "dashboard-key-health",
    "dashboard-routing-queue",
    "dashboard-recent-usage",
  ],
  settings: [
    "settings-theme",
    "settings-local-proxy",
    "settings-network",
    "settings-pricing",
    "settings-data-backup",
    "settings-tutorial-entry",
  ],
  stations: ["stations-summary", "stations-toolbar", "stations-list", "stations-status-fields"],
  keyPool: ["key-pool-toolbar", "key-pool-list"],
  routing: ["routing-tabs", "routing-status"],
  collectors: ["collectors-summary"],
  channels: ["channels-tabs", "channels-local-toolbar", "channels-local-results"],
  pricing: ["pricing-summary", "pricing-filters", "pricing-comparison"],
  changes: [
    "changes-settings-entry",
    "changes-view-filter",
    "changes-severity-filter",
    "changes-unread-actions",
    "changes-list",
  ],
  logs: ["logs-display-controls", "logs-list"],
  runtimeDiagnostics: [],
};

function setStableLayout(element: HTMLElement) {
  Object.defineProperty(element, "getBoundingClientRect", {
    configurable: true,
    value: () => ({
      bottom: 40,
      height: 40,
      left: 0,
      right: 320,
      top: 0,
      width: 320,
      x: 0,
      y: 0,
      toJSON: () => ({}),
    }),
  });
}

describe("published tour anchors in real page components", () => {
  let root: Root | null = null;
  let host: HTMLDivElement | null = null;

  beforeEach(() => {
    document.body.replaceChildren();
    vi.clearAllMocks();
  });

  afterEach(() => {
    if (root) {
      act(() => root?.unmount());
      root = null;
    }
    host?.remove();
    host = null;
  });

  async function mountPage(route: AppRouteId, page: ReactNode) {
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
    await act(async () => {
      root?.render(
        <div
          data-page-transition-kind="shell"
          data-page-transition-layer
          data-page-transition-page-id={route}
          data-page-transition-state="active"
        >
          {page}
        </div>,
      );
    });
  }

  it("resolves every published page anchor from its active shell layer", async () => {
    const pages = await Promise.all([
      import("@/features/dashboard/DashboardPage").then(({ DashboardPage }) => ["dashboard", <DashboardPage />] as const),
      import("@/features/settings/SettingsPage").then(({ SettingsPage }) => [
        "settings",
        <SettingsPage onOpenModelBasePrices={vi.fn()} onOpenTourCenter={vi.fn()} />,
      ] as const),
      import("@/features/stations/StationsPage").then(({ StationsPage }) => ["stations", <StationsPage />] as const),
      import("@/features/key-pool/KeyPoolPage").then(({ KeyPoolPage }) => ["keyPool", <KeyPoolPage />] as const),
      import("@/features/routing/RoutingPage").then(({ RoutingPage }) => ["routing", <RoutingPage />] as const),
      import("@/features/pricing/PricingPage").then(({ PricingPage }) => ["pricing", <PricingPage />] as const),
      import("@/features/channels/ChannelStatusPage").then(({ ChannelStatusPage }) => ["channels", <ChannelStatusPage />] as const),
      import("@/features/changes/ChangeCenterPage").then(({ ChangeCenterPage }) => ["changes", <ChangeCenterPage onOpenSettings={vi.fn()} />] as const),
      import("@/features/logs/LogsPage").then(({ LogsPage }) => ["logs", <LogsPage />] as const),
      import("@/features/collectors/CollectorsPage").then(({ CollectorsPage }) => ["collectors", <CollectorsPage />] as const),
    ]);

    for (const [route, page] of pages) {
      await mountPage(route, page);
      for (const anchor of pageAnchors[route]) {
        const element = host?.querySelector<HTMLElement>(`[data-tour="${anchor}"]`);
        expect(element, `${route}/${anchor} should render`).not.toBeNull();
        setStableLayout(element!);
      }
      const resolver = new TourTargetResolver();
      for (const anchor of pageAnchors[route]) {
        expect(resolver.resolveTarget(anchor, route as AppPageId), `${route}/${anchor} should resolve`).toBe(
          host?.querySelector(`[data-tour="${anchor}"]`),
        );
      }
      const publishedAnchors = new Set(PUBLISHED_TOURS.flatMap((tour) =>
        tour.steps
          .filter((step) => step.route === route)
          .map((step) => step.target.anchor),
      ));
      if (publishedAnchors.size > 0) {
        for (const anchor of pageAnchors[route]) {
          expect(publishedAnchors.has(anchor), `${route}/${anchor} is referenced by the catalog`).toBe(true);
        }
      }
      act(() => root?.unmount());
      root = null;
      host?.replaceChildren();
    }
  });

  it("resolves global shell anchors independently of page layers", async () => {
    const { AppShell } = await import("@/components/shell/AppShell");
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
    await act(async () => {
      root?.render(
        <AppShell activeRouteId="dashboard" navigationSequence={1} onRouteChange={vi.fn()}>
          <div />
        </AppShell>,
      );
    });

    const resolver = new TourTargetResolver();
    for (const anchor of ["shell-sidebar", "nav-dashboard", "nav-settings"]) {
      const element = host?.querySelector<HTMLElement>(`[data-tour="${anchor}"]`);
      expect(element, `${anchor} should render in AppShell`).not.toBeNull();
      setStableLayout(element!);
      expect(resolver.resolveTarget(anchor, "dashboard")).toBe(element);
      expect(element?.closest("[data-page-transition-layer]")).toBeNull();
    }
  });

  it("prepares and restores the retained routing tab through a narrow view port", async () => {
    const { RoutingPage } = await import("@/features/routing/RoutingPage");
    let viewPort: RoutingViewPreparationPort | null = null;
    await mountPage(
      "routing",
      <RoutingPage onViewPreparationPort={(port) => { viewPort = port; }} />,
    );

    const buttons = Array.from(host?.querySelectorAll<HTMLButtonElement>('[role="radio"]') ?? []);
    const statusButton = buttons.find((button) => button.textContent?.includes("概览"));
    const editButton = buttons.find((button) => button.textContent?.includes("设置"));
    expect(statusButton).toBeDefined();
    expect(editButton).toBeDefined();

    await act(async () => editButton?.click());
    expect(editButton?.getAttribute("aria-checked")).toBe("true");

    let restore: () => void = () => undefined;
    await act(async () => { restore = viewPort?.showStatusView() ?? restore; });
    expect(statusButton?.getAttribute("aria-checked")).toBe("true");

    await act(async () => restore());
    expect(editButton?.getAttribute("aria-checked")).toBe("true");
  });
});
