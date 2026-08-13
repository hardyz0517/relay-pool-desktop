import path from "node:path";
import {
  assert,
  assertOwnedExpiry,
  authoritativeStage,
  listFiles,
  readJson,
  readRequiredManifest,
  repoRoot,
  runMain,
} from "./lib.mjs";

function debtEntries(manifest) {
  return [
    ...(manifest.temporary_entries ?? []),
    ...(manifest.temporary_exceptions ?? []),
    ...(manifest.known_risks ?? []),
    ...(manifest.risks ?? []),
  ];
}

function requireDebt(manifest, idPattern, description, currentStage) {
  const entry = debtEntries(manifest).find((candidate) => idPattern.test(String(candidate.id ?? candidate.risk ?? candidate.kind ?? candidate.concern ?? "")));
  assert(entry, `${description} requires an explicit security-manifest debt entry`);
  assertOwnedExpiry(entry, description, currentStage);
  return entry;
}

function permissionCommands() {
  const acl = readJson("src-tauri/gen/schemas/acl-manifests.json", "compiled Tauri ACL manifest");
  const permissions = acl["__app-acl__"]?.permissions;
  assert(permissions && typeof permissions === "object", "compiled ACL application permissions are missing");
  return new Map(Object.entries(permissions).map(([identifier, value]) => {
    const raw = value?.commands?.allow ?? [];
    return [identifier, new Set(Array.isArray(raw) ? raw : String(raw).split(/\s+/).filter(Boolean))];
  }));
}

function exactOriginValidatorImplemented(exactOrigin) {
  if (!exactOrigin || typeof exactOrigin !== "object") return false;
  if (/missing/i.test(String(exactOrigin.status ?? ""))) return false;
  const required = new Set(exactOrigin.required_bindings ?? []);
  return [
    "window_label",
    "station_id",
    "endpoint_revision",
    "exact_origin",
  ].every((binding) => required.has(binding));
}

runMain(() => {
  const manifest = readRequiredManifest("docs/audits/architecture-scale-tauri-security-manifest.json", [
    "schema_version",
    "production_config",
    "window_patterns",
    "command_permissions",
    "application_exact_origin_validator",
    "demo_entry_reachability",
  ]);
  const boundary = readRequiredManifest("docs/audits/architecture-scale-boundary-manifest.json", ["current_stage"]);
  const currentStage = authoritativeStage(boundary, "boundary manifest");
  assert(manifest.schema_version === 1, "Tauri security manifest schema_version must be 1");
  const productionConfigPath = manifest.production_config.path ?? manifest.production_config;
  assert(typeof productionConfigPath === "string", "production_config.path is required");
  const config = readJson(productionConfigPath, "production Tauri config");
  const csp = config.app?.security?.csp;
  assert(typeof csp === "string" && csp.trim(), "production CSP must be a non-empty string");
  assert(/\bscript-src\s+'self'(?:;|$)/.test(csp), "production CSP must pin script-src to 'self'");
  assert(!/\bunsafe-eval\b/.test(csp), "production CSP must not allow unsafe-eval");
  assert(!/\bscript-src[^;]*(?:https?:|data:|blob:|\*)/.test(csp), "production CSP must not allow remote script sources");
  const exactOrigin = manifest.application_exact_origin_validator;
  assert(exactOrigin && typeof exactOrigin === "object", "application_exact_origin_validator must be an object");
  assert(typeof exactOrigin.owner === "string" && exactOrigin.owner.trim(), "application exact-origin validator owner is required");
  assert(typeof (exactOrigin.symbol ?? exactOrigin.path) === "string", "application exact-origin validator symbol/path is required");

  const capabilityFiles = listFiles(path.join(repoRoot, "src-tauri/capabilities"), (file) => file.endsWith(".json"));
  assert(capabilityFiles.length > 0, "no Tauri capability manifests found");
  const capabilities = capabilityFiles.map((file) => readJson(path.relative(repoRoot, file), "Tauri capability"));
  const labels = new Set();
  for (const capability of capabilities) {
    assert(typeof capability.identifier === "string", "capability identifier is required");
    assert(Array.isArray(capability.windows) && capability.windows.length > 0, `${capability.identifier} must scope explicit windows`);
    for (const label of capability.windows) labels.add(label);
    const urls = capability.remote?.urls ?? [];
    if (urls.some((url) => /^(?:https?|\*):\/\/\*$/.test(url))) {
      if (!exactOriginValidatorImplemented(exactOrigin)) {
        requireDebt(manifest, /capture.*remote|remote.*shell|wildcard.*url/i, `${capability.identifier} wildcard remote URL`, currentStage);
      }
    }
  }
  if (/missing/i.test(String(exactOrigin.status ?? ""))) requireDebt(manifest, /exact.*origin|origin.*validator/i, "missing exact-origin validator", currentStage);

  const compiled = permissionCommands();
  for (const capability of capabilities) {
    for (const permission of capability.permissions ?? []) {
      if (permission.includes(":")) continue;
      assert(compiled.has(permission), `${capability.identifier} references unknown application permission '${permission}'`);
    }
  }
  const capture = capabilities.find((capability) => capability.identifier === "capture");
  assert(capture, "capture capability is required");
  assert(capture.local === false, "capture capability must not grant local windows");
  assert(capture.windows.every((label) => label === "capture-*"), "capture capability must only match capture-* windows");
  const captureCommands = new Set((capture.permissions ?? []).flatMap((permission) => [...(compiled.get(permission) ?? [])]));
  const approvedCapture = new Set([
    "record_capture_event",
    "finish_web_authorization_session",
    "finish_provider_draft_authorization_session",
  ]);
  assert([...captureCommands].every((command) => approvedCapture.has(command)), `capture capability reaches main commands: ${[...captureCommands].filter((command) => !approvedCapture.has(command)).join(", ")}`);

  assert(manifest.demo_entry_reachability === "unreachable" || manifest.demo_entry_reachability?.production === false, "production build must not reach demo entry");
  assert(labels.has("main") && labels.has("capture-*"), "security manifest must cover main and capture windows");
  console.log(`Tauri security gate passed (${capabilities.length} capabilities)`);
});
