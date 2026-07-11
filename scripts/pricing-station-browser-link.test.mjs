import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const pricingPageSource = await readFile("src/features/pricing/PricingPage.tsx", "utf8");

assert.match(
  pricingPageSource,
  /import \{[^}]*openStationBaseUrl[^}]*\} from "@\/lib\/api\/stations"/,
  "pricing page should reuse the validated external URL opener",
);

assert.match(
  pricingPageSource,
  /const stationBaseUrls = useMemo\([\s\S]*new Map\(stations\.map\(\(station\) => \[station\.id, station\.baseUrl\]\)\)[\s\S]*\[stations\]/,
  "pricing rows should map station ids to their original configured base URLs",
);

assert.match(
  pricingPageSource,
  /<button[\s\S]*?aria-label=\{`在浏览器打开 \$\{row\.stationName\}`\}[\s\S]*?onClick=\{\(\) => onOpenStation\(row\.stationId, row\.stationName\)\}[\s\S]*?>[\s\S]*?\{row\.stationName\}[\s\S]*?<\/button>/,
  "station names in pricing rows should be accessible buttons",
);

assert.match(
  pricingPageSource,
  /await openStationBaseUrl\(baseUrl\)[\s\S]*toast\.error\("打开中转站网址失败", readError\(error\)\)/,
  "clicking a station name should open its original URL and report failures",
);

console.log("pricing station browser link checks passed");
