import {
  closestCenter,
  DndContext,
  DragOverlay,
  PointerSensor,
  useSensor,
  useSensors,
} from "@dnd-kit/core";
import { SortableContext, verticalListSortingStrategy } from "@dnd-kit/sortable";
import { Plus, Search } from "lucide-react";
import { StationGroupOptionLabel, StationGroupTriggerLabel } from "@/components/group/StationGroupChip";
import { PageScaffold } from "@/components/shell/PageScaffold";
import { Button, ConfirmDialog, EmptyState, SelectControl, StatusBadge } from "@/components/ui";
import { findMatchingGroupOption } from "@/lib/groupOptionViewModels";
import type { StationGroupOption } from "@/lib/types/groupFacts";
import type { KeyPoolItem } from "@/lib/types/stationKeys";
import { cn } from "@/lib/utils";
import { KeyConnectivityTestDialog } from "./KeyConnectivityTestDialog";
import { KeyEditDialog } from "./KeyEditDialog";
import {
  KeyRowContent,
  SortableKeyRow,
  TableHeadCell,
  keyPoolGridClassName,
  keyPoolTableClassName,
} from "./KeyPoolRows";
import { useKeyPoolPageController, type KeyPoolPageControllerOptions } from "./useKeyPoolPageController";

type KeyPoolPageProps = KeyPoolPageControllerOptions;
type KeyPoolPageShellProps = KeyPoolPageProps & {
  onOpenRoutingDeepLink?: (link: { kind: "station-key"; stationKeyId: string; source: "key_pool" }) => void;
};

export function KeyPoolPage(props: KeyPoolPageShellProps) {
  const { onOpenRoutingDeepLink, ...controllerProps } = props;
  const {
    activeDragItem,
    closeConnectivityDialog,
    closeEditDialog,
    connectivityCapabilities,
    connectivityDialogItem,
    connectivityProgressLabel,
    connectivityStreamFallbackReason,
    connectivityTestError,
    connectivityTestResult,
    creatingKey,
    displayError,
    displayedResponseText,
    dragEnabled,
    editForm,
    editingItem,
    filterMode,
    filteredEnabledCount,
    filteredItems,
    groupOptionsForEdit,
    handleAddKeyClick,
    handleConfirmDelete,
    handleCreateSave,
    handleCreateStationChange,
    handleDelete,
    handleDragCancel,
    handleDragEnd,
    handleDragStart,
    handleEdit,
    handleEditSave,
    handleRunConnectivityTest,
    handleTestConnectivity,
    handleToggleEnabled,
    handleToggleMonitoring,
    loading,
    monitorByKey,
    monitorStatusByKey,
    monitoringKeyId,
    pendingDeleteItem,
    query,
    saving,
    selectedStationId,
    setDisplayedResponseText,
    setEditForm,
    setFilterMode,
    setPendingDeleteItem,
    setQuery,
    setSelectedStationId,
    stationOptions,
    stations,
    testingKeyId,
  } = useKeyPoolPageController(controllerProps);
  const sensors = useSensors(useSensor(PointerSensor, { activationConstraint: { distance: 8 } }));

  return (
    <PageScaffold
      title="密钥池"
      status={
        <div className="flex min-w-0 flex-wrap items-center gap-1.5" aria-label="密钥池状态">
          <StatusBadge tone="info" className="bg-surface-subtle text-muted-foreground">
            {`${filteredItems.length} 密钥`}
          </StatusBadge>
          <StatusBadge tone={filteredEnabledCount > 0 ? "healthy" : "disabled"}>
            {`${filteredEnabledCount} 启用`}
          </StatusBadge>
        </div>
      }
      actions={
        <div className="flex items-center gap-2">
          <SelectControl
            ariaLabel="筛选中转站"
            className={selectClassName}
            value={selectedStationId}
            options={[
              { value: "all", label: "全部中转站" },
              ...stationOptions.map((station) => ({ value: station.id, label: station.label })),
            ]}
            onChange={setSelectedStationId}
          />
          <SelectControl
            ariaLabel="筛选启用状态"
            className={selectClassName}
            value={filterMode}
            options={[
              { value: "all", label: "全部状态" },
              { value: "enabled", label: "只看启用" },
              { value: "disabled", label: "只看禁用" },
            ]}
            onChange={setFilterMode}
          />
          <div className="relative">
            <Search className="pointer-events-none absolute left-2.5 top-2 h-4 w-4 text-muted-foreground" />
            <input className={`${selectClassName} pl-8`} value={query} onChange={(event) => setQuery(event.target.value)} placeholder="搜索密钥 / 站点" />
          </div>
          <Button
            variant="secondary"
            onClick={handleAddKeyClick}
            disabled={loading || saving || stations.length === 0}
          >
            <Plus className="h-4 w-4" />
            新增密钥
          </Button>
        </div>
      }
    >
      {displayError && (
        <div className="mb-3 rounded-[var(--surface-radius)] border border-danger-border bg-danger-surface px-3 py-2 text-sm text-danger-foreground">
          {displayError}
        </div>
      )}
      {loading ? (
        <div className="rounded-[var(--surface-radius)] border border-info-border bg-surface/85 px-4 py-5 text-sm text-muted-foreground">
          正在读取密钥池...
        </div>
      ) : filteredItems.length === 0 ? (
        <EmptyState
          title="还没有可管理的密钥"
          description="先在中转站页创建一个站点和它下面的密钥。"
        />
      ) : (
        <div className="space-y-[var(--shell-page-gap)]">
          <DndContext
            sensors={sensors}
            collisionDetection={closestCenter}
            onDragStart={handleDragStart}
            onDragCancel={handleDragCancel}
            onDragEnd={handleDragEnd}
          >
            <SortableContext items={filteredItems.map((item) => item.id)} strategy={verticalListSortingStrategy}>
              <div className="overflow-x-auto">
                <div className={keyPoolTableClassName}>
                  <div className={cn(keyPoolGridClassName, "border-b border-border px-3 pb-2 text-[11px] font-medium text-muted-foreground")}>
                    <div aria-hidden />
                    <TableHeadCell>名称</TableHeadCell>
                    <TableHeadCell align="center">状态</TableHeadCell>
                    <TableHeadCell align="center">调度</TableHeadCell>
                    <TableHeadCell align="center">监控</TableHeadCell>
                    <TableHeadCell align="center">分组</TableHeadCell>
                    <div className="text-right">操作</div>
                  </div>
                  <div className="divide-y divide-border">
                    {filteredItems.map((item) => (
                      <SortableKeyRow
                        key={item.id}
                        item={item}
                        dragEnabled={dragEnabled}
                        onEdit={handleEdit}
                        onDelete={handleDelete}
                        onTestConnectivity={handleTestConnectivity}
                        testing={testingKeyId === item.id}
                        onToggleEnabled={handleToggleEnabled}
                        monitor={monitorByKey.get(item.id) ?? null}
                        monitorStatus={monitorStatusByKey.get(item.id) ?? null}
                        monitoring={monitoringKeyId === item.id}
                        onToggleMonitoring={handleToggleMonitoring}
                        onOpenRoutingImpact={
                          onOpenRoutingDeepLink
                            ? (candidate) =>
                                onOpenRoutingDeepLink({
                                  kind: "station-key",
                                  stationKeyId: candidate.id,
                                  source: "key_pool",
                                })
                            : undefined
                        }
                      />
                    ))}
                  </div>
                </div>
              </div>
            </SortableContext>
            <DragOverlay dropAnimation={null}>
              {activeDragItem ? <KeyRowContent overlay item={activeDragItem} /> : null}
            </DragOverlay>
          </DndContext>
        </div>
      )}

      {(creatingKey || editingItem) && (
        <KeyEditDialog
          actionSaving={saving}
          groupOptions={groupOptionsForEdit}
          sourceItem={editingItem}
          mode={creatingKey ? "create" : "edit"}
          form={editForm}
          stations={stations}
          onClose={closeEditDialog}
          onFormChange={setEditForm}
          onSave={creatingKey ? handleCreateSave : handleEditSave}
          onStationChange={creatingKey ? handleCreateStationChange : undefined}
          renderCurrentGroupOption={currentGroupOption}
          renderGroupOptionLabel={groupOptionLabel}
          renderGroupTriggerLabel={groupTriggerLabel}
        />
      )}

      <KeyConnectivityTestDialog
        item={connectivityDialogItem}
        capabilities={connectivityCapabilities}
        result={connectivityTestResult}
        error={connectivityTestError}
        displayedResponseText={displayedResponseText}
        streamFallbackReason={connectivityStreamFallbackReason}
        progressLabel={connectivityProgressLabel}
        onDisplayedResponseTextChange={setDisplayedResponseText}
        testing={Boolean(connectivityDialogItem && testingKeyId === connectivityDialogItem.id)}
        onClose={closeConnectivityDialog}
        onTest={(model) => void handleRunConnectivityTest(model)}
      />

      <ConfirmDialog
        open={pendingDeleteItem !== null}
        title="删除密钥"
        description={`确定要删除密钥 "${pendingDeleteItem?.name ?? ""}" 吗？此操作无法撤销。`}
        confirming={saving}
        onCancel={() => setPendingDeleteItem(null)}
        onConfirm={() => void handleConfirmDelete()}
      />
    </PageScaffold>
  );
}

