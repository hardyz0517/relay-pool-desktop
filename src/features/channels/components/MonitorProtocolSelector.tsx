import type {
  ChannelMonitorProtocolKind,
  MonitoringCapabilityCatalog,
} from "@/lib/types/channelMonitors";
import { monitorProtocolCopy, protocolLabel } from "@/lib/channelMonitorDisplay";

type MonitorProtocolSelectorProps = {
  value: ChannelMonitorProtocolKind;
  capabilities: MonitoringCapabilityCatalog | undefined;
  onChange: (value: ChannelMonitorProtocolKind) => void;
};

export function MonitorProtocolSelector({
  value,
  capabilities,
  onChange,
}: MonitorProtocolSelectorProps) {
  return (
    <div className="grid gap-2 rounded-[8px] border border-info-border bg-info-surface p-2 md:grid-cols-2 xl:grid-cols-3">
      {(capabilities?.protocols ?? []).map((protocol) => {
        const id = protocol.id as ChannelMonitorProtocolKind;
        const copy = monitorProtocolCopy[id] ?? { title: protocol.id, description: "使用该协议执行监控探测。" };
        const active = value === id;
        return (
          <button
            key={protocol.id}
            type="button"
            className={`min-h-[72px] rounded-[8px] border bg-surface px-3 py-2 text-left transition ${
              active
                ? "border-primary text-primary shadow-surface"
                : "border-border text-muted-foreground hover:border-primary hover:bg-selected"
            } ${protocol.enabled ? "" : "cursor-not-allowed opacity-50"}`}
            disabled={!protocol.enabled}
            onClick={() => onChange(id)}
          >
            <div className="text-sm font-semibold text-foreground">{copy.title}</div>
            <div className="mt-1 text-xs leading-5 text-muted-foreground">{copy.description}</div>
          </button>
        );
      })}
    </div>
  );
}
export { protocolLabel };
