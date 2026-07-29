import fs from "node:fs";
import path from "node:path";
import {
  assert,
  assertOwnedExpiry,
  authoritativeStage,
  normalizePath,
  readJson,
  readRequiredManifest,
  repoRoot,
  runMain,
} from "./lib.mjs";

const GENERATED_REGISTRY_CANDIDATES = [
  "output/architecture-scale/command-registry.json",
  "src-tauri/generated/command-registry.json",
];

function commandsFromCompiledAcl() {
  const acl = readJson("src-tauri/gen/schemas/acl-manifests.json", "compiled Tauri ACL manifest");
  const permissions = acl["__app-acl__"]?.permissions;
  assert(permissions && typeof permissions === "object", "compiled ACL does not contain __app-acl__.permissions");
  const commands = new Set();
  for (const permission of Object.values(permissions)) {
    const allow = permission?.commands?.allow ?? [];
    for (const command of Array.isArray(allow) ? allow : String(allow).split(/\s+/)) {
      if (command) commands.add(command);
    }
  }
  return commands;
}

function normalizeCommandInventory(inventory) {
  const raw = inventory.inventories?.commands ?? inventory.inventories?.command ?? [];
  const commands = new Set();
  function collect(value) {
    if (typeof value === "string") {
      if (/^[a-z][a-z0-9_]*$/.test(value)) commands.add(value);
      return;
    }
    if (Array.isArray(value)) {
      for (const entry of value) collect(entry);
      return;
    }
    if (!value || typeof value !== "object") return;
    for (const key of ["name", "command", "command_name", "id"]) {
      const candidate = value[key];
      if (typeof candidate === "string" && /^[a-z][a-z0-9_]*$/.test(candidate)) commands.add(candidate);
    }
    for (const key of ["groups", "commands", "command_names", "items"]) {
      const nested = value[key];
      if (Array.isArray(nested)) collect(nested);
      else if (nested && typeof nested === "object") collect(Object.values(nested));
    }
  }
  collect(Array.isArray(raw) ? raw : Object.values(raw));
  assert(commands.size > 0, "command inventory contains no snake_case command names");
  return commands;
}

function requirePlaceholderException(boundary) {
  const entries = [...(boundary.temporary_exceptions ?? []), ...(boundary.temporary_edges ?? [])];
  const entry = entries.find((candidate) => candidate?.id === "compiled-command-registry-pending");
  assert(entry, "compiled registry is missing and boundary manifest has no compiled-command-registry-pending exception");
  assertOwnedExpiry(entry, "compiled-command-registry-pending", authoritativeStage(boundary, "boundary manifest"));
  assert(/task\s*-?\s*3/i.test(String(entry.owner)), "compiled registry exception owner must be Task 3");
  assert(/task\s*-?\s*3/i.test(String(entry.delete_shard)), "compiled registry exception delete_shard must be Task 3");
  assert(entry.expiry_stage === 1, "compiled registry exception must expire at Stage 1");
}

function loadRegistry(boundary) {
  for (const candidate of GENERATED_REGISTRY_CANDIDATES) {
    if (!fs.existsSync(path.join(repoRoot, candidate))) continue;
    const registry = readJson(candidate, "generated command registry");
    assert(registry.schema_version === 1, "generated command registry schema_version must be 1");
    assert(typeof registry.contract_hash === "string" && /^[a-f0-9]{64}$/.test(registry.contract_hash), "generated command registry requires a sha256 contract_hash");
    assert(Array.isArray(registry.commands), "generated command registry commands must be an array");
    assert(registry.evidence?.kind === "compiled-rust-registry", "command registry evidence must come from the compiled Rust registry");
    assert(/^[a-f0-9]{64}$/.test(registry.evidence.serialization_fixture_hash ?? ""), "compiled registry requires a serialization_fixture_hash");
    for (const [index, command] of registry.commands.entries()) {
      assert(typeof command.name === "string" && /^[a-z][a-z0-9_]*$/.test(command.name), `commands[${index}].name is invalid`);
      for (const key of ["input_schema_hash", "output_schema_hash", "error_schema_hash"]) {
        assert(/^[a-f0-9]{64}$/.test(command[key] ?? ""), `commands[${index}].${key} is required`);
      }
    }
    const bindingPath = path.join(repoRoot, "src/lib/bridge/generated.ts");
    assert(fs.existsSync(bindingPath), "generated command registry exists but generated TypeScript binding is missing");
    const binding = fs.readFileSync(bindingPath, "utf8");
    const bindingHash = binding.match(/canonical[-_ ]hash:\s*([a-f0-9]{64})/i)?.[1];
    assert(bindingHash === registry.contract_hash, "generated binding canonical hash differs from command registry");
    return { commands: new Set(registry.commands.map((entry) => entry.name)), mode: "generated" };
  }
  requirePlaceholderException(boundary);
  const inventory = readRequiredManifest("docs/superpowers/audits/architecture-scale-upgrade-inventory.json", [
    "schema_version",
    "source_revision",
    "inventories",
  ]);
  assert(inventory.schema_version === 1, "architecture inventory schema_version must be 1");
  const commandInventory = inventory.inventories?.commands ?? inventory.inventories?.command ?? [];
  const normalizeCollection = (value) => Array.isArray(value) ? value : value && typeof value === "object" ? Object.values(value) : [];
  const fixtures = [
    ...normalizeCollection(inventory.inventories?.command_serialization_fixtures),
    ...normalizeCollection(inventory.command_serialization_fixtures),
    ...(Array.isArray(commandInventory) ? commandInventory.flatMap((entry) => entry?.serialization_fixtures ?? (entry?.serialization_fixture ? [entry.serialization_fixture] : [])) : []),
  ].map((entry) => typeof entry === "string" ? entry : entry?.path ?? entry?.file ?? entry?.fixture_path).filter(Boolean);
  assert(fixtures.length > 0, "Stage 0 command placeholder requires registered serialization fixtures");
  for (const fixture of fixtures) assert(fs.existsSync(path.join(repoRoot, fixture)), `missing registered command serialization fixture: ${fixture}`);
  return { commands: normalizeCommandInventory(inventory), mode: "stage-0-placeholder" };
}

function setDifference(left, right) {
  return [...left].filter((value) => !right.has(value)).sort();
}

runMain(() => {
  const boundary = readRequiredManifest("docs/superpowers/audits/architecture-scale-boundary-manifest.json", ["current_stage", "command_state_allowlist", "temporary_edges"]);
  const registry = loadRegistry(boundary);
  const acl = commandsFromCompiledAcl();
  assert(registry.commands.size > 0, "command registry must not be empty");
  const unauthorized = setDifference(registry.commands, acl);
  const unregistered = setDifference(acl, registry.commands);
  assert(unauthorized.length === 0, `registered commands missing ACL authorization: ${unauthorized.join(", ")}`);
  assert(unregistered.length === 0, `ACL authorizes unregistered commands: ${unregistered.join(", ")}`);

  const allowlist = boundary.command_state_allowlist;
  assert(Array.isArray(allowlist), "command_state_allowlist must be an array");
  for (const [index, entry] of allowlist.entries()) {
    assert(entry && typeof entry === "object", `command_state_allowlist[${index}] must be an object`);
    assert(typeof entry.command === "string" && registry.commands.has(entry.command), `stale command state allowlist entry: ${normalizePath(String(entry.command))}`);
    assert(typeof entry.state_type === "string" && entry.state_type.trim(), `command_state_allowlist[${index}].state_type is required`);
  }
  console.log(`Command registry gate passed (${registry.commands.size} commands, ${registry.mode})`);
});
