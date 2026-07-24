import { beforeEach, describe, expect, it, vi } from "vitest";

const generated = vi.hoisted(() => ({
  getSettings: vi.fn(),
  loadLocalRoutingWorkspace: vi.fn(),
  reorderLocalRoutingKeys: vi.fn(),
}));
const transport = vi.hoisted(() => ({ invoke: vi.fn() }));

vi.mock("@/lib/bridge/generated", () => generated);
vi.mock("@/lib/bridge/transport", () => transport);

import { reorderLocalRoutingKeys } from "./localRouting";

describe("local routing reorder generated transport cutover", () => {
  beforeEach(() => {
    generated.reorderLocalRoutingKeys.mockReset().mockResolvedValue(undefined);
    transport.invoke.mockReset().mockResolvedValue(undefined);
  });

  it("routes reorder through the generated wrapper", async () => {
    const input = { stationKeyIds: ["key-1", "key-2"] };
    await reorderLocalRoutingKeys(input);
    expect(generated.reorderLocalRoutingKeys).toHaveBeenCalledWith(input);
    expect(transport.invoke).not.toHaveBeenCalled();
  });
});
