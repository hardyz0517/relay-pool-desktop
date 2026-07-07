import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const addProviderSource = await readFile("src/features/stations/AddProviderPage.tsx", "utf8");
const editorSource = await readFile(
  "src/features/stations/components/StationKeyRowsEditor.tsx",
  "utf8",
);
const groupEditorSource = await readFile(
  "src/features/stations/components/StationGroupRowsEditor.tsx",
  "utf8",
).catch(() => "");
const createRemoteKeyDialogSource = await readFile(
  "src/features/stations/components/CreateRemoteKeyDialog.tsx",
  "utf8",
);
const groupOptionViewModelSource = await readFile(
  "src/features/stations/groupOptionViewModels.ts",
  "utf8",
);
const databaseSource = await readFile("src-tauri/src/services/database.rs", "utf8");
const stationDetailViewModelSource = await readFile(
  "src/features/stations/stationDetailViewModels.ts",
  "utf8",
);

assert.ok(
  !addProviderSource.includes("璇峰～鍐欓粯璁ゅ瘑閽ユ垨鏈湴瀵嗛挜"),
  "supplier creation should not force a default key or local key before saving",
);

assert.ok(
  !addProviderSource.includes('label={editing ? "瀵嗛挜" : "榛樿瀵嗛挜"}'),
  "connection info should not keep a separate default-key field above the key editor",
);

