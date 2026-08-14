import assert from "node:assert/strict";
import { existsSync, readFileSync, readdirSync } from "node:fs";
import path from "node:path";

const args = process.argv.slice(2);
const rootArgIndex = args.indexOf("--root");
const root = path.resolve(rootArgIndex >= 0 ? args[rootArgIndex + 1] : process.cwd());
const manifestPath = path.join(
  root,
  "docs",
  "audits",
  "routing-operational-boundary-manifest.json",
);

const manifest = readJson(manifestPath, {
  schema_version: 1,
  production_forbidden_symbols: [],
  temporary_allowed_exceptions: [],
  boundary_symbols: [],
});
const failures = [];
const registered = new Map(
  (manifest.production_forbidden_symbols ?? []).map((entry) => [
    normalize(entry.symbol ?? ""),
    {
      paths: new Set((entry.paths ?? []).map(normalize)),
      reason: entry.reason,
      deleteByTask: entry.delete_by_task,
    },
  ]),
);
const boundarySymbols = new Map(
  (manifest.boundary_symbols ?? []).map((entry) => [entry.id, entry]),
);
const credentialBearingRouteDtoSymbol = "credential-bearing route DTOs api_key/api_key_secret";

checkMonitoringDoesNotOwnRoutingCandidate();
checkRoutingKernelIsPure();
checkFrontendTruthIsRegistered();
checkCredentialBearingTypes();
checkTestOnlyProductionApis();
checkHierarchicalV1DoesNotReadLegacyWeights();
checkBoundarySymbolsAreRegistered();

if (failures.length > 0) {
  console.error(failures.join("\n"));
  process.exit(1);
}

console.log("routing operational architecture gate passed");

function readJson(file, fallback) {
  if (!existsSync(file)) return fallback;
  return JSON.parse(readFileSync(file, "utf8"));
}

