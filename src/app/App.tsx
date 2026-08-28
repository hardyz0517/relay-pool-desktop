import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { appRoutes } from "@/app/routes";
import { AppShell } from "@/components/shell/AppShell";
import type { TransientPageDescriptor } from "@/app/TransientPageHost";
import { ShellPageHost } from "@/app/ShellPageHost";
import type { ShellPageActions } from "@/app/shellPageRegistry";
import { useNavigationController } from "@/app/navigationController";
import { useIdlePagePrewarm } from "@/app/useIdlePagePrewarm";
import {
  getPageTransitionPolicy,
  isShellPage,
  resolveActiveShellRouteId,
} from "@/app/pageTransitionPolicy";
import { AddKeyPage, EditKeyPage } from "@/features/key-pool";
import { ModelBasePricesPage } from "@/features/pricing";
import {
  CHANGE_CENTER_DEFAULT_VIEW,
  ChangeCenterSettingsPage,
  type ChangeCenterView,
} from "@/features/changes";
import type { RequestLogDeepLink, VersionedRequestLogDeepLink } from "@/lib/types/requestLogDeepLinks";
import type { RoutingDeepLink, VersionedRoutingDeepLink } from "@/lib/types/routingDeepLinks";
import { AddProviderPage, StationDetailPage } from "@/features/stations";
import type { AppPageId, AppRouteId, TransientPageId } from "@/lib/types/navigation";
import type { Station } from "@/lib/types/stations";
import { settingsQueryOptions } from "@/lib/query/resourceQueries";
import { useActivityQuery } from "@/lib/query/useActivityQuery";
import { createTourNavigationPort } from "@/app/tours/tourNavigation";
import type { TourNavigationCurrent } from "@/app/tours/tourTypes";
import { createDriverJsAdapter } from "@/app/tours/TourDriverAdapter";
import { TourManager } from "@/app/tours/TourManager";
import { TourProvider } from "@/app/tours/TourProvider";
import { TourTargetResolver } from "@/app/tours/tourTargetResolver";
import { TourPreparationRegistry } from "@/app/tours/tourPreparationRegistry";
import { createTourProgressStore } from "@/app/tours/tourProgressStorage";
import { PUBLISHED_TOURS } from "@/app/tours/tourCatalog";
import { hasBlockingBusinessModal } from "@/app/tours/tourModalGuard";
import { scheduleTourAutoStart } from "@/app/tours/tourAutoStart";
import type { PublishedTourId } from "@/app/tours/tourTypes";
import { TourCenterDialog } from "@/features/settings/TourCenterDialog";
import type { RoutingViewPreparationPort } from "@/features/routing/routingViewPreparation";
import type { ChannelViewPreparationPort } from "@/features/channels/channelViewPreparation";

const ACTIONABLE_ELEMENT_SELECTOR = [
  "[data-page-autofocus]",
  "button:not([disabled])",
  "a[href]",
  'input:not([disabled]):not([type="hidden"])',
  "select:not([disabled])",
  "textarea:not([disabled])",
  '[tabindex]:not([tabindex^="-"])',
].join(", ");

