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
import { ModelBasePricesPage } from "@/features/pricing/ModelBasePricesPage";
import { AddKeyPage } from "@/features/key-pool/AddKeyPage";
import { EditKeyPage } from "@/features/key-pool/EditKeyPage";
import { AddProviderPage } from "@/features/stations/AddProviderPage";
import { StationDetailPage } from "@/features/stations/StationDetailPage";
import type { AppPageId, AppRouteId, TransientPageId } from "@/lib/types/navigation";
import type { Station } from "@/lib/types/stations";

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
  const { intent, committed, pending, navigate } = useNavigationController("dashboard");
  const { activeRouteId, previousRouteId, transientParentRouteId } = committed;
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
  const activeRouteIdRef = useRef<AppPageId>(activeRouteId);
  const activeShellRouteId = resolveActiveShellRouteId(
    activeRouteId,
    transientParentRouteId,
  );
  const activeShellRouteLabel =
    appRoutes.find((route) => route.id === activeShellRouteId)?.label ?? activeShellRouteId;
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

  const shellPageActions = useMemo<ShellPageActions>(
    () => ({
      addProvider: openAddProvider,
      editProvider: openEditProvider,
      openStation: openStationDetail,
      addKey: openAddKey,
      editKey: openEditKey,
      openModelBasePrices,
    }),
    [openAddProvider, openEditProvider, openStationDetail, openAddKey, openEditKey, openModelBasePrices],
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
  const previousShellRouteId =
    previousRouteId && isShellPage(previousRouteId) ? previousRouteId : null;

  return (
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
        onRememberShellFocusTarget={rememberShellFocusTarget}
        pending={pending}
        previousShellRouteId={previousShellRouteId}
        transientActive={isCurrentTransientPage}
      />
    </AppShell>
  );
}
