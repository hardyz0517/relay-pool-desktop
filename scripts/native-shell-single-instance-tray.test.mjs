import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const cargoToml = await readFile("src-tauri/Cargo.toml", "utf8");
const libSource = await readFile("src-tauri/src/lib.rs", "utf8");

assert.match(
  cargoToml,
  /tauri-plugin-single-instance\s*=/,
  "native shell should depend on Tauri's single-instance plugin",
);

assert.match(
  libSource,
  /tauri_plugin_single_instance::init/,
  "native shell should register the single-instance plugin",
);

assert.match(
  libSource,
  /\.get_webview_window\("main"\)[\s\S]{0,240}\.show\(\)[\s\S]{0,240}\.set_focus\(\)/,
  "opening a second instance should reveal and focus the existing main window",
);

assert.match(
  libSource,
  /TrayIconBuilder::with_id\("main-tray"\)/,
  "native shell should create a visible system tray icon",
);

assert.match(
  libSource,
  /enum\s+TrayBehavior[\s\S]*MinimizeToTray[\s\S]*CloseToTray[\s\S]*Disabled/,
  "native shell should model the persisted tray behavior modes",
);

assert.match(
  libSource,
  /fn\s+current_tray_behavior[\s\S]*try_state::<services::database::AppDatabase>[\s\S]*get_settings\(\)/,
  "native shell should read the latest persisted tray behavior before handling window events",
);

assert.match(
  libSource,
  /fn\s+hides_on_close[\s\S]*matches!\(self,\s*Self::CloseToTray\)/,
  "only close-to-tray mode should hide the window on close",
);

assert.match(
  libSource,
  /fn\s+hides_on_minimize[\s\S]*matches!\(self,\s*Self::MinimizeToTray\)/,
  "only minimize-to-tray mode should hide the window on minimize",
);

assert.match(
  libSource,
  /WindowEvent::CloseRequested[\s\S]*hides_on_close\(\)[\s\S]*\.prevent_close\(\)[\s\S]*\.hide\(\)/,
  "closing the main window should hide it only when close-to-tray is selected",
);

assert.match(
  libSource,
  /WindowEvent::CloseRequested\s*\{\s*api,\s*\.\.\s*\}\s*=>\s*\{[\s\S]*\.exit\(0\)/,
  "closing the main window should exit when tray close behavior is disabled or minimize-only",
);

assert.match(
  libSource,
  /WindowEvent::Resized[\s\S]*hides_on_minimize\(\)[\s\S]*\.is_minimized\(\)[\s\S]*\.hide\(\)/,
  "minimizing the main window should hide it only when minimize-to-tray is selected",
);

assert.match(
  libSource,
  /menu_id\.as_ref\(\)\s*==\s*"quit"[\s\S]{0,240}\.exit\(0\)/,
  "tray menu should include an explicit quit action",
);

console.log("native shell single-instance and tray behavior ok");