function currentGroupOption(sourceItem: KeyPoolItem | null, options: StationGroupOption[]) {
  if (!sourceItem?.groupBindingId || findMatchingGroupOption(keyPoolItemGroupRow(sourceItem), options)) {
    return [];
  }
  return [
    {
      value: sourceItem.groupBindingId,
      label: <StationGroupOptionLabel option={keyPoolItemGroupOption(sourceItem)} suffix="当前" />,
      triggerLabel: <StationGroupTriggerLabel option={keyPoolItemGroupOption(sourceItem)} suffix="当前" />,
    },
  ];
}

function groupOptionLabel(option: StationGroupOption) {
  return <StationGroupOptionLabel option={option} />;
}

function groupTriggerLabel(option: StationGroupOption) {
  return <StationGroupTriggerLabel option={option} />;
}

function keyPoolItemGroupOption(item: KeyPoolItem): StationGroupOption {
  return {
    value: item.groupBindingId ? `binding:${item.groupBindingId}` : item.groupIdHash ? `remote:${item.groupIdHash}` : `name:${item.groupName ?? ""}`,
    groupBindingId: item.groupBindingId,
    groupIdHash: item.groupIdHash,
    groupName: item.groupName ?? "当前绑定",
    rateMultiplier: item.rateMultiplier,
    inferredGroupCategory: "unknown",
    groupCategoryOverride: null,
    effectiveGroupCategory: "unknown",
    rateSource: item.rateSource,
    selectableForRemoteKey: Boolean(item.groupBindingId || item.groupIdHash),
  };
}

function keyPoolItemGroupRow(item: KeyPoolItem) {
  return {
    groupBindingId: item.groupBindingId,
    groupIdHash: item.groupIdHash,
    groupName: item.groupName ?? "",
  };
}

const selectClassName =
  "h-8 rounded-[12px] border border-border bg-surface px-3 text-sm text-foreground shadow-surface outline-none transition focus:border-ring focus:ring-2 focus:ring-ring/30";
