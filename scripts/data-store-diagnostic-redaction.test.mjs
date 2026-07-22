import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const diagnosticSource = await readFile("src-tauri/src/services/data_store/diagnostic.rs", "utf8");
const commandsSource = await readFile("src-tauri/src/commands/mod.rs", "utf8");
const permissionSource = await readFile("src-tauri/permissions/main-window.toml", "utf8");

assert.ok(
    diagnosticSource.includes("DataStoreDiagnosticReport") &&
    diagnosticSource.includes("DiagnosticDecision") &&
    diagnosticSource.includes("diagnostic_decision") &&
    diagnosticSource.includes("anonymous_id") &&
    !diagnosticSource.includes("pub path") &&
    !diagnosticSource.includes("pub decision: StartupDecision") &&
    diagnosticSource.includes("diagnostic_report_redacts_paths_names_urls_keys_cookies_and_secret_material"),
  "data-store diagnostics should use anonymous projections and include redaction regression tests",
);

for (const command of [
  "refresh_data_store_candidates",
  "locate_data_store_candidate",
  "create_new_data_store",
  "open_data_store_backup_dir",
  "export_data_store_diagnostic",
]) {
  const commandDeclaration = new RegExp(`pub\\s+(?:async\\s+)?fn\\s+${command}\\b`);
  assert.match(commandsSource, commandDeclaration, `commands should expose ${command}`);
  assert.ok(permissionSource.includes(command), `main-window ACL should grant ${command}`);
}
