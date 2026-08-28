import { LocalRoutingSettingsEditor } from "./LocalRoutingSettingsEditor";
import { ModelMappingPanel } from "./ModelMappingPanel";
export function LocalRoutingEditTab() {
  return (
    <div className="grid gap-3">
      <div data-tour="routing-policy-scope">
        <LocalRoutingSettingsEditor />
      </div>
      <ModelMappingPanel />
    </div>
  );
}
