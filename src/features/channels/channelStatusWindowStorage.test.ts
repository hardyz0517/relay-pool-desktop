import { describe, expect, it, vi } from "vitest";
import {
  CHANNEL_STATUS_WINDOW_STORAGE_KEY,
  readChannelStatusWindow,
  writeChannelStatusWindow,
  type ChannelStatusWindowStorage,
} from "./channelStatusWindowStorage";

function storage(value: string | null): ChannelStatusWindowStorage {
  return { getItem: vi.fn(() => value), setItem: vi.fn() };
}

describe("channel status window storage", () => {
  it("restores every supported monitoring window", () => {
    expect(readChannelStatusWindow(storage("recent"))).toBe("recent");
    expect(readChannelStatusWindow(storage("last24h"))).toBe("last24h");
    expect(readChannelStatusWindow(storage("last7d"))).toBe("last7d");
    expect(readChannelStatusWindow(storage("last30d"))).toBe("last30d");
  });

  it("falls back to 24 hours for missing, stale, or inaccessible preferences", () => {
    expect(readChannelStatusWindow(storage(null))).toBe("last24h");
    expect(readChannelStatusWindow(storage("last90d"))).toBe("last24h");
    expect(readChannelStatusWindow({
      getItem: () => { throw new Error("blocked"); },
      setItem: vi.fn(),
    })).toBe("last24h");
  });

  it("persists a selected window without failing when storage is unavailable", () => {
    const target = storage(null);
    expect(writeChannelStatusWindow("last7d", target)).toBe(true);
    expect(target.setItem).toHaveBeenCalledWith(CHANNEL_STATUS_WINDOW_STORAGE_KEY, "last7d");
    expect(writeChannelStatusWindow("recent", null)).toBe(false);
    expect(writeChannelStatusWindow("last30d", {
      getItem: vi.fn(),
      setItem: () => { throw new Error("full"); },
    })).toBe(false);
  });
});
