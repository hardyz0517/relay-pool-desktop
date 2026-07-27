import {
  closestCenter,
  DndContext,
  DragOverlay,
  PointerSensor,
  useSensor,
  useSensors,
} from "@dnd-kit/core";
import {
  SortableContext,
  verticalListSortingStrategy,
} from "@dnd-kit/sortable";
import { Edit3, Plus, RefreshCw, X } from "lucide-react";
import { PageScaffold } from "@/components/shell/PageScaffold";
import { Button, ConfirmDialog, EmptyState, IconButton, SelectControl, StatusBadge } from "@/components/ui";
import { cn } from "@/lib/utils";
import {
  STATION_ISSUE_FILTER_OPTIONS,
  type StationIssueFilterValue,
} from "./stationAssetViewModels";
import { SortableStationAssetListRow, StationAssetListRow } from "./pages/stations/StationAssetRows";
import { DetailBody, StationDialogs } from "./pages/stations/StationDialogs";
import {
  useStationsPageController,
  type StationsPageControllerOptions,
} from "./useStationsPageController";

type StationsPageProps = StationsPageControllerOptions & {
  onAddProvider?: () => void;
};

export function StationsPage({ onAddProvider, ...controllerOptions }: StationsPageProps) {
  const {
    actionSaving,
    activeDialogStation,
    activeDragRow,
    attentionCount,
    changeEvents,
    closeDialog,
    closeDrawer,
    collectedBalanceCount,
    collectorRunsByStation,
    credentials,
    detailStation,
    dialogMode,
    drawerClosing,
    drawerStationId,
    drawerVisible,
    filteredStationAssetRows,
    filteredStationIds,
    form,
    groupBindingsByStation,
    handleConfirmDeleteKey,
    handleConfirmDeleteStation,
    handleDelete,
    handleDeleteKey,
    handleDragCancel,
    handleDragEnd,
    handleDragStart,
    handleManualAuthorization,
    handleOpenWebsite,
    handleRefreshBalance,
    handleRemoveLoginInfo,
    handleRunCollect,
    handleSaveKey,
    handleSubmit,
    issueFilter,
    keyCountLabel,
    keyDialogOpen,
    keyForm,
    loadError,
    loading,
    openCreate,
    openCreateKeyDialog,
    openDetail,
    openEdit,
    openEditKeyDialog,
    pendingDeleteKey,
    pendingDeleteStation,
    rateRecordsByStation,
    refreshExtras,
    saving,
    selectedStationId,
    setForm,
    setIssueFilter,
    setKeyDialogOpen,
    setKeyForm,
    setPendingDeleteKey,
    setPendingDeleteStation,
    snapshot,
    snapshots,
    stationAction,
    stationKeys,
    stations,
  } = useStationsPageController(controllerOptions);
  const sensors = useSensors(useSensor(PointerSensor, { activationConstraint: { distance: 8 } }));

  return (
    <PageScaffold
      title="中转站资产"
      status={
        <div className="flex min-w-0 flex-wrap items-center gap-1.5" aria-label="中转站资产状态">
          <StatusBadge tone="info" className="bg-surface-subtle text-muted-foreground">
            {`${filteredStationAssetRows.length} 站点`}
          </StatusBadge>
          <StatusBadge tone={collectedBalanceCount > 0 ? "healthy" : "disabled"}>
            {`${collectedBalanceCount} 已有余额`}
          </StatusBadge>
          <StatusBadge tone={attentionCount > 0 ? "warning" : "healthy"}>
            {`${attentionCount} 需关注`}
          </StatusBadge>
        </div>
      }
      actions={
        <div className="flex flex-wrap items-center justify-end gap-2">
          <SelectControl<StationIssueFilterValue>
            ariaLabel="筛选问题标签"
            className={stationAssetSelectClassName}
            value={issueFilter}
            options={STATION_ISSUE_FILTER_OPTIONS}
            onChange={setIssueFilter}
          />
          <Button onClick={onAddProvider ?? openCreate}>
            <Plus className="h-4 w-4" />
            添加供应商
          </Button>
        </div>
      }
    >
      <div className="grid min-w-0 gap-3">
        <div>
          {loading ? (
            <div className="rounded-[var(--surface-radius)] border border-border bg-surface px-4 py-5 text-sm text-muted-foreground shadow-[var(--surface-shadow)]">
              正在读取本地数据...
            </div>
          ) : loadError ? (
            <div className="rounded-[var(--surface-radius)] border border-danger-border bg-danger-surface px-4 py-5 text-sm text-danger-foreground shadow-[var(--surface-shadow)]">
              {loadError}
            </div>
          ) : stations.length === 0 ? (
            <EmptyState
              title="还没有中转站"
              description="添加一个站点开始管理登录账号和多把密钥。"
              action={<Button onClick={onAddProvider ?? openCreate}>添加供应商</Button>}
            />
          ) : filteredStationAssetRows.length === 0 ? (
            <EmptyState
              title="没有匹配的问题站点"
              description="切换问题筛选条件，或继续查看全部中转站资产。"
            />
          ) : (
            <DndContext
              sensors={sensors}
              collisionDetection={closestCenter}
              onDragStart={handleDragStart}
              onDragCancel={handleDragCancel}
              onDragEnd={handleDragEnd}
            >
              <SortableContext items={filteredStationIds} strategy={verticalListSortingStrategy}>
                <div className="space-y-2">
                  {filteredStationAssetRows.map((row) => (
                    <SortableStationAssetListRow
                      key={row.station.id}
                      actionDisabled={stationAction !== null}
                      active={row.station.id === selectedStationId}
                      loadingAction={stationAction?.stationId === row.station.id ? stationAction.action : null}
                      row={row}
                      onAuthorize={(station) => void handleManualAuthorization(station)}
                      onCollect={(station) => void handleRunCollect(station)}
                      onDelete={handleDelete}
                      onEdit={openEdit}
                      onOpen={openDetail}
                      onOpenWebsite={handleOpenWebsite}
                      onRefreshBalance={(station) => void handleRefreshBalance(station)}
                    />
                  ))}
                </div>
              </SortableContext>
              <DragOverlay dropAnimation={null}>
                {activeDragRow ? (
                  <StationAssetListRow
                    actionDisabled
                    active
                    loadingAction={stationAction?.stationId === activeDragRow.station.id ? stationAction.action : null}
                    overlay
                    row={activeDragRow}
                    onCollect={() => undefined}
                    onDelete={() => undefined}
                    onEdit={() => undefined}
                    onAuthorize={() => undefined}
                    onOpen={() => undefined}
                    onOpenWebsite={() => undefined}
                    onRefreshBalance={() => undefined}
                  />
                ) : null}
              </DragOverlay>
            </DndContext>
          )}
        </div>
      </div>

      {drawerStationId && detailStation && (
        <div
          className={cn(
            "fixed inset-0 z-40 bg-transparent transition-colors duration-200 ease-out",
            drawerVisible && !drawerClosing && "bg-scrim/20",
          )}
          onMouseDown={closeDrawer}
        >
          <div
            className={cn(
              "absolute inset-y-0 right-0 w-[min(560px,calc(100vw-72px))] border-l border-border bg-surface shadow-surface transition-transform duration-[220ms] ease-[cubic-bezier(0.22,1,0.36,1)] will-change-transform",
              drawerVisible && !drawerClosing ? "translate-x-0" : "translate-x-full",
            )}
            onMouseDown={(event) => event.stopPropagation()}
          >
            <div className="flex h-full min-h-0 flex-col">
              <div className="flex items-start justify-between gap-3 border-b border-border px-4 py-3">
                <div className="min-w-0">
                  <div className="truncate text-sm font-semibold text-foreground">{detailStation.name}</div>
                  <div className="truncate text-xs text-muted-foreground">{detailStation.websiteUrl}</div>
                </div>
                <IconButton
                  className="shrink-0 text-muted-foreground transition-colors hover:bg-muted hover:text-foreground active:bg-hover"
                  label="关闭详情抽屉"
                  onClick={closeDrawer}
                >
                  <X className="h-4 w-4" />
                </IconButton>
              </div>
              <div className="flex items-center justify-between gap-2 border-b border-border px-4 py-2">
                <div className="text-xs text-muted-foreground">{keyCountLabel}</div>
                <div className="flex gap-2">
                  <Button variant="outline" onClick={() => void refreshExtras(detailStation.id)} disabled={actionSaving}>
                    <RefreshCw className="h-4 w-4" />
                    刷新
                  </Button>
                  <Button variant="secondary" onClick={openCreateKeyDialog}>
                    <Plus className="h-4 w-4" />
                    新增密钥
                  </Button>
                  <Button variant="outline" onClick={() => openEdit(detailStation)}>
                    <Edit3 className="h-4 w-4" />
                    编辑
                  </Button>
                </div>
              </div>
              <div className="min-h-0 flex-1 overflow-auto">
                <DetailBody
                  activeDialogStation={detailStation}
                  changeEvents={changeEvents.filter((event) => event.stationId === detailStation.id)}
                  credentials={credentials}
                  keyCountLabel={keyCountLabel}
                  snapshot={snapshot}
                  snapshots={snapshots}
                  stationKeys={stationKeys}
                  groupBindings={groupBindingsByStation.get(detailStation.id) ?? []}
                  rateRecords={rateRecordsByStation.get(detailStation.id) ?? []}
                  collectorRuns={collectorRunsByStation.get(detailStation.id) ?? []}
                  onDeleteKey={handleDeleteKey}
                  onEditKey={openEditKeyDialog}
                />
              </div>
            </div>
          </div>
        </div>
      )}

      {(dialogMode || keyDialogOpen) && (
        <StationDialogs
          activeDialogStation={activeDialogStation}
          actionSaving={actionSaving}
          credentials={credentials}
          dialogMode={dialogMode}
          form={form}
          keyDialogOpen={keyDialogOpen}
          keyForm={keyForm}
          onChange={setForm}
          onClose={closeDialog}
          onKeyDialogOpenChange={setKeyDialogOpen}
          onKeyFormChange={setKeyForm}
          onKeySave={handleSaveKey}
          onRemoveLoginInfo={handleRemoveLoginInfo}
          onSubmit={handleSubmit}
          saving={saving}
        />
      )}
      <ConfirmDialog
        open={pendingDeleteStation !== null}
        title="删除中转站"
        description={`确定要删除站点 "${pendingDeleteStation?.name ?? ""}" 吗？此操作无法撤销。`}
        confirmLabel="删除"
        confirming={actionSaving}
        onCancel={() => setPendingDeleteStation(null)}
        onConfirm={() => void handleConfirmDeleteStation()}
      />
      <ConfirmDialog
        open={pendingDeleteKey !== null}
        title="删除密钥"
        description={`确定要删除密钥 "${pendingDeleteKey?.name ?? ""}" 吗？此操作无法撤销。`}
        confirming={actionSaving}
        onCancel={() => setPendingDeleteKey(null)}
        onConfirm={() => void handleConfirmDeleteKey()}
      />
    </PageScaffold>
  );
}

const stationAssetSelectClassName =
  "h-8 min-w-[148px] rounded-[12px] border border-border bg-surface px-3 text-sm text-foreground shadow-surface outline-none transition focus:border-ring focus:ring-2 focus:ring-ring/30";
