import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const source = await readFile("src/features/stations/AddProviderPage.tsx", "utf8");

assert.match(
  source,
  /import \{[^}]*startManualAuthorization[^}]*\} from "@\/lib\/api\/collector"/s,
  "edit provider page should reuse the existing manual web authorization API",
);

assert.match(
  source,
  /const \[startingAuthorization, setStartingAuthorization\] = useState\(false\)/,
  "edit provider page should guard the authorization popup against repeated clicks",
);

assert.match(
  source,
  /async function handleStartManualAuthorization\(\)[\s\S]*await startManualAuthorization\(activeStationId\)/,
  "manual authorization button should open the saved station capture popup",
);

assert.match(
  source,
  /\{editing && \([\s\S]*网页登录授权[\s\S]*\)\}/,
  "manual authorization button should only be rendered on the edit provider page",
);

assert.match(
  source,
  /md:grid-cols-\[minmax\(0,1fr\)_minmax\(0,1fr\)_auto_auto\]/,
  "login credential row should reserve room for both test and web authorization buttons",
);

assert.match(
  source,
  /md:grid-cols-\[minmax\(0,1fr\)_minmax\(0,1fr\)_auto\][\s\S]*复制前端网址/,
  "website and API URL row should include the compact copy button in the same row",
);

assert.ok(
  !/mt-2 flex justify-end[\s\S]{0,240}复制前端网址/.test(source),
  "copy website button should not be pushed onto its own row",
);

console.log("edit provider manual authorization source guard passed");
