import assert from "node:assert/strict";
import { access, readFile } from "node:fs/promises";

const appSource = await readFile("src/app/App.tsx", "utf8");
const shellPageRegistrySource = await readFile("src/app/shellPageRegistry.tsx", "utf8");
const pageTransitionPolicySource = await readFile("src/app/pageTransitionPolicy.ts", "utf8");
const navigationSource = await readFile("src/lib/types/navigation.ts", "utf8");
const keyPoolSource = await readFile("src/features/key-pool/KeyPoolPage.tsx", "utf8");

await access("src/features/key-pool/EditKeyPage.tsx");
const editKeySource = await readFile("src/features/key-pool/EditKeyPage.tsx", "utf8");

assert.ok(
  navigationSource.includes('"editKey"'),
  "navigation should expose a dedicated edit-key page route",
);

assert.ok(
  appSource.includes('import { EditKeyPage } from "@/features/key-pool/EditKeyPage"'),
  "app should import the dedicated edit-key page",
);

assert.ok(
  appSource.includes('const [editingKeyId, setEditingKeyId] = useState<string | null>(null)'),
  "app should keep the key id being edited as page state",
);

assert.ok(
  appSource.includes('case "editKey"'),
  "app should render a dedicated edit-key route",
);

assert.ok(
  /<KeyPoolPage\s+onAddKey=\{actions\.addKey\}\s+onEditKey=\{actions\.editKey\}\s+\/>/.test(
    shellPageRegistrySource,
  ),
  "key-pool page should navigate row edit actions to the edit-key page",
);

assert.ok(
  appSource.includes("resolveActiveShellRouteId(") &&
    appSource.includes("activeRouteId={intent.shellRouteId}"),
  "transient pages should resolve and pass their parent shell route as active",
);

assert.ok(
  /addKey:\s*\{[\s\S]*?parentRouteId:\s*"keyPool"/.test(pageTransitionPolicySource) &&
    /editKey:\s*\{[\s\S]*?parentRouteId:\s*"keyPool"/.test(pageTransitionPolicySource),
  "add-key and edit-key should keep the key-pool shell item active",
);

assert.ok(
  keyPoolSource.includes("onEditKey?: (stationKeyId: string) => void"),
  "key-pool page should accept an edit-key navigation callback",
);

assert.ok(
  /if \(onEditKey\) \{\s*onEditKey\(item\.id\);\s*return;\s*\}/.test(keyPoolSource),
  "key-pool row edit should prefer page navigation before falling back to the legacy dialog",
);

assert.ok(
  editKeySource.includes('import { PageScaffold } from "@/components/shell/PageScaffold"'),
  "edit-key page should use the same page scaffold as create-key",
);

assert.ok(
  editKeySource.includes("PageForm") && editKeySource.includes("SectionCard"),
  "edit-key page should use the same form/card composition as create-key",
);

assert.ok(
  !editKeySource.includes("<Dialog"),
  "edit-key page should not render as a dialog",
);

assert.ok(
  !editKeySource.includes("getStationKeyCapabilities"),
  "edit-key page should not load editable routing capabilities",
);

assert.ok(
  !editKeySource.includes('title="调度能力"') &&
    !editKeySource.includes('label="聊天补全"') &&
    !editKeySource.includes('label="响应接口"') &&
    !editKeySource.includes('label="向量接口"') &&
    !editKeySource.includes('label="流式响应"') &&
    !editKeySource.includes('label="工具调用"') &&
    !editKeySource.includes('label="图片输入"') &&
    !editKeySource.includes('label="推理模型"'),
  "edit-key page should hide the routing capability editor",
);

assert.ok(
  /supportsChatCompletions:\s*true[\s\S]*supportsResponses:\s*true[\s\S]*supportsEmbeddings:\s*true[\s\S]*supportsStream:\s*true[\s\S]*supportsTools:\s*true[\s\S]*supportsVision:\s*true[\s\S]*supportsReasoning:\s*true/.test(editKeySource),
  "edit-key save should persist all protocol capabilities as supported by default",
);

assert.ok(
  editKeySource.includes("KEEP_GROUP_BINDING_VALUE") &&
    editKeySource.includes('return { kind: "keep" as const }'),
  "edit-key page must preserve current group binding when unrelated fields are edited",
);

assert.ok(
  editKeySource.includes("CLEAR_GROUP_BINDING_VALUE") &&
    editKeySource.includes('return { kind: "clear" as const }'),
  "edit-key page must only clear group binding through an explicit clear action",
);
