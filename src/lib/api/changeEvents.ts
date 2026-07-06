import { invoke } from "@tauri-apps/api/core";
import {
  clearMockChangeEvents,
  listMockChangeEvents,
  updateMockChangeEventStatus,
  upsertMockChangeEvent,
} from "@/lib/mock/changeEvents";
import type { ChangeEvent, UpsertChangeEventInput } from "@/lib/types/changeEvents";

export const CHANGE_EVENTS_UPDATED_EVENT = "relay-pool:change-events-updated";

export function notifyChangeEventsUpdated() {
  if (typeof window === "undefined") {
    return;
  }
  window.dispatchEvent(new CustomEvent(CHANGE_EVENTS_UPDATED_EVENT));
}

function isInvokeUnavailable(error: unknown) {
  return /invoke/i.test(getErrorMessage(error));
}

function isCommandNotFound(error: unknown) {
  return /command .* not found/i.test(getErrorMessage(error));
}

function getErrorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}

export function listChangeEvents() {
  return invoke<ChangeEvent[]>("list_change_events").catch((error) => {
    if (isInvokeUnavailable(error)) {
      return listMockChangeEvents();
    }
    throw error;
  });
}

export function clearChangeEvents() {
  return invoke<void>("clear_change_events").catch((error) => {
    if (isInvokeUnavailable(error)) {
      return clearMockChangeEvents();
    }
    throw error;
  });
}

export function listChangeEventsForStation(stationId: string) {
  return invoke<ChangeEvent[]>("list_change_events_for_station", { stationId }).catch((error) => {
    if (isCommandNotFound(error)) {
      return listChangeEvents().then((events) => events.filter((event) => event.stationId === stationId));
    }
    if (isInvokeUnavailable(error)) {
      return listMockChangeEvents().then((events) => events.filter((event) => event.stationId === stationId));
    }
    throw error;
  });
}

export function upsertChangeEvent(input: UpsertChangeEventInput) {
  return invoke<ChangeEvent>("upsert_change_event", { input }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      return upsertMockChangeEvent(input);
    }
    throw error;
  });
}

export function markChangeEventRead(id: string) {
  return invoke<ChangeEvent>("mark_change_event_read", { id }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      return updateMockChangeEventStatus(id, "read");
    }
    throw error;
  });
}

export function dismissChangeEvent(id: string) {
  return invoke<ChangeEvent>("dismiss_change_event", { id }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      return updateMockChangeEventStatus(id, "dismissed");
    }
    throw error;
  });
}

export function resolveChangeEvent(id: string) {
  return invoke<ChangeEvent>("resolve_change_event", { id }).catch((error) => {
    if (isInvokeUnavailable(error)) {
      return updateMockChangeEventStatus(id, "resolved");
    }
    throw error;
  });
}
