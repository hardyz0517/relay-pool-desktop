import { useMemo, useState } from "react";
import { AppShell } from "@/components/shell/AppShell";
import { CollectorsPage } from "@/features/collectors/CollectorsPage";
import { DashboardPage } from "@/features/dashboard/DashboardPage";
import { LogsPage } from "@/features/logs/LogsPage";
import { PricingPage } from "@/features/pricing/PricingPage";
import { RoutingPage } from "@/features/routing/RoutingPage";
import { SettingsPage } from "@/features/settings/SettingsPage";
import { StationsPage } from "@/features/stations/StationsPage";
import type { AppRouteId } from "@/lib/types/navigation";

export function App() {
  const [activeRouteId, setActiveRouteId] = useState<AppRouteId>("dashboard");

  const page = useMemo(() => {
    switch (activeRouteId) {
      case "stations":
        return <StationsPage />;
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
        return <DashboardPage />;
    }
  }, [activeRouteId]);

  return (
    <AppShell activeRouteId={activeRouteId} onRouteChange={setActiveRouteId}>
      {page}
    </AppShell>
  );
}