assert.match(
  addProviderSource,
  /createRemoteStationKey\(\{\s*stationId: targetStationId,/,
  "remote key creation should use the current active station id after create-page autosave",
);

assert.ok(
  addProviderSource.includes("groupBindingsToDrafts"),
  "create/edit page should derive editable group rows from persisted collector group bindings and rates",
);

assert.ok(
  addProviderSource.includes("syncRowsWithGroupRateOptions"),
  "exchange-ratio changes should rebuild selected group multipliers in key rows",
);

assert.ok(
  addProviderSource.includes("StationGroupRowsEditor"),
  "create/edit supplier page should render an editable group rows editor",
);

assert.ok(
  /<SectionCard[\s\S]*?<StationGroupRowsEditor[\s\S]*?<\/SectionCard>[\s\S]*?<SectionCard[\s\S]*?<StationKeyRowsEditor/.test(addProviderSource),
  "group rows should live under a standalone second-level group section",
);

assert.ok(
  /<SectionCard[\s\S]*?<StationKeyRowsEditor[\s\S]*?<\/SectionCard>/.test(addProviderSource),
  "key rows should stay under a separate key section",
);

assert.ok(
  addProviderSource.includes("handleSyncRemoteGroups"),
  "create/edit supplier page should offer a one-click remote group sync action",
);

assert.ok(
  addProviderSource.includes('collectStationTask(targetStationId, "groups")'),
  "remote group sync should reuse the existing collector groups task instead of guessing groups from remote keys",
);

assert.ok(
  addProviderSource.includes("listGroupRateRecords(targetStationId)"),
  "remote group sync should load persisted group rate records after collector groups runs",
);

const syncGroupsBody = addProviderSource.match(/async function handleSyncRemoteGroups\(\) \{[\s\S]*?\n  \}/)?.[0] ?? "";
assert.ok(
  !syncGroupsBody.includes("scanRemoteStationKeys"),
  "remote group sync must not derive group rows from remote key scanning",
);

assert.ok(
  !addProviderSource.includes("syncGroupRowsWithRemoteOptions"),
  "remote key scanning should not mutate editable group rows; groups and keys use separate collection paths",
);

assert.ok(
  !addProviderSource.includes('activeStationId ? remoteCapability?.canListRemoteKeys !== true : form.stationType !== "sub2api"'),
  "create supplier page should not keep remote scan buttons disabled only because the station has not been saved yet",
);

assert.ok(
  !/const createRemoteDisabled =[\s\S]*?;\n/.exec(addProviderSource)?.[0].includes("!activeStationId"),
  "create supplier page should allow remote-key creation to autosave the station before opening or submitting",
);

assert.ok(
  addProviderSource.includes("handleOpenCreateRemoteKey") &&
    addProviderSource.includes("const targetStationId = await ensureStationForRemoteKeyActions();"),
  "remote-key creation should autosave an unsaved create-page station before using remote APIs",
);

assert.ok(
  addProviderSource.includes("editableGroupOptions"),
  "key group dropdown options should come from editable group rows, not directly from remote discovery only",
);

assert.ok(
  addProviderSource.includes("mergeRemoteGroupOptions(editableGroupOptions, collectRemoteGroupOptions(remoteKeys))"),
  "create-remote-key group dropdown should include synced editable group rows, not only groups inferred from existing remote keys",
);

assert.ok(
  groupOptionViewModelSource.includes("const groupBindingId = row.groupBindingId?.trim() ?? \"\";") &&
    groupOptionViewModelSource.includes("if (groupBindingId) {") &&
    groupOptionViewModelSource.includes("option.groupBindingId === groupBindingId") &&
    groupOptionViewModelSource.includes("const groupIdHash = row.groupIdHash?.trim() ?? \"\";") &&
    groupOptionViewModelSource.includes("if (groupIdHash) {") &&
    groupOptionViewModelSource.includes("option.groupIdHash === groupIdHash") &&
    groupOptionViewModelSource.includes("const groupName = row.groupName.trim();"),
  "group option matching should prefer binding id, then remote group id hash, before falling back to group name",
);

assert.ok(
  addProviderSource.includes("function groupOptionMergeKey(") &&
    addProviderSource.includes("return `remote:${groupIdHash}:${groupName}`;") &&
    addProviderSource.includes("return `binding:${groupBindingId}`;") &&
    addProviderSource.includes("return `name:${groupName}`;") &&
    addProviderSource.includes("const groupKey = groupOptionMergeKey(group, groupName);") &&
    !addProviderSource.includes("const groupKey = stationGroupSelectValue({ ...group, groupName });"),
  "remote group merge should dedupe saved bindings and discovered groups by structural remote identity",
);

assert.ok(
  createRemoteKeyDialogSource.includes("rateMultiplier: number | null"),
  "create-remote-key group dropdown should receive group rate multipliers",
);

assert.ok(
  createRemoteKeyDialogSource.includes("groupBindingId: string | null") &&
    createRemoteKeyDialogSource.includes("groupBindingId: selectedGroup?.groupBindingId ?? null"),
  "create-remote-key dialog should submit the selected local group binding id",
);

assert.ok(
  addProviderSource.includes("groupBindingId: group.groupBindingId") &&
    addProviderSource.includes("groupBindingId: null"),
  "create-remote-key group options should preserve binding ids for synced local groups and leave remote-key-only groups unbound",
);

assert.ok(
  createRemoteKeyDialogSource.includes("RemoteGroupRateTag"),
  "create-remote-key group dropdown should show a compact multiplier tag beside each group name",
);

assert.ok(
  !/description:\s*group\.groupIdHash\s*\?/.test(createRemoteKeyDialogSource) &&
    !createRemoteKeyDialogSource.includes("无分组 ID"),
  "create-remote-key group dropdown should not display remote group ids",
);

assert.ok(
  addProviderSource.includes("upsertStationGroupBinding"),
  "create/edit supplier page should persist editable group rows as station group bindings",
);

assert.ok(
  addProviderSource.includes("setGroupRows(syncedGroupRows)"),
  "remote group sync should replace editable group rows with persisted collector facts",
);

assert.ok(
  addProviderSource.includes("dedupeGroupRows"),
  "loaded, synced, and saved group rows should be deduplicated before rendering",
);

assert.ok(
  addProviderSource.includes("disableMatchingGroupBindings"),
  "deleting an editable group row should disable every matching stored station group binding",
);

assert.ok(
  /async function saveGroupRows\(targetStationId: string, rows: StationGroupDraft\[\]\)[\s\S]*listStationGroupBindings\(targetStationId\)/.test(addProviderSource),
  "saving group rows should compare against persisted bindings so edit-page deletion affects detail-page storage",
);

assert.ok(
  /setGroupRows\(dedupeGroupRows\(/.test(addProviderSource),
  "edit-page loading should dedupe persisted group bindings before rendering",
);

assert.ok(
  /const \[groupBindings, groupRates/.test(addProviderSource) &&
    addProviderSource.includes("groupBindingsToDrafts(groupBindings, groupRates)"),
  "remote group sync should rebuild editable group rows from stored collector bindings and rates",
);

assert.ok(
  /groupBindingId:\s*row\.groupBindingId/.test(addProviderSource),
  "saved key rows should preserve the selected station group binding id",
);

assert.ok(
  addProviderSource.lastIndexOf("handleScanRemoteKeys") <
    addProviderSource.lastIndexOf("handleOpenCreateRemoteKey") &&
    addProviderSource.lastIndexOf("handleOpenCreateRemoteKey") <
      addProviderSource.lastIndexOf("handleAddLocalKey"),
  "local key creation should sit to the right of the remote key creation action",
);

assert.ok(
  editorSource.includes("groupOptions"),
  "key row editor should accept group options for dropdown selection",
);

assert.ok(
  editorSource.includes("noGroupValue") && editorSource.includes("不绑定分组"),
  "key row group selector should include a no-group option for custom multiplier",
);

assert.ok(
  editorSource.includes("<SelectControl"),
  "key row group should be selected with a dropdown instead of only free text",
);

assert.ok(
  editorSource.includes("groupIdHash"),
  "key row drafts should preserve the remote group identity hash when a group is selected",
);

assert.ok(
  editorSource.includes("groupBindingId"),
  "key row drafts should preserve the station group binding id when a group is selected",
);

assert.ok(
  editorSource.includes("gridTemplateColumns: keyRowsGridTemplate"),
  "key editor header and rows should share one grid template for alignment",
);

assert.ok(
  !groupEditorSource.includes("onSyncRemoteGroups"),
  "group rows editor should only render rows; section-level group actions belong to the surrounding section header",
);

assert.ok(
  !groupEditorSource.includes("RefreshCw"),
  "group rows editor should not render a nested sync toolbar",
);

assert.ok(
  groupEditorSource.includes("groupRowsGridTemplate"),
  "group editor header and rows should share one grid template for alignment",
);

assert.ok(
  databaseSource.includes("excluded.binding_status NOT IN ('missing', 'disabled')"),
  "explicitly disabling a group binding should override the protected bound status so detail-page rows disappear after edit deletion",
);

assert.ok(
  databaseSource.includes("disable_shadow_station_group_bindings"),
  "collector group writes should disable older same-name remote_scan shadow bindings",
);

assert.ok(
  stationDetailViewModelSource.includes("dedupeStationGroupBindings") &&
    stationDetailViewModelSource.includes("preferStationGroupBinding"),
  "station detail group rows should merge same-name duplicate bindings before rendering",
);
