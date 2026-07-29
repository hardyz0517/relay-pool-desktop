import { getActiveBackendClient } from "@/lib/bridge/activeBackendClient";
import type { ChangeEvent, UpsertChangeEventInput } from "@/lib/types/changeEvents";

function changeEventsClient() {
  return getActiveBackendClient().changeEvents;
}

export function listChangeEvents(): Promise<ChangeEvent[]> {
  return changeEventsClient().listChangeEvents();
}

export function clearChangeEvents(): Promise<void> {
  return changeEventsClient().clearChangeEvents();
}

export function listChangeEventsForStation(stationId: string): Promise<ChangeEvent[]> {
  return changeEventsClient().listChangeEventsForStation(stationId);
}

export function upsertChangeEvent(input: UpsertChangeEventInput): Promise<ChangeEvent> {
  return changeEventsClient().upsertChangeEvent(input);
}

export function markChangeEventRead(id: string): Promise<ChangeEvent> {
  return changeEventsClient().markChangeEventRead(id);
}

export function markChangeEventsRead(ids: string[]): Promise<ChangeEvent[]> {
  const uniqueIds = Array.from(new Set(ids.filter(Boolean)));
  if (uniqueIds.length === 0) {
    return Promise.resolve([]);
  }

  return changeEventsClient().markChangeEventsRead(uniqueIds);
}

export function dismissChangeEvent(id: string): Promise<ChangeEvent> {
  return changeEventsClient().dismissChangeEvent(id);
}

export function resolveChangeEvent(id: string): Promise<ChangeEvent> {
  return changeEventsClient().resolveChangeEvent(id);
}
