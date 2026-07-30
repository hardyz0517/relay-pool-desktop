import { existsSync, readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";

const root = process.cwd();
const monitoringRoot = join(root, "src-tauri", "src", "models", "monitoring");
const adapterRoot = join(root, "src-tauri", "src", "services", "monitoring", "adapters");
const profileRoot = join(root, "src-tauri", "src", "services", "monitoring", "profiles");
const monitoringRunnerService = join(root, "src-tauri", "src", "services", "monitoring", "runner.rs");

if (!existsSync(monitoringRoot)) {
  console.error("models/monitoring directory is missing");
  process.exit(1);
}

const forbidden = [
  { pattern: /\btauri\b|tauri::|#\[tauri::/u, label: "tauri" },
  { pattern: /\bsqlx\b|sqlx::/u, label: "sqlx" },
  { pattern: /\breqwest\b|reqwest::/u, label: "reqwest" },
  { pattern: /crate::persistence\b|super::super::persistence\b/u, label: "persistence" },
  { pattern: /crate::services\b|super::super::services\b/u, label: "services" },
];

const failures = [];

checkFiles(listRustFiles(monitoringRoot), forbidden);

const serviceBoundaryForbidden = [
  { pattern: /\btauri\b|tauri::|#\[tauri::/u, label: "tauri" },
  { pattern: /\bsqlx\b|sqlx::/u, label: "sqlx" },
  { pattern: /\breqwest\b|reqwest::/u, label: "reqwest" },
  { pattern: /crate::persistence\b|super::super::persistence\b/u, label: "persistence" },
  { pattern: /SecretManager|secret_manager/u, label: "secret manager" },
];

checkFiles(listExistingRustFiles(adapterRoot), serviceBoundaryForbidden);
checkFiles(listExistingRustFiles(profileRoot), serviceBoundaryForbidden);
checkProductionMonitorRunnerCutover(monitoringRunnerService);

if (failures.length > 0) {
  console.error(failures.join("\n"));
  process.exit(1);
}

console.log("monitoring architecture gate passed");

function listRustFiles(dir) {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) return listRustFiles(path);
    return entry.isFile() && entry.name.endsWith(".rs") ? [path] : [];
  });
}

function listExistingRustFiles(dir) {
  return existsSync(dir) ? listRustFiles(dir) : [];
}

function checkFiles(files, rules) {
  for (const file of files) {
    const source = readFileSync(file, "utf8");
    for (const rule of rules) {
      if (rule.pattern.test(source)) {
        failures.push(`${file}: forbidden ${rule.label} dependency`);
      }
    }
  }
}

function checkProductionMonitorRunnerCutover(file) {
  if (!existsSync(file)) return;
  const source = readFileSync(file, "utf8");
  const match = source.match(/pub\(crate\) fn compose_monitoring_runner[\s\S]*?\n\}\n/u);
  if (!match) {
    failures.push(`${file}: compose_monitoring_runner composition boundary is missing`);
    return;
  }
  const body = match[0];
  const forbiddenProductionCalls = [
    ["V2ChannelMonitor" + "RunnerAdapter", "legacy monitor runner adapter"],
    ["V2ChannelMonitor" + "ProbeAdapter", "legacy status-only probe adapter"],
    ["record_monitor" + "_run_v2", "legacy monitor run write helper"],
    ["record_probe" + "_outcome", "legacy channel_monitor_runs write path"],
    ["ChannelMonitor" + "RunnerPort", "legacy runner port"],
    ["ACTIVE_MONITOR" + "_RUNS", "legacy static active-run guard"],
    ["RUNNER_POLL" + "_INTERVAL", "legacy fixed polling interval"],
  ];
  for (const [needle, label] of forbiddenProductionCalls) {
    if (body.includes(needle)) {
      failures.push(`${file}: compose_monitoring_runner still composes ${label}`);
    }
  }
  if (!body.includes("MonitoringRunner::new(")) {
    failures.push(`${file}: compose_monitoring_runner must compose the V2 monitoring runner`);
  }
  if (!source.includes("BudgetedProbeTransport::new(")) {
    failures.push(`${file}: production monitor transport must reserve probe budget before network send`);
  }
  if (existsSync(join(root, "src-tauri", "src", "services", "channel_monitors", "mod.rs"))) {
    failures.push("legacy services/channel_monitors/mod.rs must not exist after production runner cutover");
  }
}
