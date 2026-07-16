import { invoke } from "@tauri-apps/api/core";
import { isTauriInvokeUnavailable } from "@/lib/tauriErrors";
import type { ProxyStatus, RequestLog } from "@/lib/types/proxy";

export const PROXY_STATUS_UPDATED_EVENT = "relay-pool:proxy-status-updated";

let memoryProxyStatus: ProxyStatus = {
  running: false,
  lifecycle: "stopped",
  bindAddr: "127.0.0.1",
  port: 8787,
  startedAt: null,
  lastError: null,
  activeRequests: 0,
  requestCount: 0,
};
let memoryRequestLogs: RequestLog[] = [];

export function getProxyStatus() {
  return invoke<ProxyStatus>("get_proxy_status").catch((error) => {
    if (isTauriInvokeUnavailable(error)) {
      return memoryProxyStatus;
    }
    throw error;
  });
}

export function startLocalProxy() {
  return invoke<ProxyStatus>("start_local_proxy")
    .catch((error) => {
      if (isTauriInvokeUnavailable(error)) {
        memoryProxyStatus = {
          ...memoryProxyStatus,
          running: true,
          lifecycle: "running",
          startedAt: new Date().toISOString(),
          lastError: null,
        };
        return memoryProxyStatus;
      }
      throw error;
    })
    .then(publishProxyStatus);
}

export function stopLocalProxy() {
  return invoke<ProxyStatus>("stop_local_proxy")
    .catch((error) => {
      if (isTauriInvokeUnavailable(error)) {
        memoryProxyStatus = {
          ...memoryProxyStatus,
          running: false,
          lifecycle: "stopped",
          activeRequests: 0,
        };
        return memoryProxyStatus;
      }
      throw error;
    })
    .then(publishProxyStatus);
}

export function prepareLocalProxyForUpdate() {
  return invoke<ProxyStatus>("prepare_local_proxy_for_update")
    .catch((error) => {
      if (isTauriInvokeUnavailable(error)) {
        memoryProxyStatus = {
          ...memoryProxyStatus,
          running: false,
          lifecycle: "stopped",
          activeRequests: 0,
        };
        return memoryProxyStatus;
      }
      throw error;
    })
    .then(publishProxyStatus);
}

export function restartLocalProxy() {
  return invoke<ProxyStatus>("restart_local_proxy")
    .catch((error) => {
      if (isTauriInvokeUnavailable(error)) {
        memoryProxyStatus = {
          ...memoryProxyStatus,
          running: true,
          lifecycle: "running",
          startedAt: new Date().toISOString(),
          lastError: null,
        };
        return memoryProxyStatus;
      }
      throw error;
    })
    .then(publishProxyStatus);
}

export function listRequestLogs() {
  return invoke<RequestLog[]>("list_request_logs").catch((error) => {
    if (isTauriInvokeUnavailable(error)) {
      return memoryRequestLogs;
    }
    throw error;
  });
}

export function clearRequestLogs() {
  return invoke<void>("clear_request_logs").catch((error) => {
    if (isTauriInvokeUnavailable(error)) {
      memoryRequestLogs = [];
      return;
    }
    throw error;
  });
}

function publishProxyStatus(status: ProxyStatus) {
  window.dispatchEvent(new CustomEvent<ProxyStatus>(PROXY_STATUS_UPDATED_EVENT, { detail: status }));
  return status;
}
