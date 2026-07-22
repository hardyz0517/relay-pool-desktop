import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const settingsPageSource = await readFile("src/features/settings/SettingsPage.tsx", "utf8");
const settingsApiSource = await readFile("src/lib/api/settings.ts", "utf8");
const settingsTypesSource = await readFile("src/lib/types/settings.ts", "utf8");
const tauriCommandsSource = await readFile("src-tauri/src/commands/mod.rs", "utf8");
const tauriLibSource = await readFile("src-tauri/src/lib.rs", "utf8");
const dataDirectoryServiceSource = await readFile(
  "src-tauri/src/application/data_directory.rs",
  "utf8",
);
const dataDirectoryPortSource = await readFile(
  "src-tauri/src/services/data_store/data_directory_port.rs",
  "utf8",
);
const settingsStoreSource = await readFile(
  "src-tauri/src/persistence/stores/settings_store.rs",
  "utf8",
);
const relocationSource = await readFile("src-tauri/src/services/data_store/relocation.rs", "utf8");
const cargoTomlSource = await readFile("src-tauri/Cargo.toml", "utf8");

assert.ok(
  !settingsPageSource.includes("默认列表接口只返回脱敏值和存在状态"),
  "settings data/security section should remove the old cyan data-safety note",
);

assert.ok(
  !settingsPageSource.includes("本地数据库不在仓库目录。"),
  "settings data-directory row should not show the static explanatory copy under the title",
);

for (const label of ["加密存储", "凭据迁移", "安全扫描"]) {
  assert.ok(
    !settingsPageSource.includes(`label="${label}"`),
    `settings data/security section should not show read-only status row: ${label}`,
  );
}

assert.ok(
  settingsPageSource.includes("选择位置") &&
    settingsPageSource.includes("handleChooseDataDir") &&
    settingsPageSource.includes("handleResetDataDir") &&
    settingsPageSource.includes('aria-label="选择数据目录位置"') &&
    settingsPageSource.includes('aria-label="恢复默认数据目录"') &&
    settingsPageSource.includes('className="data-dir-path-field') &&
    settingsPageSource.includes("restartRequired") &&
    settingsPageSource.includes("重启后使用新的数据目录"),
  "settings page should expose compact choose/reset data-directory actions and tell the user changes take effect after restart",
);

assert.ok(
  settingsTypesSource.includes("pendingDataDir: string | null") &&
    settingsTypesSource.includes("dataDirChangeRequiresRestart: boolean"),
  "AppSettings should expose the pending data directory and restart flag",
);

assert.ok(
  settingsApiSource.includes("chooseDataDir") &&
    settingsApiSource.includes('invoke<AppSettings>("choose_data_dir"') &&
    settingsApiSource.includes("resetDataDir") &&
    settingsApiSource.includes('invoke<AppSettings>("reset_data_dir"') &&
    settingsApiSource.includes("normalizeSettings"),
  "settings API should expose desktop commands for choosing and resetting the data directory",
);

assert.ok(
  tauriCommandsSource.includes("pub async fn choose_data_dir") &&
    tauriCommandsSource.includes("pub async fn reset_data_dir") &&
    tauriCommandsSource.includes("rfd::FileDialog::new()") &&
    tauriCommandsSource.includes(".data_directory") &&
    tauriCommandsSource.includes(".select_pending(data_dir)") &&
    tauriCommandsSource.includes(".reset_to_default()"),
  "Tauri commands should choose a folder or reset to the default data directory",
);

assert.ok(
  tauriLibSource.includes("commands::choose_data_dir") &&
    tauriLibSource.includes("commands::reset_data_dir"),
  "Tauri command handler should register choose_data_dir and reset_data_dir",
);

assert.ok(
  dataDirectoryServiceSource.includes("pub(crate) trait DataDirectoryPort") &&
    dataDirectoryServiceSource.includes("set_data_directory_projection") &&
    dataDirectoryPortSource.includes("write_relocation_intent") &&
    settingsStoreSource.includes("data_dir_change_requires_restart"),
  "data-directory application and persistence owners should expose pending selection and restart state",
);

assert.ok(
    relocationSource.includes("write_relocation_intent") &&
    relocationSource.includes("apply_trusted_relocation") &&
    relocationSource.includes("create_verified_backup_from_path") &&
    !relocationSource.includes("fs::copy(&source_db_path, &db_path)"),
  "data-directory changes should use v2 relocation intent plus SQLite backup and must not use raw fs::copy activation",
);

assert.ok(
  cargoTomlSource.includes("rfd = "),
  "desktop backend should depend on rfd for native folder picking",
);
