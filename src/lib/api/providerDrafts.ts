import { getActiveBackendClient } from "@/lib/bridge/activeBackendClient";
import type { ProviderDraftPatch, ProviderDraftPayload } from "@/lib/types/providerDrafts";

function providerDraftsClient() {
  const client = getActiveBackendClient().providerDrafts;
  if (!client) throw new Error("当前后端不支持供应商草稿");
  return client;
}

export function createOrResumeProviderDraft(payload: ProviderDraftPayload) {
  return providerDraftsClient().createOrResume({
    baseStationId: null,
    payload,
  });
}

export function patchProviderDraft(input: ProviderDraftPatch) {
  return providerDraftsClient().patch(input);
}

export function discardProviderDraft(draftId: string) {
  return providerDraftsClient().discard(draftId);
}

export function collectProviderDraftPreview(
  draftId: string,
  taskType: "detect" | "balance" | "groups" | "models" | "full",
) {
  return providerDraftsClient().collectPreview({ draftId, taskType });
}

export function scanProviderDraftRemoteKeys(draftId: string) {
  return providerDraftsClient().scanRemoteKeys(draftId);
}

export function startProviderDraftAuthorization(draftId: string) {
  return providerDraftsClient().startAuthorization(draftId);
}

export function commitProviderDraft(draftId: string, expectedRevision: number, commitKey: string) {
  return providerDraftsClient().commit({ draftId, expectedRevision, commitKey });
}
