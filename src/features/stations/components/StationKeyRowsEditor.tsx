import { Trash2 } from "lucide-react";
import { Button, SelectControl, SwitchControl } from "@/components/ui";
import type { StationGroupOption } from "@/lib/types/groupFacts";
import { cn } from "@/lib/utils";
import {
  findMatchingGroupOption,
  formatMultiplier,
  noGroupOptionValue,
  normalizeStationGroupOptions,
  stationGroupSelectValue,
} from "../groupOptionViewModels";

export type StationKeyDraft = {
  clientId: string;
  id: string | null;
  name: string;
  apiKey: string;
  groupBindingId: string | null;
  groupIdHash: string | null;
  groupName: string;
  rateMultiplier: string;
  enabled: boolean;
  note: string;
  deleteRequested: boolean;
};

export type StationKeyGroupOption = StationGroupOption;

type StationKeyRowsEditorProps = {
  rows: StationKeyDraft[];
  disabled?: boolean;
  groupOptions?: StationKeyGroupOption[];
  onRowsChange: (rows: StationKeyDraft[]) => void;
};

const inputClassName =
  "h-8 w-full min-w-0 rounded-[var(--surface-radius)] border border-border bg-white px-2.5 text-xs text-slate-800 outline-none transition placeholder:text-slate-400 focus:border-[hsl(var(--accent)/0.5)] focus:ring-2 focus:ring-[hsl(var(--accent)/0.16)] disabled:bg-slate-50 disabled:text-slate-500";
const selectClassName =
  "h-8 w-full min-w-0 px-2.5 text-xs shadow-none";
const keyRowsGridTemplate = "minmax(7rem,1fr) minmax(11rem,1.35fr) minmax(8rem,0.9fr) 6rem 3.75rem 2.5rem";
const noGroupValue = noGroupOptionValue;

export function createEmptyStationKeyDraft(index: number): StationKeyDraft {
  return {
    clientId: `station-key-draft-${Date.now()}-${index}`,
    id: null,
    name: "",
    apiKey: "",
    groupBindingId: null,
    groupIdHash: null,
    groupName: "",
    rateMultiplier: "",
    enabled: true,
    note: "",
    deleteRequested: false,
  };
}

