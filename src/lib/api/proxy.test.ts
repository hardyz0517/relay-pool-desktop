import { beforeEach, describe, expect, it, vi } from "vitest";

const generated = vi.hoisted(() => ({
  clearRequestLogs: vi.fn(),
  listRequestLogs: vi.fn(),
}));

vi.mock("@/lib/bridge/generated", () => generated);

import { clearRequestLogs, listRequestLogs } from "./proxy";

describe("request log generated transport cutover", () => {
  beforeEach(() => {
    generated.clearRequestLogs.mockReset().mockResolvedValue(undefined);
    generated.listRequestLogs.mockReset().mockResolvedValue([]);
  });

  it("routes both request-log commands through generated wrappers", async () => {
    await listRequestLogs();
    await clearRequestLogs();
    expect(generated.listRequestLogs).toHaveBeenCalledWith();
    expect(generated.clearRequestLogs).toHaveBeenCalledWith();
  });
});
