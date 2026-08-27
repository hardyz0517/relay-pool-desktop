import { getActiveBackendClient } from "@/lib/bridge/activeBackendClient";
import type {
  ApplyModelMappingDocumentInputDto,
} from "@/lib/types/modelMapping";

export function getModelMappingWorkspace() {
  return getActiveBackendClient().routing.getModelMappingWorkspace();
}

export function applyModelMappingDocument(input: ApplyModelMappingDocumentInputDto) {
  return getActiveBackendClient().routing.applyModelMappingDocument(input);
}
