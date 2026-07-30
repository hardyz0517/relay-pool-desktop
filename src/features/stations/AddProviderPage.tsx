import { ArrowLeft, Check } from "lucide-react";
import { PageScaffold } from "@/components/shell/PageScaffold";
import { Button, ConfirmDialog, IconButton, PageForm } from "@/components/ui";
import { CreateRemoteKeyDialog } from "./components/CreateRemoteKeyDialog";
import {
  ProviderConnectionSection,
  ProviderGroupsSection,
  ProviderKeysSection,
  ProviderOptionsSection,
  ProviderPresetSection,
} from "./pages/add-provider/AddProviderSections";
import {
  useAddProviderPageController,
  type AddProviderPageControllerOptions,
} from "./useAddProviderPageController";

type AddProviderPageProps = AddProviderPageControllerOptions;

export function AddProviderPage(props: AddProviderPageProps) {
  const {
    activeStationId,
    applyPreset,
    cancelDeleteImportedLocalKey,
    cancelDeleteRemoteKey,
    closeCreateRemoteDialog,
    closeDiscardConfirm,
    commonLoginOptions,
    confirmDiscardChanges,
    confirmDeleteImportedLocalKey,
    confirmDeleteRemoteKey,
    connectionTest,
    createRemoteDisabled,
    createRemoteOpen,
    currentCreditPerCny,
    developerModeEnabled,
    discardConfirmOpen,
    editableGroupOptions,
    editing,
    error,
    form,
    groupRows,
    handleAddGroup,
    handleAddLocalKey,
    handleBindRemoteKey,
    handleCommonEmailSelect,
    handleCommonPasswordSelect,
    handleCopyWebsiteUrl,
    handleCreateRemoteKey,
    handleGroupRowsChange,
    handleImportRemoteKey,
    handleOpenCreateRemoteKey,
    handleUnbindRemoteKey,
    handleScanRemoteKeys,
    handleStartManualAuthorization,
    handleStationTypeChange,
    handleSubmit,
    handleSyncRemoteGroups,
    handleTestConnection,
    keyRows,
    importedLocalKeyPendingDelete,
    loading,
    passwordProfileLoading,
    providerDraftId,
    localStationKeys,
    remoteCapability,
    remoteCapabilityError,
    remoteCapabilityUnavailableReason,
    remoteCreatedLocalKeyIds,
    remoteDiscoveryReason,
    remoteGroupOptions,
    remoteKeys,
    remoteKeyPendingDelete,
    remoteListError,
    remoteLoading,
    remoteUnsupportedReason,
    requestDeleteRemoteKey,
    requestDeleteImportedLocalKey,
    requestExit,
    resetConnectionTest,
    saving,
    scanRemoteDisabled,
    setForm,
    setKeyRows,
    startingAuthorization,
    testingConnection,
  } = useAddProviderPageController(props);

  return (
    <PageScaffold
      title={editing ? "编辑供应商" : "添加新供应商"}
      stickyHeader
      backAction={
        <IconButton label="返回中转站" onClick={requestExit}>
          <ArrowLeft className="h-4 w-4" />
        </IconButton>
      }
    >
      <PageForm
        className="w-full"
        onSubmit={handleSubmit}
        footer={
          <>
            <Button variant="secondary" onClick={requestExit} disabled={saving}>
              取消
            </Button>
            <Button type="submit" disabled={saving || loading}>
              <Check className="h-4 w-4" />
              {saving ? "保存中" : editing ? "保存修改" : "添加供应商"}
            </Button>
          </>
        }
      >
        <section className="grid gap-[var(--shell-page-gap)]">
          <div className="grid gap-[var(--shell-page-gap)]">
            {!editing && <ProviderPresetSection presetId={form.presetId} onApplyPreset={applyPreset} />}

            <ProviderConnectionSection
              commonLoginEmails={commonLoginOptions.emails}
              commonLoginPasswords={commonLoginOptions.passwords}
              connectionTest={connectionTest}
              editing={editing}
              error={error}
              form={form}
              loading={loading}
              passwordProfileLoading={passwordProfileLoading}
              saving={saving}
              startingAuthorization={startingAuthorization}
              testingConnection={testingConnection}
              onConnectionTestReset={resetConnectionTest}
              onCommonEmailSelect={handleCommonEmailSelect}
              onCommonPasswordSelect={(profileId) => void handleCommonPasswordSelect(profileId)}
              onCopyWebsiteUrl={handleCopyWebsiteUrl}
              onFormChange={setForm}
              onStartManualAuthorization={handleStartManualAuthorization}
              onStationTypeChange={handleStationTypeChange}
              onTestConnection={handleTestConnection}
            />

            <ProviderGroupsSection
              developerModeEnabled={developerModeEnabled}
              disabled={saving || loading}
              remoteCapabilityUnavailableReason={remoteCapabilityUnavailableReason}
              remoteLoading={remoteLoading}
              rows={groupRows}
              scanRemoteDisabled={scanRemoteDisabled}
              onAddGroup={handleAddGroup}
              onRowsChange={handleGroupRowsChange}
              onSyncRemoteGroups={() => void handleSyncRemoteGroups()}
            />

            <ProviderKeysSection
              activeStationId={activeStationId}
              providerDraftId={providerDraftId}
              createRemoteDisabled={createRemoteDisabled}
              currentCreditPerCny={currentCreditPerCny}
              disabled={saving || loading}
              groupOptions={editableGroupOptions}
              localKeyIdsCreatedByRemote={remoteCreatedLocalKeyIds}
              localKeys={localStationKeys}
              remoteCapability={remoteCapability}
              remoteCapabilityError={remoteCapabilityError}
              remoteCapabilityUnavailableReason={remoteCapabilityUnavailableReason}
              remoteDiscoveryReason={remoteDiscoveryReason}
              remoteKeys={remoteKeys}
              remoteListError={remoteListError}
              remoteLoading={remoteLoading}
              remoteUnsupportedReason={remoteUnsupportedReason}
              rows={keyRows}
              scanRemoteDisabled={scanRemoteDisabled}
              onAddLocalKey={handleAddLocalKey}
              onBindRemoteKey={(remoteKeyId, stationKeyId) =>
                void handleBindRemoteKey(remoteKeyId, stationKeyId)
              }
              onDeleteImportedLocalKey={requestDeleteImportedLocalKey}
              onDeleteRemoteKey={requestDeleteRemoteKey}
              onImportRemoteKey={(remoteKey) => void handleImportRemoteKey(remoteKey)}
              onOpenCreateRemoteKey={() => void handleOpenCreateRemoteKey()}
              onRowsChange={setKeyRows}
              onScanRemoteKeys={() => void handleScanRemoteKeys()}
              onUnbindRemoteKey={(remoteKey) => void handleUnbindRemoteKey(remoteKey)}
            />
          </div>

          <aside className="grid content-start gap-[var(--shell-page-gap)]">
            <ProviderOptionsSection form={form} onFormChange={setForm} />
          </aside>
        </section>
      </PageForm>
      <CreateRemoteKeyDialog
        groups={remoteGroupOptions}
        open={createRemoteOpen}
        saving={remoteLoading}
        onClose={closeCreateRemoteDialog}
        onSubmit={handleCreateRemoteKey}
      />
      <ConfirmDialog
        open={Boolean(importedLocalKeyPendingDelete)}
        title="从 Key 池移除本地 Key？"
        description={`将删除由 ${importedLocalKeyPendingDelete?.remoteKey.remoteKeyName?.trim() || "这条远端记录"} 导入的本地 Station Key；如果仍有关联，也会一并解除。远端 Key 不会被删除。`}
        confirmLabel="删除本地 Key"
        cancelLabel="取消"
        confirming={remoteLoading}
        onCancel={cancelDeleteImportedLocalKey}
        onConfirm={() => void confirmDeleteImportedLocalKey()}
      />
      <ConfirmDialog
        open={Boolean(remoteKeyPendingDelete)}
        title="删除远端 Key？"
        description={
          remoteKeyPendingDelete?.matchedStationKeyId
            ? `将从远端删除 ${remoteKeyPendingDelete.remoteKeyName?.trim() || "这把 Key"}。已关联的本地 Station Key 会保留；只有远端对账确认 Key 已消失后，操作才会完成。`
            : `将从远端删除 ${remoteKeyPendingDelete?.remoteKeyName?.trim() || "这把 Key"}。只有远端对账确认 Key 已消失后，操作才会完成。`
        }
        confirmLabel="删除远端 Key"
        cancelLabel="取消"
        confirming={remoteLoading}
        onCancel={cancelDeleteRemoteKey}
        onConfirm={() => void confirmDeleteRemoteKey()}
      />
      <ConfirmDialog
        open={discardConfirmOpen}
        title="放弃未保存修改？"
        description={editing ? "当前供应商修改还没有保存，退出后这些修改会丢失。" : "当前新增供应商还没有保存，退出后这些修改会丢失。"}
        confirmLabel="放弃修改"
        cancelLabel="继续编辑"
        onCancel={closeDiscardConfirm}
        onConfirm={confirmDiscardChanges}
      />
    </PageScaffold>
  );
}
