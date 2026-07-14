import { Sparkles, Trash2 } from "lucide-react";
import { Button, SelectControl, type SelectOption } from "@/components/ui";
import { groupCategoryDefinitions, type StationGroupCategory } from "@/lib/groupCategories";
import { cn } from "@/lib/utils";

export type StationGroupDraftSource = "manual" | "remote";

export type StationGroupDraft = {
  clientId: string;
  groupBindingId: string | null;
  groupKeyHash: string;
  groupIdHash: string | null;
  groupName: string;
  rateMultiplier: string;
  inferredGroupCategory: StationGroupCategory;
  groupCategoryOverride: StationGroupCategory | null;
  source: StationGroupDraftSource;
  deleteRequested: boolean;
};

type StationGroupRowsEditorProps = {
  rows: StationGroupDraft[];
  disabled?: boolean;
  developerModeEnabled?: boolean;
  onRowsChange: (rows: StationGroupDraft[]) => void;
};

const inputClassName =
  "h-8 w-full min-w-0 rounded-[var(--surface-radius)] border border-border bg-surface px-2.5 text-xs text-foreground outline-none transition placeholder:text-muted-foreground/70 focus:border-ring focus:ring-2 focus:ring-ring/30 disabled:bg-surface-subtle disabled:text-muted-foreground";
const groupRowsGridTemplate = "minmax(9rem,1fr) 7.5rem 6rem 5.5rem 2.5rem";
const autoGroupCategoryValue = "__auto__";
type GroupCategorySelectValue = typeof autoGroupCategoryValue | StationGroupCategory;
export const groupCategoryOptions = groupCategoryDefinitions.map((definition) => ({
  value: definition.value,
  label: definition.label,
}));

export function createEmptyStationGroupDraft(index: number): StationGroupDraft {
  return {
    clientId: `station-group-draft-${Date.now()}-${index}`,
    groupBindingId: null,
    groupKeyHash: "",
    groupIdHash: null,
    groupName: "",
    rateMultiplier: "",
    inferredGroupCategory: "unknown",
    groupCategoryOverride: null,
    source: "manual",
    deleteRequested: false,
  };
}

export function StationGroupRowsEditor({
  rows,
  disabled,
  developerModeEnabled = false,
  onRowsChange,
}: StationGroupRowsEditorProps) {
  const visibleRows = rows.filter((row) => !row.deleteRequested);

  function updateRow(clientId: string, patch: Partial<StationGroupDraft>) {
    onRowsChange(rows.map((row) => (row.clientId === clientId ? { ...row, ...patch } : row)));
  }

  function deleteRow(target: StationGroupDraft) {
    if (target.groupBindingId) {
      updateRow(target.clientId, { deleteRequested: true });
      return;
    }
    onRowsChange(rows.filter((row) => row.clientId !== target.clientId));
  }

  return (
    <div className="grid gap-2">
      <div className="overflow-x-auto">
        <div className="min-w-[420px]">
          <div
            className="grid h-7 items-center gap-2 border-b border-border px-1 text-[11px] font-medium text-muted-foreground"
            style={{ gridTemplateColumns: groupRowsGridTemplate }}
          >
            <span>分组</span>
            <span>类型</span>
            <span>倍率</span>
            <span>来源</span>
            <span className="text-right">操作</span>
          </div>

          <div className="grid gap-1.5 py-2">
            {visibleRows.map((row, index) => (
              <div
                key={row.clientId}
                className="grid min-h-9 items-center gap-2"
                style={{ gridTemplateColumns: groupRowsGridTemplate }}
              >
                <input
                  className={inputClassName}
                  disabled={disabled}
                  value={row.groupName}
                  onChange={(event) =>
                    updateRow(row.clientId, {
                      groupIdHash: null,
                      groupKeyHash: "",
                      groupName: event.target.value,
                      source: "manual",
                    })
                  }
                  placeholder={`分组 ${index + 1}`}
                />
                <SelectControl<GroupCategorySelectValue>
                  ariaLabel="选择分组类型"
                  className="h-8 w-full min-w-0 px-2.5 text-xs shadow-none"
                  disabled={disabled}
                  value={row.groupCategoryOverride ?? autoGroupCategoryValue}
                  menuClassName="min-w-[12rem]"
                  options={groupCategorySelectOptions(
                    row.inferredGroupCategory,
                    developerModeEnabled,
                  )}
                  onChange={(value) =>
                    updateRow(row.clientId, {
                      groupCategoryOverride:
                        value === autoGroupCategoryValue
                          ? null
                          : value,
                    })
                  }
                />
                <input
                  className={inputClassName}
                  disabled={disabled}
                  inputMode="decimal"
                  value={row.rateMultiplier}
                  onChange={(event) =>
                    updateRow(row.clientId, {
                      rateMultiplier: event.target.value,
                      source: "manual",
                    })
                  }
                  placeholder="1"
                />
                <span className="min-w-0 truncate text-xs text-muted-foreground">
                  {row.source === "remote" ? "远端" : "手动"}
                </span>
                <Button
                  aria-label={`删除分组 ${index + 1}`}
                  className={cn("justify-self-end", row.groupBindingId && "text-danger-foreground")}
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
        <div className="rounded-[var(--surface-radius)] border border-dashed border-border bg-surface-subtle px-3 py-2 text-xs text-muted-foreground">
          暂无分组，可手动添加或从远端同步。
        </div>
      )}
    </div>
  );
}

function groupCategoryLabel(value: StationGroupCategory) {
  return groupCategoryOptions.find((option) => option.value === value)?.label ?? "未知";
}

function groupCategorySelectOptions(
  inferredGroupCategory: StationGroupCategory,
  developerModeEnabled: boolean,
): SelectOption<GroupCategorySelectValue>[] {
  const manualOptions = groupCategoryDefinitions
    .filter(
      (definition) =>
        developerModeEnabled ||
        (definition.value !== "embedding" && definition.value !== "rerank"),
    )
    .map((definition, index) => ({
      value: definition.value,
      label: definition.label,
      sectionLabel: index === 0 ? "手动指定" : undefined,
    }));

  return [
    {
      value: autoGroupCategoryValue,
      label: "跟随识别结果",
      triggerLabel: groupCategoryLabel(inferredGroupCategory),
      description: `当前识别：${groupCategoryLabel(inferredGroupCategory)}`,
      leadingIcon: <Sparkles className="h-3.5 w-3.5" />,
    },
    ...manualOptions,
  ];
}
