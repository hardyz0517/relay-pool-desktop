import { SelectControl } from "@/components/ui";
import type {
  ChannelMonitorClientProfileId,
  ChannelMonitorProtocolKind,
  MonitoringCapabilityCatalog,
} from "@/lib/types/channelMonitors";
import { profileLabel } from "@/lib/channelMonitorDisplay";

type MonitorProfileSelectorProps = {
  value: ChannelMonitorClientProfileId;
  protocolKind: ChannelMonitorProtocolKind;
  capabilities: MonitoringCapabilityCatalog | undefined;
  onChange: (value: ChannelMonitorClientProfileId, version: number) => void;
};

export function MonitorProfileSelector({
  value,
  protocolKind,
  capabilities,
  onChange,
}: MonitorProfileSelectorProps) {
  const profiles = capabilities?.profiles ?? [];
  const options = profiles.map((profile) => ({
      value: profile.id,
      label: profileLabel(profile.id, profile.cliCompat),
      description: `${profile.method} ${profile.path} · v${profile.version}`,
      disabled: !profile.enabled || !profile.supportedProtocols.includes(protocolKind),
    }));

  return (
    <SelectControl
      ariaLabel="请求 Profile"
      value={value}
      options={options}
      placeholder="请选择请求 Profile"
      onChange={(nextValue) => {
        const profile = profiles.find((item) => item.id === nextValue);
        if (profile) {
          onChange(nextValue as ChannelMonitorClientProfileId, profile.version);
        }
      }}
      className="w-full"
      menuClassName="min-w-[280px]"
    />
  );
}
export { profileLabel };
