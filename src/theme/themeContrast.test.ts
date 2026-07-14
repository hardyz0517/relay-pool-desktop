import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const css = readFileSync(new URL("../styles.css", import.meta.url), "utf8");
const pairs = [
  ["foreground", "background"], ["foreground", "surface"],
  ["muted-foreground", "surface"], ["selected-foreground", "selected"],
  ["control-active-foreground", "control-active"],
  ["primary", "surface"], ["primary-foreground", "primary-solid"],
  ["on-solid", "danger-solid"],
  ...["success", "warning", "danger", "info"].map((tone) => [`${tone}-foreground`, `${tone}-surface`]),
  ...["slate", "emerald", "green", "blue", "amber", "indigo", "violet", "purple", "rose"].map(
    (accent) => [`metric-${accent}-foreground`, `metric-${accent}-surface`],
  ),
  ...["anthropic", "openai", "gemini", "grok", "image", "generic"].map(
    (platform) => [`platform-${platform}-foreground`, `platform-${platform}-surface`],
  ),
] as Array<[string, string]>;

type Hsl = [number, number, number];

function themeBlock(theme: "light" | "dark") {
  const pattern = theme === "light"
    ? /:root,\s*\.light\s*\{([^}]*)\}/s
    : /\.dark\s*\{([^}]*)\}/s;
  const block = css.match(pattern)?.[1];
  if (!block) throw new Error(`Missing ${theme} theme block`);
  return block;
}

function parseVariables(block: string) {
  const values = new Map<string, Hsl>();
  for (const match of block.matchAll(/--([\w-]+):\s*([\d.]+)\s+([\d.]+)%\s+([\d.]+)%/g)) {
    values.set(match[1], [Number(match[2]), Number(match[3]), Number(match[4])]);
  }
  return values;
}

function readToken(values: Map<string, Hsl>, name: string) {
  const value = values.get(name);
  if (!value) throw new Error(`Missing theme token: ${name}`);
  return value;
}

function hslToRgb([h, saturation, lightness]: Hsl) {
  const s = saturation / 100;
  const l = lightness / 100;
  const chroma = (1 - Math.abs(2 * l - 1)) * s;
  const segment = h / 60;
  const secondary = chroma * (1 - Math.abs((segment % 2) - 1));
  const [red, green, blue] = segment < 1 ? [chroma, secondary, 0]
    : segment < 2 ? [secondary, chroma, 0]
      : segment < 3 ? [0, chroma, secondary]
        : segment < 4 ? [0, secondary, chroma]
          : segment < 5 ? [secondary, 0, chroma]
            : [chroma, 0, secondary];
  const offset = l - chroma / 2;
  return [red + offset, green + offset, blue + offset];
}

function relativeLuminance(hsl: Hsl) {
  const [red, green, blue] = hslToRgb(hsl).map((channel) =>
    channel <= 0.03928 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4,
  );
  return 0.2126 * red + 0.7152 * green + 0.0722 * blue;
}

function contrast(foreground: Hsl, background: Hsl) {
  const first = relativeLuminance(foreground);
  const second = relativeLuminance(background);
  return (Math.max(first, second) + 0.05) / (Math.min(first, second) + 0.05);
}

describe("theme token contrast", () => {
  it("keeps the light canvas white instead of gray", () => {
    const variables = parseVariables(themeBlock("light"));
    expect(readToken(variables, "background")).toEqual([0, 0, 100]);
    expect(readToken(variables, "surface")).toEqual([0, 0, 100]);
    expect(readToken(variables, "surface-subtle")[2]).toBeGreaterThanOrEqual(98.5);
    expect(readToken(variables, "surface-inset")[2]).toBeGreaterThanOrEqual(97);
  });

  it("keeps light dashboard accents saturated instead of gray", () => {
    const variables = parseVariables(themeBlock("light"));
    for (const token of [
      "success-foreground",
      "warning-foreground",
      "danger-foreground",
      "info-foreground",
      "platform-image-foreground",
      "metric-emerald-foreground",
      "metric-green-foreground",
      "metric-blue-foreground",
      "metric-amber-foreground",
      "metric-indigo-foreground",
      "metric-violet-foreground",
      "metric-purple-foreground",
      "metric-rose-foreground",
    ]) {
      expect(readToken(variables, token)[1], token).toBeGreaterThanOrEqual(72);
    }
  });

  it.each(["light", "dark"] as const)("keeps %s text pairs readable", (theme) => {
    const variables = parseVariables(themeBlock(theme));
    for (const [foreground, background] of pairs) {
      expect(
        contrast(readToken(variables, foreground), readToken(variables, background)),
        `${theme}: ${foreground}/${background}`,
      ).toBeGreaterThanOrEqual(4.5);
    }
  });
});
