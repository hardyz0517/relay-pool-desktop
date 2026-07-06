import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const addProviderSource = await readFile("src/features/stations/AddProviderPage.tsx", "utf8");
const stationKeysApiSource = await readFile("src/lib/api/stationKeys.ts", "utf8");
const commandsSource = await readFile("src-tauri/src/commands/mod.rs", "utf8");
const tauriLibSource = await readFile("src-tauri/src/lib.rs", "utf8");
const remoteListSource = await readFile(
  "src/features/stations/components/RemoteKeyDiscoveryList.tsx",
  "utf8",
);

assert.ok(
  remoteListSource.includes("作为本地秘钥"),
  "remote discovery table should expose a local-key toggle column",
);

assert.ok(
  remoteListSource.includes("SwitchControl"),
  "remote discovery rows should use the shared switch control for local-key toggles",
);

assert.ok(
  remoteListSource.includes("onLocalKeyToggle"),
  "remote discovery list should delegate local-key toggle behavior to the page owner",
);

assert.match(
  addProviderSource,
  /const \[remoteCreatedLocalKeyIds, setRemoteCreatedLocalKeyIds\] = useState<Record<string, string>>\(\{\}\)/,
  "page state should remember which local key was created by each remote row",
);

assert.match(
  addProviderSource,
  /async function handleRemoteLocalKeyToggle\(\s*remoteKey: RemoteStationKey,\s*checked: boolean,\s*\)/,
  "page should implement an explicit remote-local toggle handler",
);

assert.match(
  addProviderSource,
  /deleteRemoteCreatedLocalKey\(\s*remoteKey,\s*createdLocalKeyId,\s*\)/,
  "turning the switch off should delete only the local key id previously created by that remote row",
);

assert.ok(
  addProviderSource.includes("if (remoteKey.matchedStationKeyId && remoteKey.matchedStationKeyId !== expectedStationKeyId)"),
  "delete guard should refuse to remove a different manually matched local key",
);

assert.ok(
  stationKeysApiSource.includes("unbindRemoteStationKey"),
  "frontend API should expose remote-key unbind before deleting an auto-created local key",
);

assert.ok(
  commandsSource.includes("pub fn unbind_remote_station_key"),
  "Tauri commands should expose remote-key unbind",
);

assert.ok(
  tauriLibSource.includes("commands::unbind_remote_station_key"),
  "Tauri invoke handler should register remote-key unbind",
);
