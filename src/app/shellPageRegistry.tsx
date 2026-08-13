import { memo } from "react";
import { ChannelStatusPage } from "@/features/channels";
import { ChangeCenterPage, ChangeCenterSettingsPage, type ChangeCenterView } from "@/features/changes";
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
import { settingsQueryOptions } from "@/lib/query/resourceQueries";
import { useActivityQuery } from "@/lib/query/useActivityQuery";

export type ShellPageActions = {
  addProvider: () => void;
  editProvider: (stationId: string) => void;
  openStation: (station: Station) => void;
  addKey: (stationId: string | null) => void;
  editKey: (stationKeyId: string) => void;
  openKeyPool: () => void;
  openLocalRouting: () => void;
  openRequestLogs: () => void;
  openModelBasePrices: () => void;
  openChangeCenterSettings: () => void;
  changeCenterView: ChangeCenterView;
  setChangeCenterView: (view: ChangeCenterView) => void;
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
  const settingsQuery = useActivityQuery(settingsQueryOptions());
  const routingDeepLinkHandler = settingsQuery.data?.developerModeEnabled
    ? actions.openRoutingDeepLink
    : undefined;

  switch (routeId) {
    case "stations":
      return (
        <StationsPage
          onAddProvider={actions.addProvider}
          onEditProvider={actions.editProvider}
          onOpenRoutingDeepLink={routingDeepLinkHandler}
          onOpenStation={actions.openStation}
        />
      );
    case "keyPool":
      return (
        <KeyPoolPage
          onAddKey={actions.addKey}
          onEditKey={actions.editKey}
          onOpenRoutingDeepLink={routingDeepLinkHandler}
        />
      );
    case "channels":
      return <ChannelStatusPage onOpenRoutingDeepLink={routingDeepLinkHandler} />;
    case "collectors":
      return <CollectorsPage onOpenRoutingDeepLink={routingDeepLinkHandler} />;
    case "changes":
      return (
        <ChangeCenterPage
          onOpenRoutingDeepLink={routingDeepLinkHandler}
          onOpenSettings={actions.openChangeCenterSettings}
          selectedView={actions.changeCenterView}
          onSelectedViewChange={actions.setChangeCenterView}
        />
      );
    case "pricing":
      return (
        <PricingPage
          onOpenModelBasePrices={actions.openModelBasePrices}
          onOpenRoutingDeepLink={routingDeepLinkHandler}
        />
      );
    case "routing":
      return (
        <RoutingPage
          deepLink={actions.routingDeepLink}
          developerModeEnabled={settingsQuery.data?.developerModeEnabled === true}
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
          onOpenRoutingDeepLink={routingDeepLinkHandler}
        />
      );
    case "settings":
      return <SettingsPage />;
    case "dashboard":
    default:
      return (
        <DashboardPage
          onOpenKeyPool={actions.openKeyPool}
          onOpenLocalRouting={actions.openLocalRouting}
          onOpenRequestLogs={actions.openRequestLogs}
        />
      );
  }
});
