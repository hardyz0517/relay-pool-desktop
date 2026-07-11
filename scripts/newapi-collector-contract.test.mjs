import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const stationKeysApiSource = await readFile("src/lib/api/stationKeys.ts", "utf8");
const addProviderSource = await readFile("src/features/stations/AddProviderPage.tsx", "utf8");

assert.ok(
  !stationKeysApiSource.includes("NewAPI 远端 Key 管理尚未适配"),
  "browser preview fallback must not describe NewAPI remote-key management as unsupported",
);

assert.match(
  stationKeysApiSource,
  /station\.stationType === "newapi"[\s\S]*?canListRemoteKeys:\s*true[\s\S]*?canCreateRemoteKey:\s*true[\s\S]*?unsupportedReason:\s*null/,
  "browser preview fallback should advertise NewAPI list/create remote-key capability",
);

assert.match(
  stationKeysApiSource,
  /message:\s*"浏览器预览模式：已创建本地临时密钥，真实远端创建将在桌面端执行。"[\s\S]*?fullKeyOnce:\s*null/,
  "browser preview create fallback should not return fullKeyOnce to the frontend",
);

assert.ok(
  addProviderSource.includes("newapiRemoteCreateConfirm") &&
    addProviderSource.includes("NewAPI 创建远端 Key 后不会在界面展示完整 Key"),
  "NewAPI remote-key creation should require an explicit truthfulness confirmation",
);
