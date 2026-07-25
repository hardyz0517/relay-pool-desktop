import fs from "node:fs";
import path from "node:path";
import {
  assert,
  readRequiredManifest,
  repoRoot,
  runMain,
} from "./lib.mjs";

const COMMANDS_PATH = "src-tauri/src/commands/mod.rs";
const MATRIX_PATH = "docs/superpowers/audits/architecture-scale-command-facade-matrix.json";

function source(relativePath) {
  const absolutePath = path.join(repoRoot, relativePath);
  assert(fs.existsSync(absolutePath), `missing source file: ${relativePath}`);
  return fs.readFileSync(absolutePath, "utf8");
}

function commandBlock(commandsSource, commandName) {
  const pattern = new RegExp(`pub\\s+async\\s+fn\\s+${commandName}\\s*\\(`);
  const match = pattern.exec(commandsSource);
  assert(match, `missing migrated command function: ${commandName}`);
  const start = match.index;
  const next = commandsSource.indexOf("\n#[tauri::command]", start + 1);
  return commandsSource.slice(start, next === -1 ? commandsSource.length : next);
}

function commandSignature(block, commandName) {
  const match = new RegExp(`pub\\s+async\\s+fn\\s+${commandName}\\s*\\(([\\s\\S]*?)\\)\\s*->`).exec(block);
  assert(match, `cannot parse command signature: ${commandName}`);
  return match[1];
}

function facadeStructBody(facadeSource, stateType) {
  const match = new RegExp(`struct\\s+${stateType}\\s*\\{([\\s\\S]*?)\\n\\}`).exec(facadeSource);
  assert(match, `missing facade struct: ${stateType}`);
  return match[1];
}

runMain(() => {
  const matrix = readRequiredManifest(MATRIX_PATH, ["schema_version", "facades"]);
  assert(matrix.schema_version === 1, "command facade matrix schema_version must be 1");
  assert(Array.isArray(matrix.facades) && matrix.facades.length > 0, "command facade matrix must declare facades");

  const commandsSource = source(COMMANDS_PATH);
  const seenCommands = new Set();

  for (const facade of matrix.facades) {
    assert(typeof facade.state_type === "string" && facade.state_type.trim(), "facade.state_type is required");
    assert(typeof facade.source_path === "string" && facade.source_path.trim(), `${facade.state_type}.source_path is required`);
    assert(Array.isArray(facade.commands) && facade.commands.length > 0, `${facade.state_type}.commands must be non-empty`);

    const facadeSource = source(facade.source_path);
    const structBody = facadeStructBody(facadeSource, facade.state_type);
    assert(!/^\s*pub(\([^)]*\))?\s+/m.test(structBody), `${facade.state_type} must not expose public fields`);

    const exposedMethods = new Set(
      [...facadeSource.matchAll(/pub\(crate\)\s+async\s+fn\s+([a-z][a-z0-9_]*)\s*\(/g)].map((match) => match[1]),
    );
    const expectedMethods = new Set(facade.commands.map((entry) => entry.use_case));
    for (const method of expectedMethods) {
      assert(exposedMethods.has(method), `${facade.state_type} is missing use-case method ${method}`);
    }
    for (const method of exposedMethods) {
      assert(expectedMethods.has(method), `${facade.state_type} exposes non-matrix method ${method}`);
    }

    for (const entry of facade.commands) {
      assert(typeof entry.command === "string" && /^[a-z][a-z0-9_]*$/.test(entry.command), "matrix command name must be snake_case");
      assert(!seenCommands.has(entry.command), `duplicate command facade matrix entry: ${entry.command}`);
      seenCommands.add(entry.command);

      const block = commandBlock(commandsSource, entry.command);
      const signature = commandSignature(block, entry.command);
      const stateMatches = [...signature.matchAll(/State\s*<\s*'_\s*,\s*([^>]+?)\s*>/g)].map((match) => match[1].trim());
      assert(stateMatches.length === 1, `${entry.command} must inject exactly one Tauri State`);
      assert(stateMatches[0] === facade.state_type, `${entry.command} must inject State<'_, ${facade.state_type}>`);
      assert(!signature.includes("AppServices"), `${entry.command} must not inject AppServices`);
      assert(!/\bservices\s*\./.test(block), `${entry.command} must call the facade, not AppServices services.*`);
    }
  }

  console.log(`Command state boundary gate passed (${seenCommands.size} migrated commands)`);
});
