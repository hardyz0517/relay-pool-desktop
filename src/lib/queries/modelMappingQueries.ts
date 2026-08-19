import {
  getModelMappingDocument,
  getModelMappingWorkspace,
  resolveRequestMappingTrace,
  simulateModelMapping,
} from "@/lib/api/modelMapping";
import type { SimulateModelMappingInputDto } from "@/lib/types/modelMapping";

export const modelMappingQueryKeys = {
  all: ["routing", "modelMapping"] as const,
  workspace: () => ["routing", "modelMapping", "workspace"] as const,
  document: () => ["routing", "modelMapping", "document"] as const,
  simulation: (input: SimulateModelMappingInputDto) => ["routing", "modelMapping", "simulation", input] as const,
  trace: (requestLogId: string) => ["routing", "modelMapping", "trace", requestLogId] as const,
};

export const loadModelMappingWorkspaceQuery = getModelMappingWorkspace;
export const loadModelMappingDocumentQuery = getModelMappingDocument;
export const resolveRequestMappingTraceQuery = resolveRequestMappingTrace;
export const simulateModelMappingQuery = simulateModelMapping;
