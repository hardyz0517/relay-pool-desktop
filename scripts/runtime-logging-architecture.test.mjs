import fs from "node:fs";
import path from "node:path";

const root = process.cwd();
const sourceRoot = path.join(root, "src-tauri", "src");
const cargoManifest = fs.readFileSync(path.join(root, "src-tauri", "Cargo.toml"), "utf8");
const allowedFallbackFiles = new Set([
  path.join("observability", "runtime", "bootstrap.rs"),
  path.join("observability", "runtime", "crash.rs"),
]);
const forbidden = /(?:println!|eprintln!|tracing::(?:error|warn|info|debug)!)|error\s*=\s*[?%]error/;
const violations = [];
const removedLegacyPaths = [
  path.join("observability", "events.rs"),
  path.join("observability", "diagnostics.rs"),
  path.join("observability", "redaction.rs"),
];

function matchingClose(text, open, left, right) {
  let depth = 0;
  let quote = false;
  let escaped = false;
  for (let index = open; index < text.length; index += 1) {
    const character = text[index];
    if (quote) {
      if (escaped) escaped = false;
      else if (character === "\\") escaped = true;
      else if (character === '"') quote = false;
      continue;
    }
    if (character === '"') {
      quote = true;
      continue;
    }
    if (character === left) depth += 1;
    else if (character === right) {
      depth -= 1;
      if (depth === 0) return index;
    }
  }
  throw new Error(`unclosed ${left} at ${open}`);
}

function assertCommandContextBoundary(full, relative) {
  if (relative.endsWith(`${path.sep}runtime_context.rs`)) return;
  let cursor = 0;
  const marker = "#[tauri::command]";
  while (true) {
    const markerIndex = full.indexOf(marker, cursor);
    if (markerIndex < 0) return;
    const functionIndex = full.indexOf("pub async fn", markerIndex + marker.length);
    if (functionIndex < 0) {
      violations.push(`${relative}: command marker has no async function`);
      return;
    }
    const open = full.indexOf("(", functionIndex);
    const close = matchingClose(full, open, "(", ")");
    const signature = full.slice(functionIndex, close + 1);
    if (!signature.includes("RuntimeContextRegistry") || !signature.includes("runtime_context:")) {
      violations.push(`${relative}: command boundary is missing runtime context parameters`);
    }
    const nextMarker = full.indexOf(marker, close + 1);
    const body = full.slice(close, nextMarker < 0 ? full.length : nextMarker);
    if (!body.includes("in_command_scope_with_runtime_context(")) {
      violations.push(`${relative}: command does not enter the runtime context scope helper`);
    }
    cursor = close + 1;
  }
}

function walk(directory) {
  for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
    const full = path.join(directory, entry.name);
    if (entry.isDirectory()) walk(full);
    else if (entry.isFile() && entry.name.endsWith(".rs")) {
      const relative = path.relative(sourceRoot, full);
      if (relative.includes(`${path.sep}tests${path.sep}`) || relative.startsWith(`test_support${path.sep}`)) continue;
      const text = fs.readFileSync(full, "utf8");
      if (relative.startsWith(`commands${path.sep}`)) {
        assertCommandContextBoundary(text, relative);
      }
      if (!forbidden.test(text)) continue;
      if (!allowedFallbackFiles.has(relative)) violations.push(relative);
    }
  }
}

walk(sourceRoot);
for (const relative of removedLegacyPaths) {
  if (fs.existsSync(path.join(sourceRoot, relative))) {
    violations.push(`${relative} is a removed legacy observability path`);
  }
}
for (const relative of ["observability/correlation.rs", "observability/decision_trace.rs"]) {
  const text = fs.readFileSync(path.join(sourceRoot, relative), "utf8");
  for (const legacy of ["observability::events", "super::events", "observability::diagnostics", "observability::redaction"]) {
    if (text.includes(legacy)) violations.push(`${relative} still imports ${legacy}`);
  }
}
if (!fs.existsSync(path.join(sourceRoot, "observability", "runtime"))) {
  violations.push("missing observability/runtime owner");
}
const runtimeService = fs.readFileSync(path.join(sourceRoot, "observability", "runtime", "service.rs"), "utf8");
const runtimeBootstrap = fs.readFileSync(path.join(sourceRoot, "observability", "runtime", "bootstrap.rs"), "utf8");
const runtimeCatalog = fs.readFileSync(path.join(sourceRoot, "observability", "runtime", "catalog.rs"), "utf8");
if (!runtimeService.includes("pub(crate) fn record_descriptor(")) {
  violations.push("runtime service must expose descriptor-driven production emission");
}
if (/bootstrap::(?:emit|emit_rate_limited|record_failure)\s*\(\s*"/.test(walkedProductionSources())) {
  violations.push("production runtime producers must pass owner-local descriptors, not string event codes");
}
if (/pub\(crate\)\s+(?:const|static)\s+\w*EVENT_DESCRIPTORS/.test(runtimeCatalog)) {
  violations.push("runtime catalog must aggregate owner slices, not declare domain descriptors");
}
for (const legacyInference of ["fn level_for(", "fn outcome_for(", "fn component_for("]) {
  if (runtimeBootstrap.includes(legacyInference)) {
    violations.push(`runtime bootstrap still infers event metadata: ${legacyInference}`);
  }
}
for (const relative of ["lib.rs", path.join("commands", "runtime_diagnostics.rs")]) {
  const source = fs.readFileSync(path.join(sourceRoot, relative), "utf8");
  const productionSource = source.split("#[cfg(all(test, feature = \"tauri-test\"))]")[0];
  if (productionSource.includes("runtime_log.record(")) {
    violations.push(`${relative}: production runtime events must use record_descriptor`);
  }
}
if (!/desktop-runtime\s*=\s*\[[\s\S]*?tauri\/common-controls-v6[\s\S]*?\]/m.test(cargoManifest)) {
  violations.push("desktop-runtime must enable tauri/common-controls-v6 for packaged Windows dialogs");
}
if (/tauri-test\s*=\s*\[[\s\S]*?tauri\/common-controls-v6[\s\S]*?\]/m.test(cargoManifest)) {
  violations.push("tauri-test must not enable tauri/common-controls-v6 without a bundled Windows manifest");
}
if (violations.length) {
  console.error("runtime logging architecture violations:");
  for (const violation of violations) console.error(`- ${violation}`);
  process.exit(1);
}
console.log("runtime logging architecture check passed");

function walkedProductionSources() {
  const sources = [];
  function collect(directory) {
    for (const entry of fs.readdirSync(directory, { withFileTypes: true })) {
      const full = path.join(directory, entry.name);
      if (entry.isDirectory()) collect(full);
      else if (entry.isFile() && entry.name.endsWith(".rs")) {
        const relative = path.relative(sourceRoot, full);
        if (relative.includes(`${path.sep}tests${path.sep}`) || relative.startsWith(`test_support${path.sep}`)) continue;
        sources.push(fs.readFileSync(full, "utf8"));
      }
    }
  }
  collect(sourceRoot);
  return sources.join("\n");
}
