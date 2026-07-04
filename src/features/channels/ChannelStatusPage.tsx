import { useState } from "react";
import { PageScaffold } from "@/components/shell/PageScaffold";
import { SegmentedControl } from "@/components/ui";
import { ChannelMonitoringTab } from "./ChannelMonitoringTab";
import { ChannelStatusTab } from "./ChannelStatusTab";

type ChannelTab = "status" | "monitoring";

export function ChannelStatusPage() {
  const [activeTab, setActiveTab] = useState<ChannelTab>("status");
  const [statusRefreshToken, setStatusRefreshToken] = useState(0);

  return (
    <PageScaffold
      title="渠道状态"
      actions={
        <SegmentedControl
          ariaLabel="渠道页面"
          value={activeTab}
          options={[
            { value: "status", label: "状态" },
            { value: "monitoring", label: "监控" },
          ]}
          onChange={setActiveTab}
        />
      }
    >
      {activeTab === "status" ? (
        <ChannelStatusTab refreshToken={statusRefreshToken} />
      ) : (
        <ChannelMonitoringTab onHealthChanged={() => setStatusRefreshToken((value) => value + 1)} />
      )}
    </PageScaffold>
  );
}
