import { describe, expect, it } from "vitest";
import type { ModelMappingDocumentDto } from "./modelMapping";
import { toModelMappingApplyDocument } from "./modelMapping";

describe("toModelMappingApplyDocument", () => {
  it("strips workspace-only fields before applying a document", () => {
    const document = {
      formatVersion: 1,
      baseRevision: 2,
      policy: { unmatchedModelBehavior: "preserve" },
      rules: [{
        id: "rule-1",
        priority: 10,
        enabled: true,
        matcher: { kind: "exact", model: "gpt-5.6-luna" },
        conditions: { endpointKinds: [], stream: "any", tools: "any", vision: "any", reasoning: "any" },
        action: { kind: "map_fixed", target: { kind: "literal", upstreamModel: "gpt-5.6-terra" } },
        note: null,
        revision: 2,
        createdAtMs: 0,
        updatedAtMs: 0,
        workspaceLabel: "ignore me",
      }],
      profiles: [],
      bindings: [],
      workspaceRevision: 9,
    } as unknown as ModelMappingDocumentDto & { workspaceRevision: number };

    const normalized = toModelMappingApplyDocument(document);

    expect(normalized).toEqual({
      formatVersion: 1,
      baseRevision: 2,
      policy: { unmatchedModelBehavior: "preserve" },
      rules: [{
        id: "rule-1",
        priority: 10,
        enabled: true,
        matcher: { kind: "exact", model: "gpt-5.6-luna" },
        conditions: { endpointKinds: [], stream: "any", tools: "any", vision: "any", reasoning: "any" },
        action: { kind: "map_fixed", target: { kind: "literal", upstreamModel: "gpt-5.6-terra" } },
        note: null,
        revision: 2,
        createdAtMs: 0,
        updatedAtMs: 0,
      }],
      profiles: [],
      bindings: [],
    });
  });
});
