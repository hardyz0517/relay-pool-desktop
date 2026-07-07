import { listModelAliases } from "@/lib/api/routing";
import { getSettings } from "@/lib/api/settings";
import type { ModelAlias } from "@/lib/types/routing";
import type { AppSettings } from "@/lib/types/settings";

export type RoutingWorkspace = {
  settings: AppSettings;
  modelAliases: ModelAlias[];
};

export async function loadRoutingWorkspace(): Promise<RoutingWorkspace> {
  const [settings, modelAliases] = await Promise.all([getSettings(), listModelAliases()]);

  return {
    settings,
    modelAliases,
  };
}
