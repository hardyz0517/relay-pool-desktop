import { useCallback, useEffect, useRef, useState } from "react";
import { appRoutes } from "@/app/routes";
import { AppShell } from "@/components/shell/AppShell";
import { PageActivityProvider } from "@/components/shell/PageActivity";
import {
  TransientPageHost,
  type TransientPageDescriptor,
} from "@/app/TransientPageHost";
import {
  getPageTransitionPolicy,
  isShellPage,
  resolveActiveShellRouteId,
  resolveTransientParentRouteId,
} from "@/app/pageTransitionPolicy";
import { CollectorsPage } from "@/features/collectors/CollectorsPage";
import { DashboardPage } from "@/features/dashboard/DashboardPage";
import { LogsPage } from "@/features/logs/LogsPage";
import { PricingPage } from "@/features/pricing/PricingPage";
import { ModelBasePricesPage } from "@/features/pricing/ModelBasePricesPage";
import { RoutingPage } from "@/features/routing/RoutingPage";
import { KeyPoolPage } from "@/features/key-pool/KeyPoolPage";
import { SettingsPage } from "@/features/settings/SettingsPage";
import { ChannelStatusPage } from "@/features/channels/ChannelStatusPage";
import { ChangeCenterPage } from "@/features/changes/ChangeCenterPage";
import { AddKeyPage } from "@/features/key-pool/AddKeyPage";
import { EditKeyPage } from "@/features/key-pool/EditKeyPage";
import { AddProviderPage } from "@/features/stations/AddProviderPage";
import { StationDetailPage } from "@/features/stations/StationDetailPage";
import { StationsPage } from "@/features/stations/StationsPage";
import type { AppPageId, AppRouteId, TransientPageId } from "@/lib/types/navigation";
import type { Station } from "@/lib/types/stations";

type NavigationState = {
  activeRouteId: AppPageId;
  previousRouteId: AppPageId | null;
  transientParentRouteId: AppRouteId | null;
};

type ShellPageState = "active" | "background" | "inactive";

const ACTIONABLE_ELEMENT_SELECTOR = [
  "[data-page-autofocus]",
  "button:not([disabled])",
  "a[href]",
  'input:not([disabled]):not([type="hidden"])',
  "select:not([disabled])",
  "textarea:not([disabled])",
  '[tabindex]:not([tabindex^="-"])',
].join(", ");

