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
checkProductionStatusQueriesCutover();
checkLegacyRunFrontendIsolation();
checkLegacyRustFixtureIsolation();

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
  const match = source.match(/pub\(crate\) fn compose_monitoring_runner[\s\S]*?\r?\n\}\r?\n/u);
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

function checkLegacyRunFrontendIsolation() {
  const roots = [
    join(root, "src", "features"),
    join(root, "src", "lib", "api"),
    join(root, "src", "lib", "bridge"),
    join(root, "src", "lib", "queries"),
    join(root, "src", "lib", "query"),
  ];
  const allowed = new Set([
    join(root, "src", "lib", "bridge", "generated.ts"),
    join(root, "src", "lib", "bridge", "generated.test.ts"),
  ]);
  for (const dir of roots) {
    for (const file of listSourceFiles(dir)) {
      if (allowed.has(file) || file.endsWith(".test.ts") || file.endsWith(".test.tsx")) continue;
      const source = readFileSync(file, "utf8");
      if (source.includes("listChannelMonitorRuns") || source.includes("list_channel_monitor_runs")) {
        failures.push(`${file}: product frontend must use Monitoring V2 execution history`);
      }
    }
  }
}

function checkLegacyRustFixtureIsolation() {
  const moduleFiles = [
    [join(root, "src-tauri", "src", "services", "monitoring", "mod.rs"), ["runtime", "scheduler"]],
    [join(root, "src-tauri", "src", "application", "monitoring", "mod.rs"), ["retention"]],
  ];
  for (const [file, modules] of moduleFiles) {
    const source = readFileSync(file, "utf8");
    for (const moduleName of modules) {
      const declaration = new RegExp(`(?:pub(?:\\(crate\\))?\\s+)?mod\\s+${moduleName}\\s*;`, "u");
      if (declaration.test(source)) {
        failures.push(`${file}: legacy test fixture ${moduleName}.rs must not be in the production module tree`);
      }
    }
  }
}

function checkProductionStatusQueriesCutover() {
  const applicationQueries = join(root, "src-tauri", "src", "application", "monitoring", "queries.rs");
  const persistenceModule = join(root, "src-tauri", "src", "persistence", "stores", "monitoring", "mod.rs");
  const persistenceQueries = join(root, "src-tauri", "src", "persistence", "stores", "monitoring", "status_queries.rs");

  if (!existsSync(applicationQueries)) {
    failures.push(`${applicationQueries}: monitoring status read model query boundary is missing`);
    return;
  }
  if (!existsSync(persistenceQueries)) {
    failures.push(`${persistenceQueries}: production monitoring status query repository is missing`);
    return;
  }

  const applicationSource = stripRustCfgTestModule(readFileSync(applicationQueries, "utf8"));
  if (/\bsqlx\b|sqlx::|QueryBuilder|SqliteConnection/u.test(applicationSource)) {
    failures.push(`${applicationQueries}: application status read model must use persistence repository instead of SQLx`);
  }
  if (!applicationSource.includes("MonitoringStatusQueryRepository")) {
    failures.push(`${applicationQueries}: application status read model must compose MonitoringStatusQueryRepository`);
  }

  const moduleSource = readFileSync(persistenceModule, "utf8");
  if (!/(?:pub(?:\(crate\))?\s+)?mod\s+status_queries\s*;/u.test(moduleSource)) {
    failures.push(`${persistenceModule}: production monitoring store module must expose status_queries repository`);
  }

  const persistenceSource = readFileSync(persistenceQueries, "utf8");
  if (!persistenceSource.includes("pub(crate) struct MonitoringStatusQueryRepository")) {
    failures.push(`${persistenceQueries}: status_queries must define the production query repository`);
  }
  if (!/\bsqlx\b|sqlx::|QueryBuilder|SqliteConnection/u.test(persistenceSource)) {
    failures.push(`${persistenceQueries}: production query repository must own the monitoring SQL boundary`);
  }
}

function stripRustCfgTestModule(source) {
  return source.replace(/\r?\n#\[cfg\(test\)\]\s*mod\s+tests\s*\{[\s\S]*$/u, "\n");
}

function listSourceFiles(dir) {
  if (!existsSync(dir)) return [];
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) return listSourceFiles(path);
    return entry.isFile() && /\.(?:ts|tsx)$/u.test(entry.name) ? [path] : [];
  });
}
