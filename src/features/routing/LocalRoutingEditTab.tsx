import { LocalRoutingSettingsEditor } from "./LocalRoutingSettingsEditor";
import { ModelMappingPanel } from "./ModelMappingPanel";
export function LocalRoutingEditTab() {
  return (
    <div className="grid gap-3">
      <LocalRoutingSettingsEditor />
      <ModelMappingPanel />
    </div>
  );
}
