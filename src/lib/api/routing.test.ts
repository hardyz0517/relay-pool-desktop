import { beforeEach, describe, expect, it, vi } from "vitest";

const generated = vi.hoisted(() => ({
  deleteModelAlias: vi.fn(),
  getStationKeyCapabilities: vi.fn(),
  getStationKeyHealth: vi.fn(),
  listModelAliases: vi.fn(),
  listStationKeyHealth: vi.fn(),
  simulateRoute: vi.fn(),
  updateStationKeyCapabilities: vi.fn(),
  upsertModelAlias: vi.fn(),
}));

const transport = vi.hoisted(() => ({
  invoke: vi.fn(),
}));

vi.mock("@/lib/bridge/generated", () => generated);
vi.mock("@/lib/bridge/transport", () => transport);

import { deleteModelAlias, updateStationKeyCapabilities, upsertModelAlias } from "./routing";

describe("routing mutation generated transport cutover", () => {
  beforeEach(() => {
    for (const fn of Object.values(generated)) fn.mockReset().mockResolvedValue(undefined);
    transport.invoke.mockReset().mockRejectedValue(new Error("legacy transport invoked"));
  });

  it("routes capability and model-alias mutations through generated wrappers", async () => {
    const capabilities = {
      stationKeyId: "key-1",
      supportsChatCompletions: true,
      supportsResponses: true,
      supportsEmbeddings: false,
      supportsStream: true,
      supportsTools: false,
      supportsVision: false,
      supportsReasoning: false,
      modelAllowlist: ["fixture-model"],
      modelBlocklist: [],
      preferredModels: ["fixture-model"],
      onlyUseAsBackup: false,
      routingTags: ["fixture"],
    };
    const alias = {
      id: null,
      clientModel: "client-model",
      upstreamModel: "upstream-model",
      enabled: true,
      note: null,
    };

    await updateStationKeyCapabilities(capabilities);
    await upsertModelAlias(alias);
    await deleteModelAlias("alias-1");

    expect(generated.updateStationKeyCapabilities).toHaveBeenCalledWith(capabilities);
    expect(generated.upsertModelAlias).toHaveBeenCalledWith(alias);
    expect(generated.deleteModelAlias).toHaveBeenCalledWith({ id: "alias-1" });
    expect(transport.invoke).not.toHaveBeenCalled();
  });
});
