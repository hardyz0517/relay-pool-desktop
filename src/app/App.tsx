import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type AnimationEvent,
  type ReactNode,
} from "react";
import { AppShell } from "@/components/shell/AppShell";
import { PageActivityProvider } from "@/components/shell/PageActivity";
import {
  getPageTransitionPolicy,
  getShellRouteId,
  isShellPage,
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
import type { AppPageId } from "@/lib/types/navigation";
import type { AppRouteId } from "@/lib/types/navigation";
import type { Station } from "@/lib/types/stations";

declare module "react" {
  interface HTMLAttributes<T> {
    inert?: "" | undefined;
  }
}

const TRANSIENT_EXIT_TIMEOUT_MS = 200;

type RenderedTransientPage = {
  pageId: AppPageId;
  node: ReactNode;
};

type NavigationState = {
  activeRouteId: AppPageId;
  previousRouteId: AppPageId | null;
};

export function App() {
  const [{ activeRouteId, previousRouteId }, setNavigation] = useState<NavigationState>({
    activeRouteId: "dashboard",
    previousRouteId: null,
  });
  const [mountedRouteIds, setMountedRouteIds] = useState<Set<AppRouteId>>(
    () => new Set(["dashboard"]),
  );
  const [editingStationId, setEditingStationId] = useState<string | null>(null);
  const [detailStationId, setDetailStationId] = useState<string | null>(null);
  const [detailStationPreview, setDetailStationPreview] = useState<Station | null>(null);
  const [initialKeyStationId, setInitialKeyStationId] = useState<string | null>(null);
  const [editingKeyId, setEditingKeyId] = useState<string | null>(null);
  const lastActiveTransientPageRef = useRef<RenderedTransientPage | null>(null);
  const transientExitTimeoutRef = useRef<number | null>(null);
  const [exitingTransientPage, setExitingTransientPage] =
    useState<RenderedTransientPage | null>(null);
  const activeShellRouteId = getShellRouteId(activeRouteId);

  const navigateTo = useCallback((routeId: AppPageId) => {
    setNavigation((current) => {
      if (current.activeRouteId === routeId) {
        return current;
      }
      return {
        activeRouteId: routeId,
        previousRouteId: current.activeRouteId,
      };
    });
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
        return <SettingsPage onOpenModelBasePrices={() => navigateTo("modelBasePrices")} />;
      case "dashboard":
      default:
        return <DashboardPage />;
    }
  }

  function renderTransientPage(): ReactNode {
    switch (activeRouteId) {
      case "addProvider":
        return (
          <AddProviderPage
            onBack={returnToStations}
            onCreated={returnToStations}
          />
        );
      case "editProvider":
        return (
          <AddProviderPage
            key={editingStationId ?? "edit-provider-empty"}
            stationId={editingStationId}
            onBack={returnToStations}
            onUpdated={returnToStations}
          />
        );
      case "stationDetail":
        return (
          <StationDetailPage
            key={detailStationId ?? "station-detail-empty"}
            stationId={detailStationId}
            initialStation={detailStationPreview}
            onBack={returnToStations}
            onEditProvider={openEditProvider}
          />
        );
      case "addKey":
        return (
          <AddKeyPage
            key={initialKeyStationId ?? "add-key-unscoped"}
            initialStationId={initialKeyStationId}
            onBack={returnToKeyPool}
            onCreated={returnToKeyPool}
          />
        );
      case "editKey":
        return (
          <EditKeyPage
            key={editingKeyId ?? "edit-key-empty"}
            stationKeyId={editingKeyId}
            onBack={returnToKeyPool}
            onUpdated={returnToKeyPool}
          />
        );
      case "modelBasePrices":
        return <ModelBasePricesPage onBack={() => navigateTo("pricing")} />;
      default:
        return null;
    }
  }

  const activeTransitionPolicy = getPageTransitionPolicy(activeRouteId);
  const activeTransientPage = useMemo<RenderedTransientPage | null>(() => {
    if (activeTransitionPolicy.kind !== "transient") {
      return null;
    }
    return {
      pageId: activeRouteId,
      node: renderTransientPage(),
    };
  }, [
    activeRouteId,
    activeTransitionPolicy.kind,
    detailStationId,
    detailStationPreview,
    editingKeyId,
    editingStationId,
    initialKeyStationId,
  ]);
  const isCurrentTransientPage = activeTransitionPolicy.kind === "transient";
  const previousTransitionPolicy = previousRouteId
    ? getPageTransitionPolicy(previousRouteId)
    : null;
  const isReturningFromTransient =
    activeTransitionPolicy.kind === "shell" && previousTransitionPolicy?.kind === "transient";
  const pendingExitingTransientPage =
    isReturningFromTransient && lastActiveTransientPageRef.current?.pageId === previousRouteId
      ? lastActiveTransientPageRef.current
      : null;
  const renderedTransientPage =
    activeTransientPage ?? pendingExitingTransientPage ?? exitingTransientPage;
  const transientPageIsExiting = !activeTransientPage && renderedTransientPage !== null;

  useLayoutEffect(() => {
    if (pendingExitingTransientPage) {
      setExitingTransientPage((current) =>
        current === pendingExitingTransientPage ? current : pendingExitingTransientPage,
      );
      return;
    }

    if (activeTransientPage) {
      setExitingTransientPage(null);
      lastActiveTransientPageRef.current = activeTransientPage;
    }
  }, [activeTransientPage, pendingExitingTransientPage]);

  useEffect(() => {
    if (!exitingTransientPage) {
      return;
    }

    if (transientExitTimeoutRef.current !== null) {
      window.clearTimeout(transientExitTimeoutRef.current);
    }

    transientExitTimeoutRef.current = window.setTimeout(handleTransientExitComplete,
      TRANSIENT_EXIT_TIMEOUT_MS,
    );

    return () => {
      if (transientExitTimeoutRef.current !== null) {
        window.clearTimeout(transientExitTimeoutRef.current);
        transientExitTimeoutRef.current = null;
      }
    };
  }, [exitingTransientPage]);

  function handleTransientExitComplete() {
    if (transientExitTimeoutRef.current !== null) {
      window.clearTimeout(transientExitTimeoutRef.current);
      transientExitTimeoutRef.current = null;
    }
    if (
      exitingTransientPage &&
      lastActiveTransientPageRef.current?.pageId === exitingTransientPage.pageId
    ) {
      lastActiveTransientPageRef.current = null;
    }
    setExitingTransientPage(null);
  }

  function handleTransientExitAnimationEnd(event: AnimationEvent<HTMLDivElement>) {
    if (event.target !== event.currentTarget) {
      return;
    }
    handleTransientExitComplete();
  }

  const shellRouteIds = isShellPage(activeRouteId) && !mountedRouteIds.has(activeRouteId)
    ? [...mountedRouteIds, activeRouteId]
    : [...mountedRouteIds];

  return (
    <AppShell activeRouteId={activeShellRouteId} onRouteChange={navigateTo}>
      <div
        className="app-page-transition-stack"
        data-page-transition-handoff={isReturningFromTransient ? "transient-exit" : "none"}
      >
        {shellRouteIds.map((routeId) => {
          const active = activeRouteId === routeId && !isCurrentTransientPage;
          const inert = !active;

          return (
            <PageActivityProvider key={routeId} active={active}>
              <div
                aria-hidden={inert}
                className="app-page-transition-layer"
                data-page-transition-layer
                data-page-transition-kind="shell"
                data-page-transition-direction="none"
                data-page-transition-state={active ? "active" : "inactive"}
                inert={inert ? "" : undefined}
              >
                {renderShellPage(routeId)}
              </div>
            </PageActivityProvider>
          );
        })}

        {renderedTransientPage && (
          <PageActivityProvider
            key={renderedTransientPage.pageId}
            active={!transientPageIsExiting}
          >
            <div
              aria-hidden={transientPageIsExiting}
              className="app-page-transition-layer app-page-transition-overlay"
              data-page-transition-layer
              data-page-transition-kind="transient"
              data-page-transition-direction={
                transientPageIsExiting
                  ? getPageTransitionPolicy(renderedTransientPage.pageId).exitDirection
                  : activeTransitionPolicy.enterDirection
              }
              data-page-transition-state={transientPageIsExiting ? "exiting" : "active"}
              inert={transientPageIsExiting ? "" : undefined}
              onAnimationEnd={
                transientPageIsExiting ? handleTransientExitAnimationEnd : undefined
              }
            >
              {renderedTransientPage.node}
            </div>
          </PageActivityProvider>
        )}
      </div>
    </AppShell>
  );
}