export function StationKeyRowsEditor({
  rows,
  disabled,
  groupOptions = [],
  onRowsChange,
}: StationKeyRowsEditorProps) {
  const visibleRows = rows.filter((row) => !row.deleteRequested);
  const normalizedGroupOptions = normalizeStationGroupOptions([
    ...groupOptions,
    ...visibleRows
      .filter((row) => row.groupBindingId || row.groupIdHash || row.groupName.trim())
      .map((row) => {
        const option = {
          value: "",
          groupBindingId: row.groupBindingId,
          groupIdHash: row.groupIdHash,
          groupName: row.groupName,
          rateMultiplier: parseDraftMultiplier(row.rateMultiplier),
          rateSource: "key_draft",
          selectableForRemoteKey: Boolean(row.groupBindingId || row.groupIdHash),
        };
        return { ...option, value: stationGroupSelectValue(option) };
      }),
  ]);
  const selectOptions = [
    { value: noGroupValue, label: "无", description: "不绑定分组，手动填写倍率" },
    ...normalizedGroupOptions.map((group) => ({
      value: stationGroupSelectValue(group),
      label: group.groupName,
      description:
        group.rateMultiplier === null ? "未采集倍率" : `${formatMultiplier(group.rateMultiplier)}x`,
    })),
  ];

  function updateRow(clientId: string, patch: Partial<StationKeyDraft>) {
    onRowsChange(rows.map((row) => (row.clientId === clientId ? { ...row, ...patch } : row)));
  }

  function selectGroup(row: StationKeyDraft, value: string) {
    if (value === noGroupValue) {
      updateRow(row.clientId, { groupBindingId: null, groupIdHash: null, groupName: "" });
      return;
    }
    const selectedGroup =
      normalizedGroupOptions.find((group) => stationGroupSelectValue(group) === value) ?? null;
    if (!selectedGroup) {
      return;
    }
    updateRow(row.clientId, {
      groupBindingId: selectedGroup.groupBindingId,
      groupIdHash: selectedGroup.groupIdHash,
      groupName: selectedGroup.groupName,
      rateMultiplier:
        selectedGroup.rateMultiplier === null
          ? row.rateMultiplier
          : formatMultiplier(selectedGroup.rateMultiplier),
    });
  }

  function deleteRow(target: StationKeyDraft) {
    if (target.id) {
      updateRow(target.clientId, { deleteRequested: true });
      return;
    }
    onRowsChange(rows.filter((row) => row.clientId !== target.clientId));
  }

  return (
    <div className="grid gap-2">
      <div className="overflow-x-auto">
        <div className="min-w-[760px]">
          <div
            className="grid h-7 items-center gap-2 border-b border-border px-1 text-[11px] font-medium text-muted-foreground"
            style={{ gridTemplateColumns: keyRowsGridTemplate }}
          >
            <span>名称</span>
            <span>密钥</span>
            <span>分组</span>
            <span>倍率</span>
            <span>启用</span>
            <span className="text-right">操作</span>
          </div>

          <div className="grid gap-1.5 py-2">
            {visibleRows.map((row, index) => (
              <div
                key={row.clientId}
                className="grid min-h-9 items-center gap-2"
                style={{ gridTemplateColumns: keyRowsGridTemplate }}
              >
                <input
                  className={inputClassName}
                  disabled={disabled}
                  value={row.name}
                  onChange={(event) => updateRow(row.clientId, { name: event.target.value })}
                  placeholder={`密钥 ${index + 1}`}
                />
                <input
                  className={inputClassName}
                  disabled={disabled}
                  type="password"
                  value={row.apiKey}
                  onChange={(event) => updateRow(row.clientId, { apiKey: event.target.value })}
                  placeholder={row.id ? "留空保留旧密钥" : "sk-..."}
                />
                <SelectControl
                  ariaLabel={`选择密钥 ${index + 1} 分组`}
                  className={selectClassName}
                  disabled={disabled}
                  menuClassName="text-xs"
                  options={selectOptions}
                  value={resolveSelectedGroupValue(row, normalizedGroupOptions)}
                  onChange={(value) => selectGroup(row, value)}
                />
                <input
                  className={inputClassName}
                  disabled={disabled || resolveSelectedGroupValue(row, normalizedGroupOptions) !== noGroupValue}
                  inputMode="decimal"
                  value={row.rateMultiplier}
                  onChange={(event) => updateRow(row.clientId, { rateMultiplier: event.target.value })}
                  placeholder="1"
                />
                <SwitchControl
                  ariaLabel={`切换密钥 ${index + 1}`}
                  checked={row.enabled}
                  className="h-6 justify-center border-0 bg-transparent px-0 shadow-none"
                  disabled={disabled}
                  offLabel="停用"
                  onCheckedChange={() => updateRow(row.clientId, { enabled: !row.enabled })}
                  onLabel="启用"
                  showLabel={false}
                />
                <Button
                  aria-label={`删除密钥 ${index + 1}`}
                  className={cn("justify-self-end", row.id && "text-rose-700")}
                  disabled={disabled}
                  size="icon"
                  variant="ghost"
                  onClick={() => deleteRow(row)}
                >
                  <Trash2 className="h-4 w-4" />
                </Button>
              </div>
            ))}
          </div>
        </div>
      </div>

      {visibleRows.length === 0 && (
        <div className="rounded-[var(--surface-radius)] border border-dashed border-border bg-slate-50 px-3 py-2 text-xs text-muted-foreground">
          暂无本地密钥，点击添加后录入。
        </div>
      )}
    </div>
  );
}

function resolveSelectedGroupValue(row: StationKeyDraft, groupOptions: StationKeyGroupOption[]) {
  if (!row.groupBindingId && !row.groupName.trim() && !row.groupIdHash) {
    return noGroupValue;
  }
  const selectedGroup = findMatchingGroupOption(row, groupOptions);
  return selectedGroup ? stationGroupSelectValue(selectedGroup) : noGroupValue;
}

function parseDraftMultiplier(value: string) {
  if (!value.trim()) {
    return null;
  }
  const multiplier = Number(value);
  return Number.isFinite(multiplier) ? multiplier : null;
}
