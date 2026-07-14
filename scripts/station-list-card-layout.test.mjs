import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

const source = await readFile("src/features/stations/StationsPage.tsx", "utf8");
const rowStart = source.indexOf("function StationAssetListRow");
const rowEnd = source.indexOf("function supportsManualAuthorization", rowStart);

assert.ok(rowStart >= 0 && rowEnd > rowStart, "StationAssetListRow source should be present");

const rowSource = source.slice(rowStart, rowEnd);

assert.match(
  rowSource,
  /<GripVertical className="h-4 w-4" \/>/,
  "station asset rows should keep the drag handle visible",
);
assert.match(
  rowSource,
  /stationAvatarLabel\(station\.name\)/,
  "station asset rows should keep the station avatar label",
);
assert.match(
  rowSource,
  /className="truncate text-\[15px\] font-semibold leading-5 text-foreground">\{station\.name\}<\/div>/,
  "station asset rows should render the station name as the primary title",
);
assert.match(
  rowSource,
  /stationTypeLabels\[station\.stationType\]/,
  "station asset rows should keep the station type badge next to the title",
);
assert.match(
  rowSource,
  /issueTags\.map\(\(tag\) =>/,
  "station asset rows should render issue tags next to the title",
);
assert.match(
  rowSource,
  /formatStationDisplayUrl\(station\.websiteUrl\)/,
  "station asset rows should render the website URL as secondary metadata",
);

console.log("station list card layout contract ok");
