import { memo } from "react";
import { ChannelStatusPage } from "@/features/channels";
import { ChangeCenterPage } from "@/features/changes";
import { CollectorsPage } from "@/features/collectors";
import { DashboardPage } from "@/features/dashboard";
import { KeyPoolPage } from "@/features/key-pool/KeyPoolPage";
import { LogsPage } from "@/features/logs";
import { PricingPage } from "@/features/pricing/PricingPage";
import { RoutingPage } from "@/features/routing";
import { SettingsPage } from "@/features/settings";
import { StationsPage } from "@/features/stations";
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
      return <SettingsPage />;
    case "dashboard":
    default:
      return <DashboardPage />;
  }
});
