import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const read = (path) => readFile(path, "utf8").catch(() => "");
const provider = await read("src/features/updater/UpdaterProvider.tsx");
const dialog = await read("src/features/updater/UpdateDialog.tsx");
const main = await read("src/main.tsx");
const settings = await read("src/features/settings/SettingsPage.tsx");

assert.ok(main.includes("UpdaterProvider"), "global updater provider must be mounted");
assert.ok(provider.includes("setTimeout") && provider.includes("5_000"), "startup check must be delayed");
assert.ok(provider.includes("downloadPendingUpdate"));
assert.ok(provider.includes("cleanupBeforeUpdate"));
assert.ok(provider.includes("installPendingUpdateAndRelaunch"));
assert.ok(provider.includes("setDialogOpen(false)"), "up-to-date retry must close stale dialog");
assert.ok(provider.includes("currentAppVersion()") && provider.includes(".catch(() => undefined)"), "version initialization errors must be handled");
assert.ok(!provider.includes("toast.loading"), "update checks should not show loading toast");
assert.ok(provider.includes('toast.success("已是最新")'), "up-to-date checks should use the exact success toast copy");
assert.ok(provider.includes("toast.error"), "failed update checks should use toast errors");
assert.ok(dialog.includes("立即更新") && dialog.includes("稍后更新"));
assert.ok(dialog.includes("正在进行的代理请求会被中断"), "dialog must disclose request interruption");
assert.ok(dialog.includes("downloadedBytes") && dialog.includes("totalBytes"));
assert.ok(!dialog.includes("取消下载"), "MVP must not promise download cancellation");
assert.ok(dialog.includes('state.phase === "checking"'), "checking dialog must not expose install actions");
assert.ok(settings.includes("关于"));
assert.ok(settings.includes("检查更新"));
assert.ok(settings.includes("currentVersion"));
assert.ok(settings.includes("useUpdater"), "Settings must consume the shared updater controller");

console.log("updater UI contract checks passed");
