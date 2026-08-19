import { getActiveBackendClient } from "@/lib/bridge/activeBackendClient";
import type {
  ApplyModelMappingDocumentInputDto,
  RestoreModelMappingRevisionInputDto,
  SimulateModelMappingInputDto,
  ValidateModelMappingDocumentInputDto,
} from "@/lib/types/modelMapping";

export function getModelMappingWorkspace() {
  return getActiveBackendClient().routing.getModelMappingWorkspace();
}

export function getModelMappingDocument() {
  return getActiveBackendClient().routing.getModelMappingDocument();
}

export function validateModelMappingDocument(input: ValidateModelMappingDocumentInputDto) {
  return getActiveBackendClient().routing.validateModelMappingDocument(input);
}

export function applyModelMappingDocument(input: ApplyModelMappingDocumentInputDto) {
  return getActiveBackendClient().routing.applyModelMappingDocument(input);
}

export function restoreModelMappingRevision(input: RestoreModelMappingRevisionInputDto) {
  return getActiveBackendClient().routing.restoreModelMappingRevision(input);
}

export function simulateModelMapping(input: SimulateModelMappingInputDto) {
  return getActiveBackendClient().routing.simulateModelMapping(input);
}

export function resolveRequestMappingTrace(requestLogId: string) {
  return getActiveBackendClient().routing.resolveRequestMappingTrace(requestLogId);
}