function normalize(value) {
  return String(value).replaceAll("\\", "/").replace(/^\.\//, "");
}

function relative(file) {
  return normalize(path.relative(root, file));
}

function filesUnder(relativeDir, extensions = [".rs", ".ts", ".tsx", ".mjs"]) {
  const dir = path.join(root, ...relativeDir.split("/"));
  if (!existsSync(dir)) return [];
  const result = [];
  const pending = [dir];
  while (pending.length > 0) {
    const current = pending.pop();
    for (const entry of readdirSync(current, { withFileTypes: true })) {
      const full = path.join(current, entry.name);
      if (entry.isDirectory()) {
        pending.push(full);
      } else if (entry.isFile() && extensions.some((extension) => entry.name.endsWith(extension))) {
        result.push(full);
      }
    }
  }
  return result;
}

function stripTestModules(source) {
  return source.replaceAll(
    /#\[cfg\(test\)\][\s\S]*?(?=\n(?:pub|pub(?:\(crate\))?|mod|use|const|fn|struct|enum|impl)\b|$)/g,
    "",
  );
}

function isRegistered(symbol, file) {
  const entry = registered.get(normalize(symbol));
  if (!entry) return false;
  if (!entry.reason || !entry.deleteByTask) return false;
  const filePath = relative(file);
  return entry.paths.size === 0 || entry.paths.has(filePath);
}

function requireRegistration(symbol, file, detail) {
  if (!isRegistered(symbol, file)) {
    failures.push(`${relative(file)}: ${detail}; register ${symbol} with path, reason, and delete_by_task`);
  }
}

function checkMonitoringDoesNotOwnRoutingCandidate() {
  for (const file of [
    ...filesUnder("src-tauri/src/services/monitoring", [".rs"]),
    ...filesUnder("src-tauri/src/application/monitoring", [".rs"]),
  ]) {
    const source = stripTestModules(readFileSync(file, "utf8"));
    const leakedRoutingCandidate = source.match(
      /\b(CanonicalRoutingCandidate|RuntimeRoutingCandidate)\b|models::routing::(CanonicalRoutingCandidate|RuntimeRoutingCandidate)|routing::(CanonicalRoutingCandidate|RuntimeRoutingCandidate)/u,
    );
    if (leakedRoutingCandidate) {
      const candidateName =
        leakedRoutingCandidate[1] ?? leakedRoutingCandidate[2] ?? leakedRoutingCandidate[3];
      requireRegistration(
        `monitoring imports ${candidateName}`,
        file,
        "monitoring must not depend on routing candidate DTO",
      );
    }
  }
}

function checkRoutingKernelIsPure() {
  for (const file of filesUnder("src-tauri/src/application/routing_engine", [".rs"])) {
    const source = stripTestModules(readFileSync(file, "utf8"));
    const forbidden = [
      [/\bsqlx\b|sqlx::/u, "SQLx"],
      [/\breqwest\b|reqwest::/u, "Reqwest"],
      [/\bSecretManager\b|secret_manager|services::secrets/u, "SecretManager"],
      [/\btauri\b|tauri::|#\[tauri::/u, "Tauri"],
      [/crate::ipc::dto|ipc::dto/u, "IPC DTO"],
    ];
    for (const [pattern, label] of forbidden) {
      assert.doesNotMatch(source, pattern, `${relative(file)}: routing kernel must not import ${label}`);
    }
  }
}

function checkFrontendTruthIsRegistered() {
  const truthPatterns = [
    /\bfunction\s+firstMatchingPricingRule\b/u,
    /\bfunction\s+derivePricingGroupDisplayCandidates\b/u,
    /\bfunction\s+deriveStationGroupDisplayFacts\b/u,
    /\bexport\s+function\s+deriveStationGroupDisplayFacts\b/u,
    /\bauthoritative(?:Pricing|Capability|Group)Matcher\b/u,
  ];
  for (const file of filesUnder("src", [".ts", ".tsx"])) {
    if (/\.test\.[cm]?[jt]sx?$/u.test(file)) continue;
    const source = readFileSync(file, "utf8");
    if (truthPatterns.some((pattern) => pattern.test(source))) {
      requireRegistration(
        "frontend pricing/group matcher",
        file,
        "frontend must not own authoritative pricing/group/capability matching",
      );
    }
  }
}

function checkCredentialBearingTypes() {
  for (const file of filesUnder("src-tauri/src", [".rs"])) {
    const source = stripTestModules(readFileSync(file, "utf8"));
    const structs = source.matchAll(
      /#\[derive\((?<derive>[^\]]*)\)\]\s*(?:pub(?:\(crate\))?\s+)?struct\s+(?<name>\w+)\s*\{(?<body>[\s\S]*?)\n\}/gu,
    );
    for (const match of structs) {
      const derive = match.groups?.derive ?? "";
      const body = match.groups?.body ?? "";
      const derivesSerializableDebug = /\b(?:Serialize|Deserialize|Debug)\b/u.test(derive);
      const carriesRoutingCredential = /\b(?:api_key|api_key_secret)\b/u.test(body);
      if (!derivesSerializableDebug || !carriesRoutingCredential) continue;
      requireRegistration(
        credentialBearingRouteDtoSymbol,
        file,
        "credential-bearing type must not derive Serialize/Deserialize/Debug without a deletion owner",
      );
    }
  }
}

function checkTestOnlyProductionApis() {
  for (const file of filesUnder("src-tauri/src/application/routing_engine", [".rs"])) {
    const source = readFileSync(file, "utf8");
    const testOnlyFacade = /#\[cfg\(test\)\]\s*(?:pub(?:\(crate\))?\s+)?fn\s+(?:report_result|bind_session|try_acquire)\b/u.test(source);
    if (testOnlyFacade) {
      requireRegistration(
        "scheduler report_result/bind_session #[cfg(test)] production-equivalent facade",
        file,
        "production-equivalent scheduler API is test-only",
      );
    }
  }
}

function checkHierarchicalV1DoesNotReadLegacyWeights() {
  for (const file of filesUnder("src-tauri/src", [".rs"])) {
    if (
      [
        "src-tauri/src/ipc/dto/settings.rs",
        "src-tauri/src/models/settings.rs",
        "src-tauri/src/persistence/stores/settings_store.rs",
      ].includes(relative(file))
    ) {
      continue;
    }
    const source = stripTestModules(readFileSync(file, "utf8"));
    if (/hierarchical_v1/u.test(source) && /\b(?:weight|weights|score|DispatchAlgorithmSettings)\b/u.test(source)) {
      failures.push(`${relative(file)}: hierarchical_v1 must not read legacy weights or score path`);
    }
  }
}

function checkBoundarySymbolsAreRegistered() {
  const marker = /RPD_ROUTING_BOUNDARY:([A-Za-z0-9_.:-]+)/gu;
  for (const file of [
    ...filesUnder("src-tauri/src", [".rs"]),
    ...filesUnder("src", [".ts", ".tsx"]),
    ...filesUnder("scripts", [".mjs"]),
  ]) {
    const source = readFileSync(file, "utf8");
    for (const match of source.matchAll(marker)) {
      const id = match[1];
      const entry = boundarySymbols.get(id);
      if (!entry || !entry.owner || !entry.consumer || !entry.deletion_status) {
        failures.push(`${relative(file)}: boundary symbol ${id} is missing owner, consumer, or deletion_status`);
      }
    }
  }
}
