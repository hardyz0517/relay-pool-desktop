import { invoke } from "@tauri-apps/api/core";
import {
  clearMockChangeEvents,
  listMockChangeEvents,
  updateMockChangeEventStatus,
  upsertMockChangeEvent,
} from "@/lib/mock/changeEvents";
import { isTauriCommandNotFound, isTauriInvokeUnavailable } from "@/lib/tauriErrors";
import type { ChangeEvent, UpsertChangeEventInput } from "@/lib/types/changeEvents";

export const CHANGE_EVENTS_UPDATED_EVENT = "relay-pool:change-events-updated";

export function notifyChangeEventsUpdated() {
  if (typeof window === "undefined") {
    return;
  }
  window.dispatchEvent(new CustomEvent(CHANGE_EVENTS_UPDATED_EVENT));
}

export function listChangeEvents() {
  return invoke<ChangeEvent[]>("list_change_events").catch((error) => {
    if (isTauriInvokeUnavailable(error)) {
      return listMockChangeEvents();
    }
    throw error;
  });
}

export function clearChangeEvents() {
  return invoke<void>("clear_change_events").catch((error) => {
    if (isTauriInvokeUnavailable(error)) {
      return clearMockChangeEvents();
    }
    throw error;
  });
}

export function listChangeEventsForStation(stationId: string) {
  return invoke<ChangeEvent[]>("list_change_events_for_station", { stationId }).catch((error) => {
    if (isTauriCommandNotFound(error)) {
      return listChangeEvents().then((events) => events.filter((event) => event.stationId === stationId));
    }
    if (isTauriInvokeUnavailable(error)) {
      return listMockChangeEvents().then((events) => events.filter((event) => event.stationId === stationId));
    }
    throw error;
  });
}

export function upsertChangeEvent(input: UpsertChangeEventInput) {
  return invoke<ChangeEvent>("upsert_change_event", { input }).catch((error) => {
    if (isTauriInvokeUnavailable(error)) {
      return upsertMockChangeEvent(input);
    }
    throw error;
  });
}

export function markChangeEventRead(id: string) {
  return invoke<ChangeEvent>("mark_change_event_read", { id }).catch((error) => {
    if (isTauriInvokeUnavailable(error)) {
      return updateMockChangeEventStatus(id, "read");
    }
    throw error;
  });
}

export function markChangeEventsRead(ids: string[]) {
  const uniqueIds = Array.from(new Set(ids.filter(Boolean)));
  if (uniqueIds.length === 0) {
    return Promise.resolve([]);
  }

  return invoke<ChangeEvent[]>("mark_change_events_read", { ids: uniqueIds }).catch((error) => {
    if (isTauriCommandNotFound(error)) {
      return Promise.all(uniqueIds.map((id) => markChangeEventRead(id)));
    }
    if (isTauriInvokeUnavailable(error)) {
      return Promise.all(uniqueIds.map((id) => updateMockChangeEventStatus(id, "read")));
    }
    throw error;
  });
}

export function dismissChangeEvent(id: string) {
  return invoke<ChangeEvent>("dismiss_change_event", { id }).catch((error) => {
    if (isTauriInvokeUnavailable(error)) {
      return updateMockChangeEventStatus(id, "dismissed");
    }
    throw error;
  });
}

export function resolveChangeEvent(id: string) {
  return invoke<ChangeEvent>("resolve_change_event", { id }).catch((error) => {
    if (isTauriInvokeUnavailable(error)) {
      return updateMockChangeEventStatus(id, "resolved");
    }
    throw error;
  });
}
