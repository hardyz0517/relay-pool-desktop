import {
  clearChangeEvents as ipcClearChangeEvents,
  dismissChangeEvent as ipcDismissChangeEvent,
  listChangeEvents as ipcListChangeEvents,
  listChangeEventsForStation as ipcListChangeEventsForStation,
  markChangeEventRead as ipcMarkChangeEventRead,
  markChangeEventsRead as ipcMarkChangeEventsRead,
  resolveChangeEvent as ipcResolveChangeEvent,
  upsertChangeEvent as ipcUpsertChangeEvent,
} from "@/lib/bridge/generated";
import {
  clearMockChangeEvents,
  listMockChangeEvents,
  updateMockChangeEventStatus,
  upsertMockChangeEvent,
} from "@/lib/mock/changeEvents";
import { isTauriInvokeUnavailable } from "@/lib/tauriErrors";
import type { ChangeEvent, UpsertChangeEventInput } from "@/lib/types/changeEvents";

export const CHANGE_EVENTS_UPDATED_EVENT = "relay-pool:change-events-updated";

export function notifyChangeEventsUpdated() {
  if (typeof window === "undefined") {
    return;
  }
  window.dispatchEvent(new CustomEvent(CHANGE_EVENTS_UPDATED_EVENT));
}

export function listChangeEvents() {
  return ipcListChangeEvents().catch((error) => {
    if (isTauriInvokeUnavailable(error)) {
      return listMockChangeEvents();
    }
    throw error;
  });
}

export function clearChangeEvents() {
  return ipcClearChangeEvents().catch((error) => {
    if (isTauriInvokeUnavailable(error)) {
      return clearMockChangeEvents();
    }
    throw error;
  });
}

export function listChangeEventsForStation(stationId: string) {
  return ipcListChangeEventsForStation({ stationId }).catch((error) => {
    if (isTauriInvokeUnavailable(error)) {
      return listMockChangeEvents().then((events) => events.filter((event) => event.stationId === stationId));
    }
    throw error;
  });
}

export function upsertChangeEvent(input: UpsertChangeEventInput) {
  return ipcUpsertChangeEvent(input).catch((error) => {
    if (isTauriInvokeUnavailable(error)) {
      return upsertMockChangeEvent(input);
    }
    throw error;
  });
}

export function markChangeEventRead(id: string) {
  return ipcMarkChangeEventRead({ id }).catch((error) => {
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

  return ipcMarkChangeEventsRead({ ids: uniqueIds }).catch((error) => {
    if (isTauriInvokeUnavailable(error)) {
      return Promise.all(uniqueIds.map((id) => updateMockChangeEventStatus(id, "read")));
    }
    throw error;
  });
}

export function dismissChangeEvent(id: string) {
  return ipcDismissChangeEvent({ id }).catch((error) => {
    if (isTauriInvokeUnavailable(error)) {
      return updateMockChangeEventStatus(id, "dismissed");
    }
    throw error;
  });
}

export function resolveChangeEvent(id: string) {
  return ipcResolveChangeEvent({ id }).catch((error) => {
    if (isTauriInvokeUnavailable(error)) {
      return updateMockChangeEventStatus(id, "resolved");
    }
    throw error;
  });
}
