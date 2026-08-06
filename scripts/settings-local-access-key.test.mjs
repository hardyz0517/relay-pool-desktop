import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const settingsPageSource = await readFile("src/features/settings/SettingsPage.tsx", "utf8");
const settingsApiSource = await readFile("src/lib/api/settings.ts", "utf8");
const tauriCommandsSource = await readFile("src-tauri/src/commands/settings.rs", "utf8");
const registrySource = await readFile("src-tauri/src/ipc/registry.rs", "utf8");
const settingsServiceSource = await readFile("src-tauri/src/application/settings.rs", "utf8");
const settingsStoreSource = await readFile(
  "src-tauri/src/persistence/stores/settings_store.rs",
  "utf8",
);

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
    settingsApiSource.includes("getActiveBackendClient().settings.updateLocalAccessKey(value)") &&
    !settingsApiSource.includes('invoke<AppSettings>("update_local_access_key"'),
  "settings API should expose updateLocalAccessKey through the generated IPC wrapper",
);

assert.ok(
  tauriCommandsSource.includes("pub async fn update_local_access_key") &&
    tauriCommandsSource.includes("UpdateLocalAccessKeyInputDto::parse(input)?") &&
    tauriCommandsSource.includes("SettingsStationsCommandFacade") &&
    tauriCommandsSource.includes(".update_local_access_key(input.value)"),
  "Tauri commands should expose typed update_local_access_key input",
);

assert.ok(
  registrySource.includes("update_local_access_key => $crate::commands::settings::update_local_access_key"),
  "Tauri command registry should register update_local_access_key",
);

assert.ok(
  settingsServiceSource.includes("pub async fn update_local_access_key") &&
    settingsServiceSource.includes("encrypt_local_access_key") &&
    settingsStoreSource.includes("upsert_local_access_key_secret") &&
    settingsStoreSource.includes("app_secret_bindings") &&
    settingsStoreSource.includes("UPDATE settings SET value = ''"),
  "settings application and store should validate local access keys but persist them through encrypted secret bindings",
);
