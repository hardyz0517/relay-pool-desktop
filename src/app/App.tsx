import { useMemo, useState } from "react";
import { AppShell } from "@/components/shell/AppShell";
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

export function App() {
  const [activeRouteId, setActiveRouteId] = useState<AppPageId>("dashboard");
  const [editingStationId, setEditingStationId] = useState<string | null>(null);
  const [detailStationId, setDetailStationId] = useState<string | null>(null);
  const [detailStationPreview, setDetailStationPreview] = useState<Station | null>(null);
  const [initialKeyStationId, setInitialKeyStationId] = useState<string | null>(null);
  const [editingKeyId, setEditingKeyId] = useState<string | null>(null);
  const activeShellRouteId = getShellRouteId(activeRouteId);

  function returnToStations() {
    setEditingStationId(null);
    setDetailStationId(null);
    setDetailStationPreview(null);
    setActiveRouteId("stations");
  }

  function returnToKeyPool() {
    setInitialKeyStationId(null);
    setEditingKeyId(null);
    setActiveRouteId("keyPool");
  }

  function openEditProvider(stationId: string) {
    setEditingStationId(stationId);
    setActiveRouteId("editProvider");
  }

  function openStationDetail(station: Station) {
    setDetailStationId(station.id);
    setDetailStationPreview(station);
    setActiveRouteId("stationDetail");
  }

  function openAddKey(stationId: string | null) {
    setInitialKeyStationId(stationId);
    setEditingKeyId(null);
    setActiveRouteId("addKey");
  }

  function openEditKey(stationKeyId: string) {
    setEditingKeyId(stationKeyId);
    setInitialKeyStationId(null);
    setActiveRouteId("editKey");
  }

  const page = useMemo(() => {
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
            stationId={editingStationId}
            onBack={returnToStations}
            onUpdated={returnToStations}
          />
        );
      case "stationDetail":
        return (
          <StationDetailPage
            stationId={detailStationId}
            initialStation={detailStationPreview}
            onBack={returnToStations}
            onEditProvider={openEditProvider}
          />
        );
      case "addKey":
        return (
          <AddKeyPage
            initialStationId={initialKeyStationId}
            onBack={returnToKeyPool}
            onCreated={returnToKeyPool}
          />
        );
      case "editKey":
        return (
          <EditKeyPage
            stationKeyId={editingKeyId}
            onBack={returnToKeyPool}
            onUpdated={returnToKeyPool}
          />
        );
      case "stations":
        return (
          <StationsPage
            onAddProvider={() => setActiveRouteId("addProvider")}
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
        return <PricingPage onOpenModelBasePrices={() => setActiveRouteId("modelBasePrices")} />;
      case "modelBasePrices":
        return <ModelBasePricesPage onBack={() => setActiveRouteId("pricing")} />;
      case "routing":
        return <RoutingPage onOpenPage={(routeId) => setActiveRouteId(routeId)} />;
      case "logs":
        return <LogsPage />;
      case "settings":
        return <SettingsPage onOpenModelBasePrices={() => setActiveRouteId("modelBasePrices")} />;
      case "dashboard":
      default:
        return <DashboardPage />;
    }
  }, [activeRouteId, editingStationId, detailStationId, detailStationPreview, initialKeyStationId, editingKeyId]);

  return (
    <AppShell activeRouteId={activeShellRouteId} onRouteChange={(routeId) => setActiveRouteId(routeId)}>
      {page}
    </AppShell>
  );
}

function getShellRouteId(pageId: AppPageId): AppRouteId {
  if (pageId === "addProvider" || pageId === "editProvider" || pageId === "stationDetail") {
    return "stations";
  }
  if (pageId === "addKey" || pageId === "editKey") {
    return "keyPool";
  }
  if (pageId === "modelBasePrices") {
    return "pricing";
  }
  return pageId;
}
