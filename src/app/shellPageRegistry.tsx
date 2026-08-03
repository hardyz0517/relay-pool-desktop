import { memo } from "react";
import { ChannelStatusPage } from "@/features/channels";
import { ChangeCenterPage } from "@/features/changes";
import { CollectorsPage } from "@/features/collectors";
import { DashboardPage } from "@/features/dashboard";
import { KeyPoolPage } from "@/features/key-pool";
import { LogsPage } from "@/features/logs";
import { PricingPage } from "@/features/pricing";
import { RoutingPage } from "@/features/routing";
import type { VersionedRequestLogDeepLink, RequestLogDeepLink } from "@/lib/types/requestLogDeepLinks";
import type { VersionedRoutingDeepLink, RoutingDeepLink } from "@/lib/types/routingDeepLinks";
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
  openRoutingDeepLink: (link: RoutingDeepLink) => void;
  routingDeepLink: VersionedRoutingDeepLink | null;
  openRequestLogDeepLink: (link: RequestLogDeepLink) => void;
  requestLogDeepLink: VersionedRequestLogDeepLink | null;
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
          onOpenRoutingDeepLink={actions.openRoutingDeepLink}
          onOpenStation={actions.openStation}
        />
      );
    case "keyPool":
      return (
        <KeyPoolPage
          onAddKey={actions.addKey}
          onEditKey={actions.editKey}
          onOpenRoutingDeepLink={actions.openRoutingDeepLink}
        />
      );
    case "channels":
      return <ChannelStatusPage onOpenRoutingDeepLink={actions.openRoutingDeepLink} />;
    case "collectors":
      return <CollectorsPage onOpenRoutingDeepLink={actions.openRoutingDeepLink} />;
    case "changes":
      return <ChangeCenterPage onOpenRoutingDeepLink={actions.openRoutingDeepLink} />;
    case "pricing":
      return (
        <PricingPage
          onOpenModelBasePrices={actions.openModelBasePrices}
          onOpenRoutingDeepLink={actions.openRoutingDeepLink}
        />
      );
    case "routing":
      return (
        <RoutingPage
          deepLink={actions.routingDeepLink}
          onOpenRequestLog={(requestLogId) =>
            actions.openRequestLogDeepLink({
              kind: "request-log",
              requestLogId,
              source: "routing_decision_trace",
            })
          }
        />
      );
    case "logs":
      return (
        <LogsPage
          deepLink={actions.requestLogDeepLink}
          onOpenRoutingDeepLink={actions.openRoutingDeepLink}
        />
      );
    case "settings":
      return <SettingsPage />;
    case "dashboard":
    default:
      return <DashboardPage />;
  }
});
