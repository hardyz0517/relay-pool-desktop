import { getActiveBackendClient } from "@/lib/bridge/activeBackendClient";
import type { ProxyStatus, RequestLog } from "@/lib/types/proxy";

export function getProxyStatus(): Promise<ProxyStatus> {
  return getActiveBackendClient().proxy.getProxyStatus();
}

export function startLocalProxy(): Promise<ProxyStatus> {
  return getActiveBackendClient().proxy.startLocalProxy();
}

export function stopLocalProxy(): Promise<ProxyStatus> {
  return getActiveBackendClient().proxy.stopLocalProxy();
}

export function prepareLocalProxyForUpdate(): Promise<ProxyStatus> {
  return getActiveBackendClient().proxy.prepareLocalProxyForUpdate();
}

export function restartLocalProxy(): Promise<ProxyStatus> {
  return getActiveBackendClient().proxy.restartLocalProxy();
}

export function listRequestLogs(): Promise<RequestLog[]> {
  return getActiveBackendClient().proxy.listRequestLogs();
}

export function clearRequestLogs(): Promise<void> {
  return getActiveBackendClient().proxy.clearRequestLogs();
}
