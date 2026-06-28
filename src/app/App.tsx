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
import { StationsPage } from "@/features/stations/StationsPage";
import type { AppPageId, AppRouteId } from "@/lib/types/navigation";

export function App() {
  const [activeRouteId, setActiveRouteId] = useState<AppPageId>("dashboard");
  const activeShellRouteId: AppRouteId = activeRouteId === "addProvider" ? "dashboard" : activeRouteId;

  const page = useMemo(() => {
    switch (activeRouteId) {
      case "stations":
        return <StationsPage />;
      case "keyPool":
        return <KeyPoolPage />;
      case "channels":
        return <ChannelStatusPage />;
      case "collectors":
        return <CollectorsPage />;
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
    <AppShell activeRouteId={activeShellRouteId} onRouteChange={setActiveRouteId}>
      {page}
    </AppShell>
  );
}
