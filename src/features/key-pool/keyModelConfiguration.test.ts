import { describe, expect, it } from "vitest";
import {
  addModelToList,
  applyDiscoveredModels,
  defaultModelFromPreferred,
  removeModelFromList,
} from "./keyModelConfiguration";

describe("key model configuration", () => {
  it("uses the first preferred model as the default model", () => {
    expect(defaultModelFromPreferred("gpt-5\ngpt-4.1")).toBe("gpt-5");
  });

  it("keeps a discovered default and clears a retired default", () => {
    expect(applyDiscoveredModels(["gpt-5", "gpt-4.1"], "gpt-5")).toEqual({
      modelAllowlist: "gpt-4.1\ngpt-5",
      preferredModels: "gpt-5",
      defaultModelRemoved: false,
    });
    expect(applyDiscoveredModels(["gpt-5"], "gpt-4.1-mini")).toEqual({
      modelAllowlist: "gpt-5",
      preferredModels: "",
      defaultModelRemoved: true,
    });
  });

  it("adds and removes models without case-insensitive duplicates", () => {
    expect(addModelToList("gpt-5", "GPT-5")).toBe("gpt-5");
    expect(addModelToList("gpt-5", "claude-sonnet")).toBe("gpt-5\nclaude-sonnet");
    expect(removeModelFromList("gpt-5\nclaude-sonnet", "GPT-5")).toBe("claude-sonnet");
  });
});
