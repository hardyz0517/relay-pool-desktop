import { getActiveBackendClient } from "@/lib/bridge/activeBackendClient";
import type { ProxyStatus, RequestLog } from "@/lib/types/proxy";

export const PROXY_STATUS_UPDATED_EVENT = "relay-pool:proxy-status-updated";

export function getProxyStatus(): Promise<ProxyStatus> {
  return getActiveBackendClient().proxy.getProxyStatus();
}

export function startLocalProxy(): Promise<ProxyStatus> {
  return getActiveBackendClient().proxy.startLocalProxy().then(publishProxyStatus);
}

export function stopLocalProxy(): Promise<ProxyStatus> {
  return getActiveBackendClient().proxy.stopLocalProxy().then(publishProxyStatus);
}

export function prepareLocalProxyForUpdate(): Promise<ProxyStatus> {
  return getActiveBackendClient().proxy.prepareLocalProxyForUpdate().then(publishProxyStatus);
}

export function restartLocalProxy(): Promise<ProxyStatus> {
  return getActiveBackendClient().proxy.restartLocalProxy().then(publishProxyStatus);
}

export function listRequestLogs(): Promise<RequestLog[]> {
  return getActiveBackendClient().proxy.listRequestLogs();
}

export function clearRequestLogs(): Promise<void> {
  return getActiveBackendClient().proxy.clearRequestLogs();
}

function publishProxyStatus(status: ProxyStatus) {
  window.dispatchEvent(new CustomEvent<ProxyStatus>(PROXY_STATUS_UPDATED_EVENT, { detail: status }));
  return status;
}
