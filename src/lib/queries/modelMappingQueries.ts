import {
  getModelMappingWorkspace,
} from "@/lib/api/modelMapping";

export const modelMappingQueryKeys = {
  all: ["routing", "modelMapping"] as const,
  workspace: () => ["routing", "modelMapping", "workspace"] as const,
};

export const loadModelMappingWorkspaceQuery = getModelMappingWorkspace;
