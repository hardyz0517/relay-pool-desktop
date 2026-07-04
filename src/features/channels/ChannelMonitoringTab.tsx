import { EmptyState } from "@/components/ui";

type ChannelMonitoringTabProps = {
  onHealthChanged: () => void;
};

export function ChannelMonitoringTab({ onHealthChanged }: ChannelMonitoringTabProps) {
  void onHealthChanged;

  return (
    <EmptyState
      title="监控即将接入"
      description="下一步会在这里放置渠道探测和健康变化。"
    />
  );
}
