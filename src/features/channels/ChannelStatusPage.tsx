import { useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { PageScaffold } from "@/components/shell/PageScaffold";
import { SegmentedControl } from "@/components/ui";
import { queryKeys } from "@/lib/query/queryKeys";
import { ChannelMonitoringTab } from "./ChannelMonitoringTab";
import { ChannelStatusTab } from "./ChannelStatusTab";

type ChannelTab = "status" | "monitoring";

export function ChannelStatusPage() {
  const queryClient = useQueryClient();
  const [activeTab, setActiveTab] = useState<ChannelTab>("status");
  const channelPageTabs = (
    <SegmentedControl
      ariaLabel="渠道页面"
      value={activeTab}
      options={[
        { value: "status", label: "状态" },
        { value: "monitoring", label: "监控" },
      ]}
      onChange={setActiveTab}
    />
  );

  if (activeTab === "monitoring") {
    return (
      <ChannelMonitoringTab
        headerActions={channelPageTabs}
        onHealthChanged={() => void queryClient.invalidateQueries({ queryKey: queryKeys.channelStatus })}
      />
    );
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
