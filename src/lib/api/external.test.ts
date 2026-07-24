import { beforeEach, describe, expect, it, vi } from "vitest";

const generated = vi.hoisted(() => ({
  openExternalUrl: vi.fn(),
}));
const transport = vi.hoisted(() => ({ invoke: vi.fn() }));

vi.mock("@/lib/bridge/generated", () => generated);
vi.mock("@/lib/bridge/transport", () => transport);

import { openExternalUrl } from "./external";

describe("external URL generated transport cutover", () => {
  beforeEach(() => {
    generated.openExternalUrl.mockReset().mockResolvedValue(undefined);
    transport.invoke.mockReset().mockRejectedValue(new Error("legacy transport invoked"));
  });

  it("routes URL opening through the generated non-idempotent wrapper", async () => {
    await openExternalUrl("https://example.test");

    expect(generated.openExternalUrl).toHaveBeenCalledWith({ url: "https://example.test" });
    expect(transport.invoke).not.toHaveBeenCalled();
  });
});
