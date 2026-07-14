import { describe, expect, it } from "vitest";

import { groupVisualMetaFor } from "./groupVisualMeta";

describe("groupVisualMetaFor", () => {
  it.each([
    ["claude", "anthropic"],
    ["gpt", "openai"],
    ["gemini", "gemini"],
    ["grok", "grok"],
    ["image-generation", "image"],
    ["embedding", "generic"],
  ] as const)("maps %s to %s without visual classes", (groupName, platform) => {
    const meta = groupVisualMetaFor(groupName);
    expect(meta.platform).toBe(platform);
    expect(meta).not.toHaveProperty("badgeClassName");
    expect(meta).not.toHaveProperty("iconClassName");
    expect(meta).not.toHaveProperty("rateBadgeClassName");
  });
});