export function App({ runtimeMode = "desktop" }: { runtimeMode?: "desktop" | "demo" } = {}) {
  const { intent, committed, pending, navigate } = useNavigationController("dashboard");
  const settingsQuery = useActivityQuery(settingsQueryOptions());
  const developerModeEnabled = settingsQuery.data?.developerModeEnabled === true;
  const { activeRouteId, previousRouteId, transientParentRouteId } = committed;
  const [mountedRouteIds, setMountedRouteIds] = useState<Set<AppRouteId>>(
    () => new Set(["dashboard"]),
  );
  const [editingStationId, setEditingStationId] = useState<string | null>(null);
  const [detailStationId, setDetailStationId] = useState<string | null>(null);
  const [detailStationPreview, setDetailStationPreview] = useState<Station | null>(null);
  const [initialKeyStationId, setInitialKeyStationId] = useState<string | null>(null);
  const [editingKeyId, setEditingKeyId] = useState<string | null>(null);
  const [routingDeepLink, setRoutingDeepLink] = useState<VersionedRoutingDeepLink | null>(null);
  const [requestLogDeepLink, setRequestLogDeepLink] = useState<VersionedRequestLogDeepLink | null>(null);
  const [changeCenterView, setChangeCenterView] = useState<ChangeCenterView>(CHANGE_CENTER_DEFAULT_VIEW);
  const routingDeepLinkSequenceRef = useRef(0);
  const requestLogDeepLinkSequenceRef = useRef(0);
  const lastShellFocusTargetRef = useRef<HTMLElement | null>(null);
  const transientReturnFocusRef = useRef<HTMLElement | null>(null);
  const activeRouteIdRef = useRef<AppPageId>(activeRouteId);
  const developerModeRef = useRef(developerModeEnabled);
  const autoStartedRef = useRef(false);
  const navigationDisposeTimerRef = useRef<number | null>(null);
  const initialDashboardReadyRef = useRef(false);
  const pendingTourStartRef = useRef<PublishedTourId | null>(null);
  const tourCenterOpenerRef = useRef<HTMLElement | null>(null);
  const routingViewPreparationRef = useRef<RoutingViewPreparationPort | null>(null);
  const channelViewPreparationRef = useRef<ChannelViewPreparationPort | null>(null);
  const [tourCenterOpen, setTourCenterOpen] = useState(false);
  const [, refreshTourProgress] = useState(0);
  const navigationCurrentRef = useRef<TourNavigationCurrent>({
    routeId: activeRouteId,
    shellRouteId: resolveActiveShellRouteId(activeRouteId, transientParentRouteId),
    sequence: committed.sequence,
    pending,
  });
  const activeShellRouteId = resolveActiveShellRouteId(
    activeRouteId,
    transientParentRouteId,
  );
  const activeShellRouteLabel =
    appRoutes.find((route) => route.id === activeShellRouteId)?.label ?? activeShellRouteId;
  const availableTours = useMemo(
    () => PUBLISHED_TOURS.filter((tour) => tour.requires !== "developer-mode" || developerModeEnabled),
    [developerModeEnabled],
  );
  const idlePrewarmCandidates = useMemo(
    () =>
      appRoutes
        .map((route) => ({
          routeId: route.id,
          prewarmPriority: getPageTransitionPolicy(route.id).prewarmPriority,
        }))
        .filter((candidate): candidate is { routeId: AppRouteId; prewarmPriority: number } =>
          candidate.prewarmPriority !== null,
        )
        .sort((left, right) => left.prewarmPriority - right.prewarmPriority)
        .map((candidate) => candidate.routeId),
    [],
  );

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

  useEffect(() => {
    activeRouteIdRef.current = activeRouteId;
  }, [activeRouteId]);

  useEffect(() => {
    developerModeRef.current = developerModeEnabled;
  }, [developerModeEnabled]);

  const navigateTo = useCallback((routeId: AppPageId) => {
    if (isShellPage(activeRouteIdRef.current) && !isShellPage(routeId)) {
      const recordedTarget = lastShellFocusTargetRef.current;
      const activeElement = document.activeElement;
      transientReturnFocusRef.current = recordedTarget?.isConnected
        ? recordedTarget
        : activeElement instanceof HTMLElement
          ? activeElement
          : null;
    }

    navigate(routeId);
  }, [navigate]);

  navigationCurrentRef.current = {
    routeId: activeRouteId,
    shellRouteId: activeShellRouteId,
    sequence: committed.sequence,
    pending,
  };
  const tourNavigation = useMemo(
    () =>
      createTourNavigationPort({
        navigate: navigateTo,
        getCurrent: () => navigationCurrentRef.current,
      }),
    [navigateTo],
  );
  const handlePageReady = useCallback(
    (routeId: AppRouteId, sequence: number) => {
      tourNavigation.notifyReady({
        routeId,
        shellRouteId: routeId,
        sequence,
      });
    },
    [tourNavigation],
  );

  useEffect(() => {
    if (navigationDisposeTimerRef.current !== null) {
      window.clearTimeout(navigationDisposeTimerRef.current);
      navigationDisposeTimerRef.current = null;
    }
    const dispose = () => tourNavigation.dispose();
    window.addEventListener("pagehide", dispose);
    return () => {
      window.removeEventListener("pagehide", dispose);
      navigationDisposeTimerRef.current = window.setTimeout(() => {
        navigationDisposeTimerRef.current = null;
        tourNavigation.dispose();
      }, 0);
    };
  }, [tourNavigation]);

  useEffect(() => {
    if (initialDashboardReadyRef.current || activeRouteId !== "dashboard" || pending) return;
    let cancelled = false;
    const frameId = window.requestAnimationFrame(() => {
      if (cancelled) return;
      initialDashboardReadyRef.current = true;
      tourNavigation.notifyReady({ routeId: "dashboard", shellRouteId: "dashboard", sequence: committed.sequence });
    });
    return () => {
      cancelled = true;
      window.cancelAnimationFrame(frameId);
    };
  }, [activeRouteId, committed.sequence, pending, tourNavigation]);

  const tourBundle = useMemo(() => {
    const progress = createTourProgressStore();
    const targetResolver = new TourTargetResolver();
    const manager = new TourManager({
      catalog: PUBLISHED_TOURS,
      driver: createDriverJsAdapter("relay-pool-tour-popover"),
      navigation: tourNavigation,
      targetResolver,
      preparation: new TourPreparationRegistry({
        actions: new Map([
          ["routing-status-tab", () => {
            const port = routingViewPreparationRef.current;
            if (!port) throw new Error("Routing view preparation is unavailable");
            return port.showStatusView();
          }],
          ["routing-settings-tab", () => {
            const port = routingViewPreparationRef.current;
            if (!port) throw new Error("Routing view preparation is unavailable");
            return port.showSettingsView();
          }],
          ["channels-local-tab", () => {
            const port = channelViewPreparationRef.current;
            if (!port) throw new Error("Channel view preparation is unavailable");
            return port.showLocalView();
          }],
          ["channels-official-tab", () => {
            const port = channelViewPreparationRef.current;
            if (!port) throw new Error("Channel view preparation is unavailable");
            return port.showOfficialView();
          }],
          ["channels-monitoring-tab", () => {
            const port = channelViewPreparationRef.current;
            if (!port) throw new Error("Channel view preparation is unavailable");
            return port.showMonitoringView();
          }],
        ]),
      }),
      progress,
      isDeveloperMode: () => developerModeRef.current,
      hasBlockingModal: () => hasBlockingBusinessModal(),
    });
    return { manager, progress, targetResolver };
  }, [tourNavigation]);

  const openTourCenter = useCallback(() => {
    const activeElement = document.activeElement;
    tourCenterOpenerRef.current = activeElement instanceof HTMLElement ? activeElement : null;
    setTourCenterOpen(true);
  }, []);

  const startTourFromCenter = useCallback((tourId: PublishedTourId) => {
    pendingTourStartRef.current = tourId;
    setTourCenterOpen(false);
  }, []);

  const startPendingTourAfterCenterExit = useCallback(() => {
    const tourId = pendingTourStartRef.current;
    pendingTourStartRef.current = null;
    const opener = tourCenterOpenerRef.current;
    tourCenterOpenerRef.current = null;
    if (opener?.isConnected && !opener.closest("[inert]")) {
      try {
        opener.focus({ preventScroll: true });
      } catch {
        opener.focus();
      }
    }
    if (tourId) tourBundle.manager.start(tourId, "settings");
  }, [tourBundle.manager]);

  const resetTourProgress = useCallback(() => {
    tourBundle.manager.resetProgress();
    refreshTourProgress((current) => current + 1);
  }, [tourBundle.manager]);

  useEffect(() => tourBundle.manager.subscribe(() => {
    // The progress store is intentionally independent from React state. A
    // lightweight render tick keeps the settings dialog accurate after a
    // tour completes or is skipped without coupling the manager to React.
    refreshTourProgress((current) => current + 1);
  }), [tourBundle.manager]);

  useEffect(() => {
    if (runtimeMode !== "desktop" || autoStartedRef.current || activeRouteId !== "dashboard" || pending) return;
    const firstStep = PUBLISHED_TOURS.find((tour) => tour.id === "basic")?.steps[0];
    return scheduleTourAutoStart({
      canAttempt: () =>
        !autoStartedRef.current &&
        document.visibilityState !== "hidden" &&
        activeRouteIdRef.current === "dashboard" &&
        !navigationCurrentRef.current.pending &&
        !hasBlockingBusinessModal(),
      hasTarget: () => Boolean(
        firstStep && tourBundle.targetResolver.resolveTarget(firstStep.target.anchor, firstStep.route),
      ),
      start: () => tourBundle.manager.start("basic", "auto"),
      onAccepted: () => { autoStartedRef.current = true; },
    });
  }, [activeRouteId, pending, runtimeMode, tourBundle.manager, tourBundle.targetResolver]);

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

  const prewarmShellRoute = useCallback((routeId: AppRouteId) => {
    setMountedRouteIds((current) => {
      if (current.has(routeId)) {
        return current;
      }
      const next = new Set(current);
      next.add(routeId);
      return next;
    });
  }, []);

  useIdlePagePrewarm({
    candidates: idlePrewarmCandidates,
    mountedRouteIds,
    disabled: pending,
    onPrewarm: prewarmShellRoute,
  });

  const returnToStations = useCallback(() => {
    setEditingStationId(null);
    setDetailStationId(null);
    setDetailStationPreview(null);
    navigateTo("stations");
  }, [navigateTo]);

  const returnToKeyPool = useCallback(() => {
    setInitialKeyStationId(null);
    setEditingKeyId(null);
    navigateTo("keyPool");
  }, [navigateTo]);

  const openAddProvider = useCallback(() => {
    navigateTo("addProvider");
  }, [navigateTo]);

  const openEditProvider = useCallback((stationId: string) => {
    setEditingStationId(stationId);
    navigateTo("editProvider");
  }, [navigateTo]);

  const openStationDetail = useCallback((station: Station) => {
    setDetailStationId(station.id);
    setDetailStationPreview(station);
    navigateTo("stationDetail");
  }, [navigateTo]);

  const openAddKey = useCallback((stationId: string | null) => {
    setInitialKeyStationId(stationId);
    setEditingKeyId(null);
    navigateTo("addKey");
  }, [navigateTo]);

  const openEditKey = useCallback((stationKeyId: string) => {
    setEditingKeyId(stationKeyId);
    setInitialKeyStationId(null);
    navigateTo("editKey");
  }, [navigateTo]);

  const openModelBasePrices = useCallback(() => {
    navigateTo("modelBasePrices");
  }, [navigateTo]);

  const openKeyPool = useCallback(() => {
    navigateTo("keyPool");
  }, [navigateTo]);

  const openLocalRouting = useCallback(() => {
    navigateTo("routing");
  }, [navigateTo]);

  const openRequestLogs = useCallback(() => {
    navigateTo("logs");
  }, [navigateTo]);

  const openChangeCenterSettings = useCallback(() => {
    navigateTo("changeSettings");
  }, [navigateTo]);

  const openRoutingDeepLink = useCallback((link: RoutingDeepLink) => {
    routingDeepLinkSequenceRef.current += 1;
    setRoutingDeepLink({ ...link, sequence: routingDeepLinkSequenceRef.current });
    navigateTo("routing");
  }, [navigateTo]);

  const registerRoutingViewPreparation = useCallback((port: RoutingViewPreparationPort | null) => {
    routingViewPreparationRef.current = port;
  }, []);

  const registerChannelViewPreparation = useCallback((port: ChannelViewPreparationPort | null) => {
    channelViewPreparationRef.current = port;
  }, []);

  const openRequestLogDeepLink = useCallback((link: RequestLogDeepLink) => {
    requestLogDeepLinkSequenceRef.current += 1;
    setRequestLogDeepLink({ ...link, sequence: requestLogDeepLinkSequenceRef.current });
    navigateTo("logs");
  }, [navigateTo]);

  const shellPageActions = useMemo<ShellPageActions>(
    () => ({
      addProvider: openAddProvider,
      editProvider: openEditProvider,
      openStation: openStationDetail,
      addKey: openAddKey,
      editKey: openEditKey,
      openKeyPool,
      openLocalRouting,
      openRequestLogs,
      openModelBasePrices,
      openChangeCenterSettings,
      openTourCenter,
      changeCenterView,
      setChangeCenterView,
      openRoutingDeepLink,
      routingDeepLink,
      registerRoutingViewPreparation,
      registerChannelViewPreparation,
      openRequestLogDeepLink,
      requestLogDeepLink,
    }),
    [
      openAddProvider,
      openEditProvider,
      openStationDetail,
      openAddKey,
      openEditKey,
      openKeyPool,
      openLocalRouting,
      openRequestLogs,
      openModelBasePrices,
      openChangeCenterSettings,
      openTourCenter,
      changeCenterView,
      openRoutingDeepLink,
      routingDeepLink,
      registerRoutingViewPreparation,
      registerChannelViewPreparation,
      openRequestLogDeepLink,
      requestLogDeepLink,
    ],
  );

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
              onOpenRoutingDeepLink={developerModeEnabled ? openRoutingDeepLink : undefined}
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
              onOpenRoutingDeepLink={developerModeEnabled ? openRoutingDeepLink : undefined}
            />
          ),
        };
      case "changeSettings":
        return {
          pageId: "changeSettings",
          instanceKey: "changeSettings",
          node: (
            <ChangeCenterSettingsPage
              onBack={() => navigateTo("changes")}
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
  const previousShellRouteId =
    previousRouteId && isShellPage(previousRouteId) ? previousRouteId : null;

  return (
      <TourProvider manager={tourBundle.manager}>
        <AppShell
          activeRouteId={intent.shellRouteId}
          navigationSequence={intent.sequence}
          onRouteChange={navigateTo}
        >
          <ShellPageHost
            actions={shellPageActions}
            activeShellRouteId={activeShellRouteId}
            activeTransientPage={activeTransientPage}
            committedNavigationSequence={committed.sequence}
            intentNavigationSequence={intent.sequence}
            intentShellRouteId={intent.shellRouteId}
            mountedRouteIds={mountedRouteIds}
            onExitComplete={restoreTransientReturnFocus}
            onPageReady={handlePageReady}
            onRememberShellFocusTarget={rememberShellFocusTarget}
            pending={pending}
            previousShellRouteId={previousShellRouteId}
            transientActive={isCurrentTransientPage}
          />
        </AppShell>
        <TourCenterDialog
          open={tourCenterOpen}
          tours={availableTours}
          progress={tourBundle.progress.getSnapshot()}
          onClose={() => setTourCenterOpen(false)}
          onExited={startPendingTourAfterCenterExit}
          onStart={startTourFromCenter}
          onReset={resetTourProgress}
        />
      </TourProvider>
  );
}