export function App() {
  const [{ activeRouteId, previousRouteId, transientParentRouteId }, setNavigation] =
    useState<NavigationState>({
      activeRouteId: "dashboard",
      previousRouteId: null,
      transientParentRouteId: null,
    });
  const [mountedRouteIds, setMountedRouteIds] = useState<Set<AppRouteId>>(
    () => new Set(["dashboard"]),
  );
  const [editingStationId, setEditingStationId] = useState<string | null>(null);
  const [detailStationId, setDetailStationId] = useState<string | null>(null);
  const [detailStationPreview, setDetailStationPreview] = useState<Station | null>(null);
  const [initialKeyStationId, setInitialKeyStationId] = useState<string | null>(null);
  const [editingKeyId, setEditingKeyId] = useState<string | null>(null);
  const lastShellFocusTargetRef = useRef<HTMLElement | null>(null);
  const transientReturnFocusRef = useRef<HTMLElement | null>(null);
  const activeShellRouteId = resolveActiveShellRouteId(
    activeRouteId,
    transientParentRouteId,
  );
  const activeShellRouteLabel =
    appRoutes.find((route) => route.id === activeShellRouteId)?.label ?? activeShellRouteId;

  const rememberShellFocusTarget = useCallback((target: EventTarget | null) => {
    if (!(target instanceof Element)) {
      return;
    }

    const candidate = target.closest<HTMLElement>(ACTIONABLE_ELEMENT_SELECTOR);
    if (
      !candidate?.closest(
        '[data-page-transition-kind="shell"][data-page-transition-state="active"]',
      )
    ) {
      return;
    }

    lastShellFocusTargetRef.current = candidate;
  }, []);

  const navigateTo = useCallback(
    (routeId: AppPageId) => {
      if (isShellPage(activeRouteId) && !isShellPage(routeId)) {
        const recordedTarget = lastShellFocusTargetRef.current;
        const activeElement = document.activeElement;
        transientReturnFocusRef.current = recordedTarget?.isConnected
          ? recordedTarget
          : activeElement instanceof HTMLElement
            ? activeElement
            : null;
      }

      setNavigation((current) => {
        if (current.activeRouteId === routeId) {
          return current;
        }
        return {
          activeRouteId: routeId,
          previousRouteId: current.activeRouteId,
          transientParentRouteId: resolveTransientParentRouteId(
            current.activeRouteId,
            routeId,
            current.transientParentRouteId,
          ),
        };
      });
    },
    [activeRouteId],
  );

  const restoreTransientReturnFocus = useCallback(() => {
    const target = transientReturnFocusRef.current;
    transientReturnFocusRef.current = null;

    if (!target?.isConnected || target.closest("[inert]")) {
      return;
    }

    target.focus({ preventScroll: true });
  }, []);

  useEffect(() => {
    if (!isShellPage(activeRouteId)) {
      return;
    }
    setMountedRouteIds((current) => {
      if (current.has(activeRouteId)) {
        return current;
      }
      const next = new Set(current);
      next.add(activeRouteId);
      return next;
    });
  }, [activeRouteId]);

  function returnToStations() {
    setEditingStationId(null);
    setDetailStationId(null);
    setDetailStationPreview(null);
    navigateTo("stations");
  }

  function returnToKeyPool() {
    setInitialKeyStationId(null);
    setEditingKeyId(null);
    navigateTo("keyPool");
  }

  function openEditProvider(stationId: string) {
    setEditingStationId(stationId);
    navigateTo("editProvider");
  }

  function openStationDetail(station: Station) {
    setDetailStationId(station.id);
    setDetailStationPreview(station);
    navigateTo("stationDetail");
  }

  function openAddKey(stationId: string | null) {
    setInitialKeyStationId(stationId);
    setEditingKeyId(null);
    navigateTo("addKey");
  }

  function openEditKey(stationKeyId: string) {
    setEditingKeyId(stationKeyId);
    setInitialKeyStationId(null);
    navigateTo("editKey");
  }

  function renderShellPage(routeId: AppRouteId) {
    switch (routeId) {
      case "stations":
        return (
          <StationsPage
            onAddProvider={() => navigateTo("addProvider")}
            onEditProvider={openEditProvider}
            onOpenStation={openStationDetail}
          />
        );
      case "keyPool":
        return <KeyPoolPage onAddKey={openAddKey} onEditKey={openEditKey} />;
      case "channels":
        return <ChannelStatusPage />;
      case "collectors":
        return <CollectorsPage />;
      case "changes":
        return <ChangeCenterPage />;
      case "pricing":
        return <PricingPage onOpenModelBasePrices={() => navigateTo("modelBasePrices")} />;
      case "routing":
        return <RoutingPage />;
      case "logs":
        return <LogsPage />;
      case "settings":
        return <SettingsPage />;
      case "dashboard":
      default:
        return <DashboardPage />;
    }
  }

  function renderTransientPage(pageId: TransientPageId): TransientPageDescriptor {
    switch (pageId) {
      case "addProvider":
        return {
          pageId: "addProvider",
          instanceKey: "addProvider",
          node: (
            <AddProviderPage onBack={returnToStations} onCreated={returnToStations} />
          ),
        };
      case "editProvider":
        return {
          pageId: "editProvider",
          instanceKey: `editProvider:${editingStationId ?? "edit-provider-empty"}`,
          node: (
            <AddProviderPage
              stationId={editingStationId}
              onBack={returnToStations}
              onUpdated={returnToStations}
            />
          ),
        };
      case "stationDetail":
        return {
          pageId: "stationDetail",
          instanceKey: `stationDetail:${detailStationId ?? "station-detail-empty"}`,
          node: (
            <StationDetailPage
              stationId={detailStationId}
              initialStation={detailStationPreview}
              onBack={returnToStations}
              onEditProvider={openEditProvider}
            />
          ),
        };
      case "addKey":
        return {
          pageId: "addKey",
          instanceKey: `addKey:${initialKeyStationId ?? "add-key-unscoped"}`,
          node: (
            <AddKeyPage
              initialStationId={initialKeyStationId}
              onBack={returnToKeyPool}
              onCreated={returnToKeyPool}
            />
          ),
        };
      case "editKey":
        return {
          pageId: "editKey",
          instanceKey: `editKey:${editingKeyId ?? "edit-key-empty"}`,
          node: (
            <EditKeyPage
              stationKeyId={editingKeyId}
              onBack={returnToKeyPool}
              onUpdated={returnToKeyPool}
            />
          ),
        };
      case "modelBasePrices":
        return {
          pageId: "modelBasePrices",
          instanceKey: "modelBasePrices",
          node: (
            <ModelBasePricesPage
              backLabel={`返回${activeShellRouteLabel}`}
              onBack={() => navigateTo(activeShellRouteId)}
            />
          ),
        };
      default: {
        const exhaustivePageId: never = pageId;
        return exhaustivePageId;
      }
    }
  }

  const activeTransitionPolicy = getPageTransitionPolicy(activeRouteId);
  const activeTransientPage = isShellPage(activeRouteId)
    ? null
    : renderTransientPage(activeRouteId);
  const isCurrentTransientPage = activeTransitionPolicy.kind === "transient";
  const previousTransitionPolicy = previousRouteId
    ? getPageTransitionPolicy(previousRouteId)
    : null;
  const isReturningFromTransient =
    activeTransitionPolicy.kind === "shell" && previousTransitionPolicy?.kind === "transient";
  const shellRouteIds = mountedRouteIds.has(activeShellRouteId)
    ? [...mountedRouteIds]
    : [...mountedRouteIds, activeShellRouteId];

  return (
    <AppShell activeRouteId={activeShellRouteId} onRouteChange={navigateTo}>
      <div
        className="app-page-transition-stack"
        data-page-transition-handoff={isReturningFromTransient ? "transient-exit" : "none"}
        onPointerDownCapture={(event) => rememberShellFocusTarget(event.target)}
        onFocusCapture={(event) => rememberShellFocusTarget(event.target)}
      >
        {shellRouteIds.map((routeId) => {
          const shellPageState: ShellPageState =
            routeId !== activeShellRouteId
              ? "inactive"
              : isCurrentTransientPage ? "background" : "active";
          const active = shellPageState === "active";
          const inert = shellPageState !== "active";

          return (
            <PageActivityProvider key={routeId} active={active}>
              <div
                aria-hidden={inert}
                className="app-page-transition-layer"
                data-page-transition-layer
                data-page-transition-kind="shell"
                data-page-transition-state={shellPageState}
                inert={inert ? "" : undefined}
              >
                <div className="app-page-transition-content">
                  {renderShellPage(routeId)}
                </div>
              </div>
            </PageActivityProvider>
          );
        })}

        <TransientPageHost
          page={activeTransientPage}
          onExitComplete={restoreTransientReturnFocus}
        />
      </div>
    </AppShell>
  );
}
