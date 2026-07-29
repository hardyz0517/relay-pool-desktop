import type { FormEvent, ReactNode } from "react";
import { Button, Dialog, SelectControl } from "@/components/ui";
import type { StationGroupOption } from "@/lib/types/groupFacts";
import type { Station } from "@/lib/types/stations";
import type { KeyPoolItem } from "@/lib/types/stationKeys";
import {
  CLEAR_GROUP_BINDING_VALUE,
  KEEP_GROUP_BINDING_VALUE,
  groupNameForDialogSelection,
  type KeyPoolEditForm,
} from "./KeyPoolFormModel";

export function KeyEditDialog({
  actionSaving,
  form,
  groupOptions,
  mode,
  onClose,
  onFormChange,
  onSave,
  onStationChange,
  renderCurrentGroupOption,
  renderGroupOptionLabel,
  sourceItem,
  stations,
}: {
  actionSaving: boolean;
  form: KeyPoolEditForm;
  groupOptions: StationGroupOption[];
  mode: "create" | "edit";
  onClose: () => void;
  onFormChange: (next: KeyPoolEditForm) => void;
  onSave: (event: FormEvent<HTMLFormElement>) => void;
  onStationChange?: (stationId: string) => void;
  renderCurrentGroupOption: (sourceItem: KeyPoolItem | null, options: StationGroupOption[]) => Array<{ value: string; label: ReactNode }>;
  renderGroupOptionLabel: (option: StationGroupOption) => ReactNode;
  sourceItem: KeyPoolItem | null;
  stations: Station[];
}) {
  const creating = mode === "create";
  const bindingOptions = [
    ...groupOptions
      .filter((option) => option.groupBindingId)
      .map((option) => ({
        value: option.groupBindingId ?? option.value,
        label: renderGroupOptionLabel(option),
      })),
    ...renderCurrentGroupOption(sourceItem, groupOptions),
  ];
  return (
    <Dialog
      open
      title={creating ? "新增密钥" : "编辑密钥"}
      description={creating ? "选择已有中转站并保存一枚可调度密钥。" : "密钥留空则保留旧值。"}
      onClose={onClose}
      footer={
        <div className="flex justify-end gap-2">
          <Button variant="outline" onClick={onClose}>取消</Button>
          <Button type="submit" form="key-pool-edit-form" disabled={actionSaving}>{actionSaving ? "保存中" : "保存"}</Button>
        </div>
      }
    >
      <form id="key-pool-edit-form" className="grid gap-4 p-5" onSubmit={onSave}>
        {creating && (
          <div className="grid gap-2 rounded-[var(--surface-radius)] border border-info-border bg-info-surface p-3">
            <div className="text-xs font-semibold text-foreground">预设中转站</div>
            <SelectControl
              ariaLabel="预设中转站"
              className={inputClassName}
              value={form.stationId}
              options={stations.map((station) => ({ value: station.id, label: station.name }))}
              onChange={(stationId) => onStationChange?.(stationId)}
            />
          </div>
        )}
        <div className="grid gap-3 md:grid-cols-2">
          <Field label="名称">
            <input className={inputClassName} value={form.name} onChange={(event) => onFormChange({ ...form, name: event.target.value })} required />
          </Field>
          <Field label="优先级">
            <input className={inputClassName} type="number" value={form.priority} onChange={(event) => onFormChange({ ...form, priority: event.target.value })} />
          </Field>
        </div>
        <Field label="所属中转站">
          <input className={inputClassName} value={form.stationName} disabled />
        </Field>
        <Field label="密钥">
          <input
            className={inputClassName}
            value={form.apiKey}
            onChange={(event) => onFormChange({ ...form, apiKey: event.target.value })}
            placeholder={creating ? "sk-..." : "留空保留旧密钥"}
            required={creating}
            type="password"
          />
        </Field>
        <div className="grid gap-3 md:grid-cols-3">
          <Field label="分组">
            <SelectControl
              ariaLabel="分组"
              className={inputClassName}
              value={form.groupBindingId}
              options={[
                ...(creating
                  ? [{ value: "", label: bindingOptions.length ? "不绑定分组" : "暂无可用分组" }]
                  : [
                      { value: KEEP_GROUP_BINDING_VALUE, label: "不调整绑定" },
                      ...(sourceItem?.groupBindingId ? [{ value: CLEAR_GROUP_BINDING_VALUE, label: "清除绑定" }] : []),
                    ]),
                ...bindingOptions,
              ]}
              onChange={(groupBindingId) => {
                onFormChange({
                  ...form,
                  groupBindingId,
                  groupName: groupNameForDialogSelection(groupBindingId, sourceItem, groupOptions, form.groupName),
                });
              }}
            />
          </Field>
          <Field label="档位">
            <input className={inputClassName} value={form.tierLabel} onChange={(event) => onFormChange({ ...form, tierLabel: event.target.value })} />
          </Field>
          <Field label="状态">
            <SelectControl
              ariaLabel="密钥状态"
              className={inputClassName}
              value={form.status}
              options={[
                { value: "unchecked", label: "未检测" },
                { value: "healthy", label: "正常" },
                { value: "warning", label: "警告" },
                { value: "error", label: "错误" },
                { value: "disabled", label: "禁用" },
              ]}
              onChange={(status) => onFormChange({ ...form, status })}
            />
          </Field>
        </div>
        <label className="flex items-center gap-2 text-sm text-foreground">
          <input checked={form.enabled} className="h-4 w-4 accent-primary" type="checkbox" onChange={(event) => onFormChange({ ...form, enabled: event.target.checked })} />
          启用
        </label>
        <div className="grid gap-2 rounded-[var(--surface-radius)] border border-info-border bg-info-surface p-3">
          <div className="text-xs font-semibold text-foreground">协议能力</div>
          <div className="grid gap-2 sm:grid-cols-2 md:grid-cols-3">
            <CheckField label="聊天补全" checked={form.supportsChatCompletions} onChange={(checked) => onFormChange({ ...form, supportsChatCompletions: checked })} />
            <CheckField label="响应接口" checked={form.supportsResponses} onChange={(checked) => onFormChange({ ...form, supportsResponses: checked })} />
            <CheckField label="向量接口" checked={form.supportsEmbeddings} onChange={(checked) => onFormChange({ ...form, supportsEmbeddings: checked })} />
            <CheckField label="流式响应" checked={form.supportsStream} onChange={(checked) => onFormChange({ ...form, supportsStream: checked })} />
            <CheckField label="工具调用" checked={form.supportsTools} onChange={(checked) => onFormChange({ ...form, supportsTools: checked })} />
            <CheckField label="图片输入" checked={form.supportsVision} onChange={(checked) => onFormChange({ ...form, supportsVision: checked })} />
            <CheckField label="推理模型" checked={form.supportsReasoning} onChange={(checked) => onFormChange({ ...form, supportsReasoning: checked })} />
          </div>
        </div>
        <div className="grid gap-3 md:grid-cols-3">
          <Field label="允许模型">
            <textarea className={`${inputClassName} min-h-24 resize-none py-2`} value={form.modelAllowlist} onChange={(event) => onFormChange({ ...form, modelAllowlist: event.target.value })} placeholder="每行一个模型；留空表示全部模型" />
          </Field>
          <Field label="禁止模型">
            <textarea className={`${inputClassName} min-h-24 resize-none py-2`} value={form.modelBlocklist} onChange={(event) => onFormChange({ ...form, modelBlocklist: event.target.value })} placeholder="每行一个模型" />
          </Field>
          <Field label="优先模型">
            <textarea className={`${inputClassName} min-h-24 resize-none py-2`} value={form.preferredModels} onChange={(event) => onFormChange({ ...form, preferredModels: event.target.value })} placeholder="每行一个模型" />
          </Field>
        </div>
        <div className="grid gap-3 md:grid-cols-[auto_minmax(0,1fr)]">
          <label className="flex items-center gap-2 text-sm text-foreground">
            <input checked={form.onlyUseAsBackup} className="h-4 w-4 accent-primary" type="checkbox" onChange={(event) => onFormChange({ ...form, onlyUseAsBackup: event.target.checked })} />
            仅作为备用密钥
          </label>
          <Field label="路由标签">
            <input className={inputClassName} value={form.routingTags} onChange={(event) => onFormChange({ ...form, routingTags: event.target.value })} placeholder="逗号分隔，例如 高优先级, 低延迟" />
          </Field>
        </div>
        <Field label="备注">
          <textarea className={`${inputClassName} min-h-20 resize-none py-2`} value={form.note} onChange={(event) => onFormChange({ ...form, note: event.target.value })} />
        </Field>
      </form>
    </Dialog>
  );
}

function Field({ label, children }: { label: string; children: ReactNode }) {
  return (
    <label className="grid gap-1.5 text-xs font-medium text-muted-foreground">
      {label}
      {children}
    </label>
  );
}

function CheckField({
  label,
  checked,
  onChange,
}: {
  label: string;
  checked: boolean;
  onChange: (checked: boolean) => void;
}) {
  return (
    <label className="flex items-center gap-2 text-sm text-foreground">
      <input
        checked={checked}
        className="h-4 w-4 accent-primary"
        type="checkbox"
        onChange={(event) => onChange(event.target.checked)}
      />
      {label}
    </label>
  );
}

const inputClassName =
  "h-8 rounded-[12px] border border-info-border bg-info-surface px-3 text-sm text-foreground outline-none transition focus:border-ring focus:bg-surface focus:ring-2 focus:ring-ring/20";
