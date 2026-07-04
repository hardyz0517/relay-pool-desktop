import { useMemo, useState } from "react";
import { AppShell } from "@/components/shell/AppShell";
import { CollectorsPage } from "@/features/collectors/CollectorsPage";
import { DashboardPage } from "@/features/dashboard/DashboardPage";
import { LogsPage } from "@/features/logs/LogsPage";
import { PricingPage } from "@/features/pricing/PricingPage";
import { RoutingPage } from "@/features/routing/RoutingPage";
import { KeyPoolPage } from "@/features/key-pool/KeyPoolPage";
import { SettingsPage } from "@/features/settings/SettingsPage";
import { ChannelStatusPage } from "@/features/channels/ChannelStatusPage";
import { ChangeCenterPage } from "@/features/changes/ChangeCenterPage";
import { AddProviderPage } from "@/features/stations/AddProviderPage";
import { StationDetailPage } from "@/features/stations/StationDetailPage";
import { StationsPage } from "@/features/stations/StationsPage";
import type { AppPageId } from "@/lib/types/navigation";
import type { AppRouteId } from "@/lib/types/navigation";

export function App() {
  const [activeRouteId, setActiveRouteId] = useState<AppPageId>("dashboard");
  const [editingStationId, setEditingStationId] = useState<string | null>(null);
  const [detailStationId, setDetailStationId] = useState<string | null>(null);
  const activeShellRouteId: AppRouteId =
    activeRouteId === "addProvider" || activeRouteId === "editProvider" || activeRouteId === "stationDetail"
      ? "stations"
      : activeRouteId;

  function returnToStations() {
    setEditingStationId(null);
    setDetailStationId(null);
    setActiveRouteId("stations");
  }

  function openEditProvider(stationId: string) {
    setEditingStationId(stationId);
    setActiveRouteId("editProvider");
  }

  function openStationDetail(stationId: string) {
    setDetailStationId(stationId);
    setActiveRouteId("stationDetail");
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
            onBack={returnToStations}
            onEditProvider={openEditProvider}
          />
        );
      case "stations":
        return (
          <StationsPage
            onAddProvider={() => setActiveRouteId("addProvider")}
            onEditProvider={openEditProvider}
          />
        );
      case "keyPool":
        return <KeyPoolPage />;
      case "channels":
        return <ChannelStatusPage />;
      case "collectors":
        return <CollectorsPage />;
      case "changes":
        return <ChangeCenterPage />;
      case "pricing":
        return <PricingPage />;
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
  }, [activeRouteId, editingStationId, detailStationId]);

  return (
    <AppShell activeRouteId={activeShellRouteId} onRouteChange={(routeId) => setActiveRouteId(routeId)}>
      {page}
    </AppShell>
  );
}
