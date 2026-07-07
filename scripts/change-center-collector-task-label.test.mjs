import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const source = await readFile("src/features/changes/changeEventViewModels.ts", "utf8");

assert.ok(
  source.includes('readString(newValue, "taskType")') &&
    source.includes('readString(oldValue, "taskType")'),
  "collector change events should read taskType from persisted event payloads",
);

assert.ok(
  source.includes("formatCollectorTaskLabel") &&
    source.includes('if (value === "balance") return "余额采集"') &&
    source.includes('if (value === "groups") return "分组采集"'),
  "collector change event titles should render task-specific labels",
);

assert.ok(
  source.includes("`${stationSubject} ${taskLabel}${event.eventType === \"collector_failed\" ? \"失败\" : \"恢复\"}`"),
  "collector change event titles should distinguish failed and recovered task labels",
);
