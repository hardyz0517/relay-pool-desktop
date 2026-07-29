import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const stationsPageSource = await readFile("src/features/stations/StationsPage.tsx", "utf8");
const stationsApiSource = await readFile("src/lib/api/stations.ts", "utf8");
const commandsSource = await readFile("src-tauri/src/commands/mod.rs", "utf8");
const registrySource = await readFile("src-tauri/src/ipc/registry.rs", "utf8");

assert.match(
  stationsPageSource,
  /import \{[^}]*openStationWebsite[^}]*\} from "@\/lib\/api\/stations"/,
  "station row should use the stations API boundary for opening website URLs",
);

assert.match(
  stationsPageSource,
  /<button[\s\S]*?type="button"[\s\S]*?aria-label=\{`在浏览器打开 \$\{station\.name\}`\}[\s\S]*?onClick=\{\(event\) => \{[\s\S]*?event\.stopPropagation\(\);[\s\S]*?void openStationWebsite\(station\.websiteUrl\);[\s\S]*?\}\}/,
  "station row website URL should be a button-like link that opens externally without triggering row details",
);

assert.match(
  stationsPageSource,
  /onKeyDown=\{\(event\) => event\.stopPropagation\(\)\}/,
  "station row base URL keyboard events should not bubble to the row details handler",
);

assert.doesNotMatch(
  stationsPageSource,
  /target="_blank"/,
  "station row base URL should not rely on WebView target=_blank behavior",
);

assert.match(
  stationsApiSource,
  /export function openStationWebsite\(url: string\)[\s\S]*openExternalUrlBinding\(\{ url \}\)/,
  "stations API should call the generated external URL opener wrapper",
);

assert.match(
  commandsSource,
  /#\[tauri::command\]\s*pub async fn open_external_url\(input: Value\) -> Result<\(\), error::CommandError>/,
  "backend should expose a typed external URL opener command",
);

assert.match(
  commandsSource,
  /OpenExternalUrlInputDto::parse\(input\)\?[\s\S]*validate_external_http_url\(&input\.url\)\?/,
  "backend external URL opener should validate station URLs before launching",
);

assert.match(
  registrySource,
  /open_external_url => \$crate::commands::open_external_url/,
  "Tauri command registry should register the external URL opener command",
);
