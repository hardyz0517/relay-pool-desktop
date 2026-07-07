import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import ts from "typescript";

async function importTsModule(path) {
  const source = await readFile(path, "utf8");
  const output = ts.transpileModule(source, {
    compilerOptions: {
      module: ts.ModuleKind.ESNext,
      target: ts.ScriptTarget.ES2022,
      verbatimModuleSyntax: true,
    },
  }).outputText;
  const encoded = Buffer.from(output, "utf8").toString("base64");
  return import(`data:text/javascript;base64,${encoded}`);
}

const { groupVisualMetaFor } = await importTsModule("src/features/stations/groupVisualMeta.ts");

assert.equal(
  groupVisualMetaFor("codex-0号池", { platform: "openai" }).platform,
  "openai",
  "Sub2API platform field should classify codex groups as OpenAI without relying on the group name",
);
assert.equal(
  groupVisualMetaFor("claude-kiro高强", { platform: "anthropic" }).platform,
  "anthropic",
  "Sub2API platform field should classify Claude groups",
);
assert.equal(groupVisualMetaFor("Gemini", { platform: "gemini" }).platform, "gemini");
assert.equal(groupVisualMetaFor("grok-super", { platform: "grok" }).platform, "grok");
assert.equal(
  groupVisualMetaFor("gpt-pro兜底").platform,
  "generic",
  "group names alone must not classify visual platform without structured Sub2API evidence",
);
assert.equal(groupVisualMetaFor("backup").platform, "generic");

assert.match(groupVisualMetaFor("codex-0号池", { platform: "openai" }).badgeClassName, /emerald/);
assert.match(groupVisualMetaFor("claude-kiro", { platform: "anthropic" }).badgeClassName, /orange/);
assert.match(groupVisualMetaFor("Gemini", { platform: "gemini" }).badgeClassName, /blue/);
assert.match(groupVisualMetaFor("grok-super", { platform: "grok" }).badgeClassName, /zinc/);
