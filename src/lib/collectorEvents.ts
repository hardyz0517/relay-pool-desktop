import type { CollectorEvent, CollectorRunResult } from "@/lib/types/collector";

export const remoteKeyRefreshEventType = "remote_keys";

export function remoteKeyRefreshFailure(result: CollectorRunResult): CollectorEvent | null {
  return (
    result.events.find(
      (event) => event.eventType === remoteKeyRefreshEventType && event.status !== "success",
    ) ?? null
  );
}
