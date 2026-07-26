import { getActiveBackendClient } from "@/lib/bridge/activeBackendClient";
import type { CollectorTaskType, StationLoginTestInput } from "@/lib/types/collector";

export function detectSub2apiStation(stationId: string) {
  return getActiveBackendClient().collectors.detectSub2apiStation(stationId);
}

export function collectSub2apiStation(stationId: string) {
  return getActiveBackendClient().collectors.collectSub2apiStation(stationId);
}

export function detectStationInfo(stationId: string) {
  return getActiveBackendClient().collectors.detectStationInfo(stationId);
}

export function collectStationInfo(stationId: string) {
  return getActiveBackendClient().collectors.collectStationInfo(stationId);
}

export function collectStationTask(stationId: string, taskType: CollectorTaskType) {
  return getActiveBackendClient().collectors.collectStationTask(stationId, taskType);
}

export function testStationLogin(stationId: string) {
  return getActiveBackendClient().collectors.testStationLogin(stationId);
}

export function testStationLoginInput(input: StationLoginTestInput) {
  return getActiveBackendClient().collectors.testStationLoginInput(input);
}

export function listCollectorSnapshots(stationId: string) {
  return getActiveBackendClient().collectors.listCollectorSnapshots(stationId);
}

export function getLatestCollectorSnapshot(stationId: string) {
  return getActiveBackendClient().collectors.getLatestCollectorSnapshot(stationId);
}

export function listLatestCollectorSnapshots(stationIds: string[]) {
  return getActiveBackendClient().collectors.listLatestCollectorSnapshots(stationIds);
}

export function startCaptureSession(stationId: string) {
  return getActiveBackendClient().collectors.startCaptureSession(stationId);
}

export function startManualAuthorization(stationId: string) {
  return startCaptureSession(stationId);
}

export function getCaptureSessionStatus(stationId: string) {
  return getActiveBackendClient().collectors.getCaptureSessionStatus(stationId);
}

export function finishCaptureSession(stationId: string) {
  return getActiveBackendClient().collectors.finishCaptureSession(stationId);
}

export function finishWebAuthorizationSession(stationId: string) {
  return getActiveBackendClient().collectors.finishWebAuthorizationSession(stationId);
}

export function clearCaptureSession(stationId: string) {
  return getActiveBackendClient().collectors.clearCaptureSession(stationId);
}

export function closeCaptureSession(stationId: string) {
  return getActiveBackendClient().collectors.closeCaptureSession(stationId);
}
