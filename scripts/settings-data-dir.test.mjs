import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const settingsPageSource = await readFile("src/features/settings/SettingsPage.tsx", "utf8");
const settingsApiSource = await readFile("src/lib/api/settings.ts", "utf8");
const settingsTypesSource = await readFile("src/lib/types/settings.ts", "utf8");
const tauriCommandsSource = await readFile("src-tauri/src/commands/mod.rs", "utf8");
const tauriLibSource = await readFile("src-tauri/src/lib.rs", "utf8");
const databaseSource = await readFile("src-tauri/src/services/database.rs", "utf8");
const cargoTomlSource = await readFile("src-tauri/Cargo.toml", "utf8");

assert.ok(
  !settingsPageSource.includes("默认列表接口只返回脱敏值和存在状态"),
  "settings data/security section should remove the old cyan data-safety note",
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
    settingsPageSource.includes("restartRequired") &&
    settingsPageSource.includes("重启后使用新的数据目录"),
  "settings page should expose a choose-data-directory action and tell the user it takes effect after restart",
);

assert.ok(
  settingsTypesSource.includes("pendingDataDir: string | null") &&
    settingsTypesSource.includes("dataDirChangeRequiresRestart: boolean"),
  "AppSettings should expose the pending data directory and restart flag",
);

assert.ok(
  settingsApiSource.includes("chooseDataDir") &&
    settingsApiSource.includes('invoke<AppSettings>("choose_data_dir"') &&
    settingsApiSource.includes("normalizeSettings"),
  "settings API should expose a desktop command for choosing a data directory",
);

assert.ok(
  tauriCommandsSource.includes("pub fn choose_data_dir") &&
    tauriCommandsSource.includes("rfd::FileDialog::new()") &&
    tauriCommandsSource.includes("database.set_pending_data_dir"),
  "Tauri commands should choose a folder and persist it as the pending data directory",
);

assert.ok(
  tauriLibSource.includes("commands::choose_data_dir"),
  "Tauri command handler should register choose_data_dir",
);

assert.ok(
  databaseSource.includes("relay-pool-data-dir.json") &&
    databaseSource.includes("configured_data_dir") &&
    databaseSource.includes("set_pending_data_dir") &&
    databaseSource.includes("data_dir_change_requires_restart"),
  "database service should load an external data-dir config before opening SQLite and report pending restart state",
);

assert.ok(
  cargoTomlSource.includes("rfd = "),
  "desktop backend should depend on rfd for native folder picking",
);
