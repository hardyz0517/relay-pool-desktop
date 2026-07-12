import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const page = await readFile("src/features/collectors/CollectorsPage.tsx", "utf8");
const panel = await readFile(
  "src/features/collectors/CollectorAdvancedSettings.tsx",
  "utf8",
);

assert.match(page, /<CollectorAdvancedSettings \/>/);
assert.match(panel, /title="采集调度"/);
assert.match(panel, /及时/);
assert.match(panel, /均衡/);
assert.match(panel, /节省资源/);
assert.match(panel, /自定义周期与执行参数/);
assert.match(panel, /余额周期/);
assert.match(panel, /分组 \/ 倍率周期/);
assert.match(panel, /模型周期/);
assert.match(panel, /价格周期/);
assert.match(panel, /采集超时/);
assert.match(panel, /采集并发数/);
assert.match(panel, /保存采集设置/);
assert.match(panel, /恢复推荐值/);

console.log("collector settings layout contract ok");
