import { Dialog } from "@/components/ui";
import type { MonitoringCapabilityCatalog } from "@/lib/types/channelMonitors";

type MonitorDefinitionDialogProps = {
  open: boolean;
  capabilities: MonitoringCapabilityCatalog | undefined;
  onClose: () => void;
};

export function MonitorDefinitionDialog({
  open,
  capabilities,
  onClose,
}: MonitorDefinitionDialogProps) {
  return (
    <Dialog
      open={open}
      title="新建监控定义"
      description="V2 monitor definition 将在后续 definition command 完整接入；这里先暴露能力目录和配置边界。"
      onClose={onClose}
      className="max-w-[760px]"
    >
      <div className="space-y-4 p-5 text-sm">
        <div className="rounded-[var(--surface-radius)] border border-warning-border bg-warning-surface px-3 py-2 text-warning-foreground">
          当前已接入执行/read model；definition CRUD 的完整表单会使用相同的协议/profile 能力目录，避免再写旧模板式探针。
        </div>
        <section>
          <div className="mb-2 font-medium text-foreground">协议能力</div>
          <div className="grid gap-2 sm:grid-cols-2">
            {(capabilities?.protocols ?? []).map((protocol) => (
              <div key={protocol.id} className="rounded-[var(--surface-radius)] border border-border bg-surface-subtle px-3 py-2">
                <div className="font-medium">{protocol.id}</div>
                <div className="text-xs text-muted-foreground">
                  {protocol.enabled ? "enabled" : "disabled"} · {protocol.streaming ? "streaming" : "non-streaming"}
                </div>
              </div>
            ))}
          </div>
        </section>
        <section>
          <div className="mb-2 font-medium text-foreground">Profile 能力</div>
          <div className="space-y-2">
            {(capabilities?.profiles ?? []).map((profile) => (
              <div key={`${profile.id}-${profile.version}`} className="rounded-[var(--surface-radius)] border border-border bg-surface-subtle px-3 py-2">
                <div className="font-medium">
                  {profile.id}@{profile.version}
                  {profile.cliCompat ? <span className="ml-2 text-xs text-warning-foreground">CLI compat</span> : null}
                </div>
                <div className="mt-1 text-xs text-muted-foreground">
                  {profile.method} {profile.path} · {profile.supportedProtocols.join(", ")}
                </div>
              </div>
            ))}
          </div>
        </section>
      </div>
    </Dialog>
  );
}
