import { useEffect, useRef, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { PageScaffold } from "@/components/shell/PageScaffold";
import { SegmentedControl } from "@/components/ui";
import { queryKeys } from "@/lib/query/queryKeys";
import type { RoutingDeepLink } from "@/lib/types/routingDeepLinks";
import { ChannelMonitoringTab } from "./ChannelMonitoringTab";
import { ChannelStatusTab } from "./ChannelStatusTab";
import { OfficialStatusTab } from "./OfficialStatusTab";
import type { ChannelViewPreparationPort } from "./channelViewPreparation";

type ChannelTab = "status" | "official" | "monitoring";

type MonitoringRoutingDeepLink = Extract<RoutingDeepLink, { kind: "station-key" }> & {
  source: "monitoring";
};

type ChannelStatusPageProps = {
  onOpenRoutingDeepLink?: (link: MonitoringRoutingDeepLink) => void;
  onViewPreparationPort?: (port: ChannelViewPreparationPort | null) => void;
};

export function ChannelStatusPage({
  onOpenRoutingDeepLink,
  onViewPreparationPort,
}: ChannelStatusPageProps = {}) {
  const queryClient = useQueryClient();
  const [activeTab, setActiveTab] = useState<ChannelTab>("status");
  const activeTabRef = useRef(activeTab);
  const mountedRef = useRef(false);
  const channelPageTabs = (
    <div data-tour="channels-tabs">
      <SegmentedControl
        ariaLabel="渠道页面"
        value={activeTab}
        options={[
          { value: "status", label: "本地状态" },
          { value: "official", label: "官方状态" },
          { value: "monitoring", label: "探针管理" },
        ]}
        onChange={setActiveTab}
      />
    </div>
  );

  useEffect(() => {
    activeTabRef.current = activeTab;
  }, [activeTab]);

  useEffect(() => {
    mountedRef.current = true;
    if (!onViewPreparationPort) return () => { mountedRef.current = false; };

    const showView = (next: ChannelTab) => {
      const previous = activeTabRef.current;
      activeTabRef.current = next;
      setActiveTab(next);
      let restored = false;
      return () => {
        if (restored) return;
        restored = true;
        if (mountedRef.current) {
          activeTabRef.current = previous;
          setActiveTab(previous);
        }
      };
    };
    const port: ChannelViewPreparationPort = {
      showLocalView: () => showView("status"),
      showOfficialView: () => showView("official"),
      showMonitoringView: () => showView("monitoring"),
    };
    onViewPreparationPort(port);
    return () => {
      mountedRef.current = false;
      onViewPreparationPort(null);
    };
  }, [onViewPreparationPort]);

  if (activeTab === "monitoring") {
    return (
      <div data-tour="channels-monitoring-list">
        <ChannelMonitoringTab
          headerActions={channelPageTabs}
          onHealthChanged={() => void queryClient.invalidateQueries({ queryKey: queryKeys.channelStatus })}
          onOpenRoutingDeepLink={onOpenRoutingDeepLink}
        />
      </div>
    );
  }

  if (activeTab === "official") {
    return <PageScaffold title="官方状态" actions={channelPageTabs}><OfficialStatusTab /></PageScaffold>;
  }

  return (
    <PageScaffold
      title="渠道状态"
      actions={channelPageTabs}
    >
      <ChannelStatusTab />
    </PageScaffold>
  );
}
