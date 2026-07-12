import { memo } from "react";
import { ChannelStatusPage } from "@/features/channels/ChannelStatusPage";
import { ChangeCenterPage } from "@/features/changes/ChangeCenterPage";
import { CollectorsPage } from "@/features/collectors/CollectorsPage";
import { DashboardPage } from "@/features/dashboard/DashboardPage";
import { KeyPoolPage } from "@/features/key-pool/KeyPoolPage";
import { LogsPage } from "@/features/logs/LogsPage";
import { PricingPage } from "@/features/pricing/PricingPage";
import { RoutingPage } from "@/features/routing/RoutingPage";
import { SettingsPage } from "@/features/settings/SettingsPage";
import { StationsPage } from "@/features/stations/StationsPage";
import type { AppRouteId } from "@/lib/types/navigation";
import type { Station } from "@/lib/types/stations";

export type ShellPageActions = {
  addProvider: () => void;
  editProvider: (stationId: string) => void;
  openStation: (station: Station) => void;
  addKey: (stationId: string | null) => void;
  editKey: (stationKeyId: string) => void;
  openModelBasePrices: () => void;
};

export const ShellPageContent = memo(function ShellPageContent({
  routeId,
  actions,
}: {
  routeId: AppRouteId;
  actions: ShellPageActions;
}) {
  switch (routeId) {
    case "stations":
      return (
        <StationsPage
          onAddProvider={actions.addProvider}
          onEditProvider={actions.editProvider}
          onOpenStation={actions.openStation}
        />
      );
    case "keyPool":
      return <KeyPoolPage onAddKey={actions.addKey} onEditKey={actions.editKey} />;
    case "channels":
      return <ChannelStatusPage />;
    case "collectors":
      return <CollectorsPage />;
    case "changes":
      return <ChangeCenterPage />;
    case "pricing":
      return <PricingPage onOpenModelBasePrices={actions.openModelBasePrices} />;
    case "routing":
      return <RoutingPage />;
    case "logs":
      return <LogsPage />;
    case "settings":
      return <SettingsPage onOpenModelBasePrices={actions.openModelBasePrices} />;
    case "dashboard":
    default:
      return <DashboardPage />;
  }
});
