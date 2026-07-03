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
import { StationsPage } from "@/features/stations/StationsPage";
import type { AppPageId } from "@/lib/types/navigation";
import type { AppRouteId } from "@/lib/types/navigation";

export function App() {
  const [activeRouteId, setActiveRouteId] = useState<AppPageId>("dashboard");
  const activeShellRouteId: AppRouteId =
    activeRouteId === "addProvider" ? "dashboard" : activeRouteId;

  const page = useMemo(() => {
    switch (activeRouteId) {
      case "addProvider":
        return (
          <AddProviderPage
            onBack={() => setActiveRouteId("stations")}
            onCreated={() => setActiveRouteId("stations")}
          />
        );
      case "stations":
        return <StationsPage onAddProvider={() => setActiveRouteId("addProvider")} />;
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
        return <DashboardPage onNavigate={setActiveRouteId} />;
    }
  }, [activeRouteId]);

  return (
    <AppShell activeRouteId={activeShellRouteId} onRouteChange={(routeId) => setActiveRouteId(routeId)}>
      {page}
    </AppShell>
  );
}
