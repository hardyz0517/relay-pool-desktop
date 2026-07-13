import assert from "node:assert/strict";
import { mkdtemp, readFile, writeFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import ts from "typescript";

async function transpileTsFile(sourcePath, outputPath, replacements = []) {
  let source = await readFile(sourcePath, "utf8");
  for (const [from, to] of replacements) {
    source = source.replaceAll(from, to);
  }
  const output = ts.transpileModule(source, {
    compilerOptions: {
      module: ts.ModuleKind.ESNext,
      target: ts.ScriptTarget.ES2022,
      verbatimModuleSyntax: true,
    },
  }).outputText;
  await writeFile(outputPath, output, "utf8");
}

async function importGroupVisualMeta() {
  const tempRoot = await mkdtemp(join(tmpdir(), "relay-station-group-visual-meta-"));
  const groupCategoriesPath = join(tempRoot, "groupCategories.mjs");
  const visualMetaPath = join(tempRoot, "groupVisualMeta.mjs");
  await transpileTsFile("src/lib/groupCategories.ts", groupCategoriesPath);
  await transpileTsFile("src/features/stations/groupVisualMeta.ts", visualMetaPath, [
    ['@/lib/groupCategories', "./groupCategories.mjs"],
  ]);
  return import(`file://${visualMetaPath.replaceAll("\\", "/")}`);
}

const { groupVisualMetaFor } = await importGroupVisualMeta();
const platformIconSource = await readFile("src/features/stations/components/Sub2ApiPlatformIcon.tsx", "utf8");

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
  "openai",
  "GPT-like group names should use the OpenAI visual platform",
);
assert.equal(groupVisualMetaFor("backup").platform, "generic");

assert.match(groupVisualMetaFor("codex-0号池", { platform: "openai" }).badgeClassName, /emerald/);
assert.match(groupVisualMetaFor("claude-kiro", { platform: "anthropic" }).badgeClassName, /orange/);
assert.match(groupVisualMetaFor("Gemini", { platform: "gemini" }).badgeClassName, /blue/);
assert.match(groupVisualMetaFor("grok-super", { platform: "grok" }).badgeClassName, /zinc/);

assert.ok(
  platformIconSource.includes('if (platform === "image")'),
  "image-generation groups should render a dedicated picture icon instead of falling through to the generic globe",
);
