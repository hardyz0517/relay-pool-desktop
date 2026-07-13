import { readFileSync } from "node:fs";
import assert from "node:assert/strict";

const typeSource = readFileSync("src/lib/types/localRouting.ts", "utf8");
const apiSource = readFileSync("src/lib/api/localRouting.ts", "utf8");
const commandSource = readFileSync("src-tauri/src/commands/mod.rs", "utf8");
const libSource = readFileSync("src-tauri/src/lib.rs", "utf8");
const editTabSource = readFileSync("src/features/routing/LocalRoutingEditTab.tsx", "utf8");
const candidateRowSource = readFileSync("src/features/routing/LocalRoutingCandidateRow.tsx", "utf8");
const statusTabSource = readFileSync("src/features/routing/LocalRoutingStatusTab.tsx", "utf8");
const statusCandidateRowSource = readFileSync(
  "src/features/routing/LocalRoutingStatusCandidateRow.tsx",
  "utf8",
);

assert.match(typeSource, /export type ReorderLocalRoutingKeysInput = \{/);
assert.match(typeSource, /stationKeyIds: string\[\]/);

assert.match(apiSource, /reorderLocalRoutingKeys\(input: ReorderLocalRoutingKeysInput\)/);
assert.match(
  apiSource,
  /invoke<LocalRoutingWorkspace>\("reorder_local_routing_keys", \{ input \}\)/,
);
assert.match(apiSource, /isInvokeUnavailable/);

assert.match(commandSource, /struct ReorderLocalRoutingKeysInput/);
assert.match(commandSource, /pub station_key_ids: Vec<String>/);
assert.match(commandSource, /pub fn reorder_local_routing_keys/);
assert.match(commandSource, /database\.reorder_local_routing_keys\(input\.station_key_ids\)/);
assert.match(libSource, /commands::reorder_local_routing_keys/);

assert.equal(typeSource.includes("权重"), false, "local routing types must not expose 权重 copy");
assert.equal(apiSource.includes("权重"), false, "local routing API must not expose 权重 copy");
assert.match(editTabSource, /DndContext/);
assert.match(editTabSource, /SortableContext/);
assert.match(editTabSource, /reorderLocalRoutingKeys/);
assert.match(editTabSource, /useRef/);
assert.match(editTabSource, /saveOperationRef/);
assert.match(editTabSource, /workspaceVersionRef/);
assert.equal(
  editTabSource.match(/operationId !== saveOperationRef\.current/g)?.length,
  2,
  "success and failure paths must both ignore stale save operations",
);
assert.equal(
  editTabSource.match(/workspaceVersionAtStart !== workspaceVersionRef\.current/g)?.length,
  2,
  "success and failure paths must both ignore responses after workspace refresh",
);
assert.match(editTabSource, /syncState === "saving"/);
assert.match(editTabSource, /disabled=\{syncState === "saving"\}/);
assert.match(editTabSource, /useSortable\(\{\s*id: candidate\.stationKeyId,\s*disabled,\s*\}\)/s);
assert.match(candidateRowSource, /const isSortable = Boolean\(/);
assert.match(candidateRowSource, /draggable=\{isSortable\}/);
assert.match(
  statusTabSource,
  /workspace\.candidates\.map\(\(candidate, index\) =>/,
  "status candidate rows should derive visible order from the rendered list position",
);
assert.match(
  statusTabSource,
  /<LocalRoutingStatusCandidateRow\s+key=\{candidate\.stationKeyId\}\s+candidate=\{candidate\}\s+order=\{index \+ 1\}/s,
  "status candidate rows should pass the same 1-based visible order as the edit preview",
);
assert.doesNotMatch(statusTabSource, /dragAttributes|dragListeners|dragDisabled/);
assert.doesNotMatch(statusTabSource, /DndContext|SortableContext|useSortable|reorderLocalRoutingKeys/);
assert.doesNotMatch(statusCandidateRowSource, /dragAttributes|dragListeners|dragDisabled/);
assert.doesNotMatch(statusCandidateRowSource, /DndContext|SortableContext|useSortable|reorderLocalRoutingKeys/);
assert.equal(editTabSource.includes("权重"), false, "local routing edit UI must not expose 权重 copy");
assert.equal(editTabSource.includes("保存策略"), false, "local routing edit UI must not expose page-level 保存策略 copy");

console.log("local routing reorder contract ok");
