import { readdirSync, readFileSync, statSync } from "node:fs";
import { extname, relative, resolve } from "node:path";

const rawPalette = /\b(?:bg|text|border|ring|divide|fill|stroke|outline|decoration|from|via|to|placeholder:text)-(?:white|black|slate|gray|zinc|neutral|stone|red|orange|amber|yellow|lime|green|emerald|teal|cyan|sky|blue|indigo|violet|purple|fuchsia|pink|rose)(?:-[0-9]+)?(?:\/[0-9]+)?\b/g;
const arbitraryUtility = /\b(?:bg|text|border|ring|shadow|fill|stroke)-\[[^\]]*(?:#[0-9a-fA-F]{3,8}(?=[^0-9a-fA-F]|$)|rgba?\(|hsla?\(|hsl\(var\(--)[^\]]*\]/g;
const directColorLiteral = /(?:rgba?|hsla?)\(|#[0-9a-fA-F]{6,8}(?=[^0-9a-fA-F]|$)|hsl\(var\(--/g;
const inlineHexColor = /\b(?:color|backgroundColor|borderColor|fill|stroke)\s*(?:=|:)\s*["'`]#[0-9a-fA-F]{3,8}\b/g;

const patterns = [rawPalette, arbitraryUtility, directColorLiteral, inlineHexColor];
const requestedRoots = process.argv.slice(2).filter((value) => value !== "--");
const roots = requestedRoots.length > 0 ? requestedRoots : ["src"];
const files = roots.flatMap((root) => collect(resolve(root)));
const violations = [];

for (const file of files) {
  const displayPath = relative(process.cwd(), file).replaceAll("\\", "/");
  const lines = readFileSync(file, "utf8").split(/\r?\n/);
  lines.forEach((line, index) => {
    for (const pattern of patterns) {
      const expression = new RegExp(pattern.source, pattern.flags);
      for (const match of line.matchAll(expression)) {
        violations.push(`${displayPath}:${index + 1}: ${match[0]}`);
      }
    }
  });
}

if (violations.length > 0) {
  console.error(violations.join("\n"));
  console.error(`theme audit found ${violations.length} violation(s)`);
  process.exitCode = 1;
} else {
  console.log(`theme audit passed (${files.length} files)`);
}

function collect(path) {
  const stat = statSync(path);
  if (stat.isFile()) {
    return [".ts", ".tsx"].includes(extname(path)) ? [path] : [];
  }
  return readdirSync(path, { withFileTypes: true }).flatMap((entry) => {
    const child = resolve(path, entry.name);
    return entry.isDirectory() ? collect(child) : collect(child);
  });
}
