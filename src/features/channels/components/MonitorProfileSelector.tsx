import { SelectControl } from "@/components/ui";
import type { MonitoringCapabilityCatalog } from "@/lib/types/channelMonitors";

type MonitorProfileSelectorProps = {
  value: string;
  capabilities: MonitoringCapabilityCatalog | undefined;
  onChange: (value: string) => void;
};

export function MonitorProfileSelector({
  value,
  capabilities,
  onChange,
}: MonitorProfileSelectorProps) {
  const options = [
    { value: "", label: "全部 Profile" },
    ...(capabilities?.profiles ?? []).map((profile) => ({
      value: profile.id,
      label: profile.cliCompat ? `${profile.id}（CLI）` : profile.id,
      description: `${profile.method} ${profile.path} · v${profile.version}`,
      disabled: !profile.enabled,
    })),
  ];

  return (
    <SelectControl
      ariaLabel="Profile 筛选"
      value={value}
      options={options}
      onChange={onChange}
      className="min-w-[180px]"
      menuClassName="min-w-[260px]"
    />
  );
}
