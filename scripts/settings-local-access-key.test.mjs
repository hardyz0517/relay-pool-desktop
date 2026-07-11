import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const settingsPageSource = await readFile("src/features/settings/SettingsPage.tsx", "utf8");
const settingsApiSource = await readFile("src/lib/api/settings.ts", "utf8");
const tauriCommandsSource = await readFile("src-tauri/src/commands/mod.rs", "utf8");
const tauriLibSource = await readFile("src-tauri/src/lib.rs", "utf8");
const databaseSource = await readFile("src-tauri/src/services/database.rs", "utf8");

assert.ok(
  settingsPageSource.includes("随机生成") &&
    settingsPageSource.includes("generateLocalAccessKey") &&
    settingsPageSource.includes("handleLocalAccessKeyBlur"),
  "settings page should let users generate a local access key and autosave it on blur",
);

assert.ok(
  settingsPageSource.includes("local-access-key-field") &&
    settingsPageSource.includes("w-[176px]") &&
    settingsPageSource.includes("<span className=\"sr-only\">随机生成</span>") &&
    !settingsPageSource.includes("flex-1 rounded-[var(--surface-radius)]"),
  "local access key display and edit field should keep a stable width without forcing action buttons to wrap",
);

assert.ok(
  !settingsPageSource.includes(">编辑</Button>") &&
    !settingsPageSource.includes("编辑密钥"),
  "local access key editing should be click-to-edit, not an explicit edit button",
);

assert.ok(
  settingsApiSource.includes("updateLocalAccessKey") &&
    settingsApiSource.includes('invoke<AppSettings>("update_local_access_key"'),
  "settings API should expose an updateLocalAccessKey command returning normalized settings",
);

assert.ok(
  tauriCommandsSource.includes("pub fn update_local_access_key") &&
    tauriCommandsSource.includes("database.update_local_access_key"),
  "Tauri commands should expose update_local_access_key",
);

assert.ok(
  tauriLibSource.includes("commands::update_local_access_key"),
  "Tauri command handler should register update_local_access_key",
);

assert.ok(
  databaseSource.includes("pub fn update_local_access_key") &&
    databaseSource.includes('upsert_setting(&connection, "local_key"') &&
    databaseSource.includes("本地访问密钥不能为空"),
  "database service should validate and persist the local access key setting",
);
