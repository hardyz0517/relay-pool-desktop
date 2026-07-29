import { getActiveBackendClient } from "@/lib/bridge/activeBackendClient";
import type {
  CreateRemoteStationKeyInput,
  CreateStationKeyInput,
  SaveStationKeyWithDefaultsInput,
  StationKeyConnectivityTestEvent,
  UpdateStationKeyInput,
  UpdateStationSessionInput,
} from "@/lib/types/stationKeys";

function stationKeysClient() {
  return getActiveBackendClient().stationKeys;
}

export function listStationKeys(stationId: string) {
  return stationKeysClient().listStationKeys(stationId);
}

export function getRemoteKeyCapability(stationId: string) {
  return stationKeysClient().getRemoteKeyCapability(stationId);
}

export function listRemoteStationKeys(stationId: string) {
  return stationKeysClient().listRemoteStationKeys(stationId);
}

export function scanRemoteStationKeys(stationId: string) {
  return stationKeysClient().scanRemoteStationKeys(stationId);
}

export function createRemoteStationKey(input: CreateRemoteStationKeyInput) {
  return stationKeysClient().createRemoteStationKey(input);
}

export function createLocalStationKeyFromRemote(remoteKeyId: string, stationId: string) {
  return stationKeysClient().createLocalStationKeyFromRemote(remoteKeyId, stationId);
}

export function deleteRemoteStationKey(remoteKeyId: string, stationId: string) {
  return stationKeysClient().deleteRemoteStationKey(remoteKeyId, stationId);
}

export function bindRemoteStationKey(remoteKeyId: string, stationKeyId: string) {
  return stationKeysClient().bindRemoteStationKey(remoteKeyId, stationKeyId);
}

export function unbindRemoteStationKey(remoteKeyId: string, stationId: string) {
  return stationKeysClient().unbindRemoteStationKey(remoteKeyId, stationId);
}

export function createStationKey(input: CreateStationKeyInput) {
  return stationKeysClient().createStationKey(input);
}

export function updateStationKey(input: UpdateStationKeyInput) {
  return stationKeysClient().updateStationKey(input);
}

export function saveStationKeyWithDefaults(input: SaveStationKeyWithDefaultsInput) {
  return stationKeysClient().saveStationKeyWithDefaults(input);
}

export function updateStationKeyGroupBinding(stationKeyId: string, groupBindingId: string) {
  return stationKeysClient().updateStationKeyGroupBinding(stationKeyId, groupBindingId);
}

export function deleteStationKey(id: string) {
  return stationKeysClient().deleteStationKey(id);
}

export function reorderStationKeys(stationId: string, keyIds: string[]) {
  return stationKeysClient().reorderStationKeys(stationId, keyIds);
}

export function listKeyPoolItems() {
  return stationKeysClient().listKeyPoolItems();
}

export function reorderKeyPool(keyIds: string[]) {
  return stationKeysClient().reorderKeyPool(keyIds);
}

export function testStationKeyConnectivity(
  stationKeyId: string,
  model: string,
  options: { onEvent?: (event: StationKeyConnectivityTestEvent) => void } = {},
) {
  return stationKeysClient().testStationKeyConnectivity(stationKeyId, model, options);
}

export function getStationCredentials(stationId: string) {
  return stationKeysClient().getStationCredentials(stationId);
}

export function updateStationCredentials(input: {
  stationId: string;
  loginUsername: string | null;
  loginPassword: string | null;
  rememberPassword: boolean;
}) {
  return stationKeysClient().updateStationCredentials(input);
}

export function clearStationCredentials(stationId: string) {
  return stationKeysClient().clearStationCredentials(stationId);
}

export function updateStationSession(input: UpdateStationSessionInput) {
  return stationKeysClient().updateStationSession(input);
}
