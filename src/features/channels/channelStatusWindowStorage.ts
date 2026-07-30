import type { ChannelStatusWorkspaceWindow } from "@/lib/types/channelMonitors";

export const CHANNEL_STATUS_WINDOW_STORAGE_KEY = "relay-pool.channel-status.window";

export type ChannelStatusWindowStorage = Pick<Storage, "getItem" | "setItem">;

const channelStatusWindows: readonly ChannelStatusWorkspaceWindow[] = [
  "recent",
  "last24h",
  "last7d",
  "last30d",
];

function browserStorage(): ChannelStatusWindowStorage | null {
  try {
    return typeof window === "undefined" ? null : window.localStorage;
  } catch {
    return null;
  }
}

export function readChannelStatusWindow(
  storage: ChannelStatusWindowStorage | null = browserStorage(),
): ChannelStatusWorkspaceWindow {
  try {
    const storedWindow = storage?.getItem(CHANNEL_STATUS_WINDOW_STORAGE_KEY);
    return channelStatusWindows.find((value) => value === storedWindow) ?? "last24h";
  } catch {
    return "last24h";
  }
}

export function writeChannelStatusWindow(
  value: ChannelStatusWorkspaceWindow,
  storage: ChannelStatusWindowStorage | null = browserStorage(),
): boolean {
  try {
    if (!storage) return false;
    storage.setItem(CHANNEL_STATUS_WINDOW_STORAGE_KEY, value);
    return true;
  } catch {
    return false;
  }
}
