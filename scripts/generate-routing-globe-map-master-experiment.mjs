import { mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const scriptDir = dirname(fileURLToPath(import.meta.url));
const projectRoot = dirname(scriptDir);
const outputDir = join(projectRoot, "docs", "assets", "routing-globe-map-master-experiment");
const landSourceName = "natural-earth-land-50m.geojson";
const landSourcePath = join(outputDir, landSourceName);

const WORLD_BOUNDS = [-180, 180, -60, 90];
const EAST_ASIA_BOUNDS = [95, 150, -12, 52];
const CLEANUP = {
  simplificationToleranceDegrees: 0.045,
  globalMinimumAreaDegrees: 0.018,
  keyRegionMinimumAreaDegrees: 0.0045,
};
const KEY_REGIONS = [
  { name: "East and Southeast Asia", bounds: [95, 150, -12, 52] },
  { name: "Mediterranean", bounds: [-7, 37, 30, 47] },
  { name: "British Isles", bounds: [-12, 4, 48, 63] },
  { name: "New Zealand", bounds: [165, 180, -48, -33] },
];
const REQUIRED_FEATURES = [
  { name: "Taiwan", bounds: [119.8, 122.2, 21.7, 25.5] },
  { name: "Japan", bounds: [129, 146, 30, 46] },
  { name: "Philippines", bounds: [116, 128, 4, 22] },
  { name: "United Kingdom", bounds: [-12, 4, 48, 63] },
  { name: "New Zealand", bounds: [165, 180, -48, -33] },
];
const OBSOLETE_OUTPUTS = [
  "routing-globe-east-asia-preview.svg",
  "routing-globe-map-master-24.svg",
  "routing-globe-map-master-32.svg",
  "routing-globe-map-master.svg",
  "routing-globe-scale-preview.svg",
  "routing-globe-single-frame-dark.png",
  "routing-globe-single-frame-dark.svg",
  "routing-globe-single-frame-icon24-dark.png",
  "routing-globe-single-frame-icon24-dark.svg",
  "routing-globe-single-frame-icon24-light.png",
  "routing-globe-single-frame-icon24-light.svg",
  "routing-globe-single-frame-icon32-dark.png",
  "routing-globe-single-frame-icon32-dark.svg",
  "routing-globe-single-frame-icon32-light.png",
  "routing-globe-single-frame-icon32-light.svg",
  "routing-globe-single-frame-light.png",
  "routing-globe-single-frame-light.svg",
  "routing-globe-single-frame-preview-dark.png",
  "routing-globe-single-frame-preview-dark.svg",
  "routing-globe-single-frame-preview-light.png",
  "routing-globe-single-frame-preview-light.svg",
  "routing-globe-source-vs-simplified.svg",
  "simplified-world-map.json",
];

const planarArea = (ring) => {
  let area = 0;
  for (let index = 0, previous = ring.length - 1; index < ring.length; previous = index++) {
    area += ring[previous][0] * ring[index][1] - ring[index][0] * ring[previous][1];
  }
  return Math.abs(area) / 2;
};

const ringBounds = (ring) => [
  Math.min(...ring.map(([longitude]) => longitude)),
  Math.max(...ring.map(([longitude]) => longitude)),
  Math.min(...ring.map(([, latitude]) => latitude)),
  Math.max(...ring.map(([, latitude]) => latitude)),
];

const intersectsBounds = (ring, bounds) => {
  const [minLongitude, maxLongitude, minLatitude, maxLatitude] = ringBounds(ring);
  return maxLongitude >= bounds[0] && minLongitude <= bounds[1] && maxLatitude >= bounds[2] && minLatitude <= bounds[3];
};

const ringCenterWithinBounds = (ring, bounds) => {
  const [minLongitude, maxLongitude, minLatitude, maxLatitude] = ringBounds(ring);
  const centerLongitude = (minLongitude + maxLongitude) / 2;
  const centerLatitude = (minLatitude + maxLatitude) / 2;
  return centerLongitude >= bounds[0]
    && centerLongitude <= bounds[1]
    && centerLatitude >= bounds[2]
    && centerLatitude <= bounds[3];
};

const outerRings = (geometry) => {
  if (geometry.type === "Polygon") return [geometry.coordinates[0]];
  if (geometry.type === "MultiPolygon") return geometry.coordinates.map((polygon) => polygon[0]);
  return [];
};

const pointLineDistance = (point, start, end) => {
  const dx = end[0] - start[0];
  const dy = end[1] - start[1];
  if (dx === 0 && dy === 0) return Math.hypot(point[0] - start[0], point[1] - start[1]);
  const ratio = Math.max(0, Math.min(1, ((point[0] - start[0]) * dx + (point[1] - start[1]) * dy) / (dx * dx + dy * dy)));
  return Math.hypot(point[0] - start[0] - ratio * dx, point[1] - start[1] - ratio * dy);
};

const simplifyRing = (ring, tolerance) => {
  const source = ring.length > 1 && ring[0][0] === ring.at(-1)[0] && ring[0][1] === ring.at(-1)[1] ? ring.slice(0, -1) : ring;
  if (source.length < 4) return [...source, source[0]];
  const simplifyOpen = (points) => {
    if (points.length < 3) return points;
    let maximumDistance = 0;
    let splitIndex = 0;
    for (let index = 1; index < points.length - 1; index += 1) {
      const distance = pointLineDistance(points[index], points[0], points.at(-1));
      if (distance > maximumDistance) {
        maximumDistance = distance;
        splitIndex = index;
      }
    }
    if (maximumDistance <= tolerance) return [points[0], points.at(-1)];
    return [...simplifyOpen(points.slice(0, splitIndex + 1)).slice(0, -1), ...simplifyOpen(points.slice(splitIndex))];
  };
  const simplified = simplifyOpen([...source, source[0]]).slice(0, -1);
  simplified.push(simplified[0]);
  return simplified;
};

const effectiveMinimumArea = (ring) => KEY_REGIONS.some(({ bounds }) => intersectsBounds(ring, bounds))
  ? CLEANUP.keyRegionMinimumAreaDegrees
  : CLEANUP.globalMinimumAreaDegrees;

const buildPreviewMaster = (sourceRings) => sourceRings
  .filter((ring) => ringBounds(ring)[3] > -60)
  .filter((ring) => planarArea(ring) >= effectiveMinimumArea(ring))
  .map((ring) => simplifyRing(ring, CLEANUP.simplificationToleranceDegrees))
  .filter((ring) => ring.length >= 4);

const toSvgPath = (ring, width, height, bounds) => `${ring.map(([longitude, latitude], index) => {
  const x = ((longitude - bounds[0]) / (bounds[1] - bounds[0])) * width;
  const y = ((bounds[3] - latitude) / (bounds[3] - bounds[2])) * height;
  return `${index === 0 ? "M" : "L"}${x.toFixed(2)} ${y.toFixed(2)}`;
}).join(" ")} Z`;

const pathsFor = (rings, width, height, bounds, attributes = "") => rings
  .filter((ring) => intersectsBounds(ring, bounds))
  .map((ring) => `<path ${attributes}d="${toSvgPath(ring, width, height, bounds)}"/>`)
  .join("");

const worldMapSvg = (rings, title, mode) => {
  const width = 1440;
  const height = 600;
  const paths = pathsFor(rings, width, height, WORLD_BOUNDS);
  const layer = mode === "source"
    ? `<g fill="#9fc7bc" stroke="#47766c" stroke-width="0.72" stroke-linejoin="round">${paths}</g>`
    : `<g fill="#439969">${paths}</g>`;
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}" role="img" aria-label="${title}"><rect width="${width}" height="${height}" fill="#e2f2f3"/>${layer}<text x="24" y="32" font-family="system-ui,sans-serif" font-size="18" fill="#284c45">${title}</text></svg>\n`;
};

const overlaySvg = (sourceRings, previewRings) => {
  const width = 1440;
  const height = 600;
  const sourcePaths = pathsFor(sourceRings, width, height, WORLD_BOUNDS);
  const previewPaths = pathsFor(previewRings, width, height, WORLD_BOUNDS);
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}" role="img" aria-label="Natural Earth 1:50m source and lightly cleaned preview master overlay"><rect width="${width}" height="${height}" fill="#e2f2f3"/><g fill="#92b9b0" opacity=".72">${sourcePaths}</g><g fill="#258b65" fill-opacity=".72" stroke="#176b50" stroke-width="0.4">${previewPaths}</g><g font-family="system-ui,sans-serif"><text x="24" y="32" font-size="18" fill="#284c45">Source / preview master overlay</text><rect x="1110" y="17" width="14" height="14" fill="#92b9b0"/><text x="1132" y="29" font-size="13" fill="#365d55">source</text><rect x="1210" y="17" width="14" height="14" fill="#258b65" fill-opacity=".72" stroke="#176b50" stroke-width=".4"/><text x="1232" y="29" font-size="13" fill="#365d55">preview master</text></g></svg>\n`;
};

const eastAsiaComparisonSvg = (sourceRings, previewRings) => {
  const width = 1440;
  const height = 760;
  const headerHeight = 56;
  const cellWidth = width / 2;
  const mapHeight = height - headerHeight;
  const sourcePaths = pathsFor(sourceRings, cellWidth, mapHeight, EAST_ASIA_BOUNDS);
  const previewPaths = pathsFor(previewRings, cellWidth, mapHeight, EAST_ASIA_BOUNDS);
  const featureBoxes = REQUIRED_FEATURES.filter(({ bounds }) => bounds[0] >= EAST_ASIA_BOUNDS[0]).map(({ name, bounds }) => {
    const x = ((bounds[0] - EAST_ASIA_BOUNDS[0]) / (EAST_ASIA_BOUNDS[1] - EAST_ASIA_BOUNDS[0])) * cellWidth;
    const y = ((EAST_ASIA_BOUNDS[3] - bounds[3]) / (EAST_ASIA_BOUNDS[3] - EAST_ASIA_BOUNDS[2])) * mapHeight;
    const featureWidth = ((bounds[1] - bounds[0]) / (EAST_ASIA_BOUNDS[1] - EAST_ASIA_BOUNDS[0])) * cellWidth;
    const featureHeight = ((bounds[3] - bounds[2]) / (EAST_ASIA_BOUNDS[3] - EAST_ASIA_BOUNDS[2])) * mapHeight;
    return `<rect x="${x.toFixed(2)}" y="${y.toFixed(2)}" width="${featureWidth.toFixed(2)}" height="${featureHeight.toFixed(2)}" rx="2" fill="none" stroke="#d36f36" stroke-width="1.4"/><text x="${(x + featureWidth + 6).toFixed(2)}" y="${(y + 14).toFixed(2)}" font-family="system-ui,sans-serif" font-size="12" fill="#9b4d27">${name}</text>`;
  }).join("");
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}" role="img" aria-label="East Asia source and preview master comparison"><rect width="${width}" height="${height}" fill="#f5f8f7"/><text x="24" y="34" font-family="system-ui,sans-serif" font-size="18" fill="#284c45">East Asia detail comparison</text><g transform="translate(0 ${headerHeight})"><defs><clipPath id="source-clip"><rect width="${cellWidth}" height="${mapHeight}"/></clipPath><clipPath id="preview-clip"><rect width="${cellWidth}" height="${mapHeight}"/></clipPath></defs><rect width="${cellWidth}" height="${mapHeight}" fill="#e2f2f3"/><g clip-path="url(#source-clip)" fill="#9fc7bc" stroke="#47766c" stroke-width=".65">${sourcePaths}</g><g>${featureBoxes}</g><rect x="${cellWidth}" width="${cellWidth}" height="${mapHeight}" fill="#e2f2f3"/><g transform="translate(${cellWidth} 0)" clip-path="url(#preview-clip)" fill="#439969">${previewPaths}${featureBoxes}</g><line x1="${cellWidth}" y1="0" x2="${cellWidth}" y2="${mapHeight}" stroke="#b7cfca"/><text x="20" y="30" font-family="system-ui,sans-serif" font-size="16" fill="#284c45">Natural Earth 1:50m source</text><text x="${cellWidth + 20}" y="30" font-family="system-ui,sans-serif" font-size="16" fill="#284c45">Lightly cleaned preview master</text></g></svg>\n`;
};

const previewHtml = `<!doctype html><meta charset="utf-8"/><title>Routing globe flat preview master</title><style>:root{font-family:system-ui,sans-serif;color:#20312f;background:#eef3f2}body{margin:0;padding:28px}h1{margin:0 0 6px;font-size:20px}p{margin:0 0 24px;color:#536966;font-size:13px;max-width:900px}section{margin:0 0 26px}h2{margin:0 0 10px;font-size:14px}.map{display:block;width:min(1440px,100%);height:auto;border:1px solid #cadeda;background:#e2f2f3}</style><h1>Routing globe flat preview master</h1><p>Preview master is rebuilt directly from Natural Earth 1:50m land polygons. It receives only light coastline simplification and a small-island noise filter. No icon master, projection frame, animation, or route UI is generated in this checkpoint.</p><section><h2>1. Natural Earth 1:50m source outlines</h2><img class="map" src="./routing-globe-source-outlines.svg" alt="Natural Earth 1:50m source outlines"/></section><section><h2>2. New lightly cleaned preview master</h2><img class="map" src="./routing-globe-map-master-preview.svg" alt="Lightly cleaned flat equirectangular preview master"/></section><section><h2>3. Source and preview master overlay</h2><img class="map" src="./routing-globe-source-vs-preview-overlay.svg" alt="Source and preview master overlay"/></section><section><h2>4. East Asia detail comparison</h2><img class="map" src="./routing-globe-east-asia-source-vs-preview.svg" alt="East Asia source and preview master comparison"/></section>`;

const landGeojson = JSON.parse(await readFile(landSourcePath, "utf8"));
const sourceRings = landGeojson.features.flatMap((feature) => outerRings(feature.geometry)).filter((ring) => ringBounds(ring)[3] > -60);
const previewRings = buildPreviewMaster(sourceRings);
const sourcePointCount = sourceRings.reduce((total, ring) => total + ring.length, 0);
const previewPointCount = previewRings.reduce((total, ring) => total + ring.length, 0);

for (const feature of REQUIRED_FEATURES) {
  if (!previewRings.some((ring) => ringCenterWithinBounds(ring, feature.bounds) && planarArea(ring) >= CLEANUP.keyRegionMinimumAreaDegrees)) {
    throw new Error(`Required preview feature is missing: ${feature.name}`);
  }
}

const previewMap = {
  description: "Lightly cleaned flat master derived directly from Natural Earth 1:50m land polygons.",
  coordinateSystem: "equirectangular / plate carree",
  source: landSourceName,
  sourceLicense: "Public domain",
  cleanup: CLEANUP,
  keyRegions: KEY_REGIONS,
  requiredFeatures: REQUIRED_FEATURES.map(({ name }) => name),
  sourceRingCount: sourceRings.length,
  sourcePointCount,
  previewRingCount: previewRings.length,
  previewPointCount,
  polygons: previewRings,
};
const manifest = {
  stage: "preview-master-only",
  source: landSourceName,
  coordinateSystem: "equirectangular / plate carree",
  cleanup: CLEANUP,
  outputs: [
    "routing-globe-source-outlines.svg",
    "routing-globe-map-master-preview.svg",
    "routing-globe-source-vs-preview-overlay.svg",
    "routing-globe-east-asia-source-vs-preview.svg",
    "routing-globe-map-master-preview.html",
    "preview-world-map.json",
  ],
};
const readme = `# Routing globe preview master experiment

This checkpoint starts from Natural Earth 1:50m land polygons and rebuilds only the flat Preview master.

- The source outline view uses all non-Antarctic 1:50m land rings.
- The Preview master uses a light 0.045 degree Douglas-Peucker cleanup.
- Only very small noise rings are removed. East/Southeast Asia, the Mediterranean, the British Isles, and New Zealand use a lower small-island threshold.
- Taiwan, Japan, the Philippines, the United Kingdom, and New Zealand are asserted as present.
- No 24px/32px icon master, orthographic frame, animation, or route UI is generated here.
`;

await mkdir(outputDir, { recursive: true });
await Promise.all(OBSOLETE_OUTPUTS.map((filename) => rm(join(outputDir, filename), { force: true })));
await Promise.all([
  writeFile(join(outputDir, "routing-globe-source-outlines.svg"), worldMapSvg(sourceRings, "Natural Earth 1:50m source outlines", "source"), "utf8"),
  writeFile(join(outputDir, "routing-globe-map-master-preview.svg"), worldMapSvg(previewRings, "Preview master - lightly cleaned Natural Earth 1:50m land", "preview"), "utf8"),
  writeFile(join(outputDir, "routing-globe-source-vs-preview-overlay.svg"), overlaySvg(sourceRings, previewRings), "utf8"),
  writeFile(join(outputDir, "routing-globe-east-asia-source-vs-preview.svg"), eastAsiaComparisonSvg(sourceRings, previewRings), "utf8"),
  writeFile(join(outputDir, "routing-globe-map-master-preview.html"), previewHtml, "utf8"),
  writeFile(join(outputDir, "preview-world-map.json"), `${JSON.stringify(previewMap, null, 2)}\n`, "utf8"),
  writeFile(join(outputDir, "manifest.json"), `${JSON.stringify(manifest, null, 2)}\n`, "utf8"),
  writeFile(join(outputDir, "README.md"), readme, "utf8"),
]);

console.log(`Preview master: ${sourceRings.length} source rings / ${sourcePointCount} points -> ${previewRings.length} rings / ${previewPointCount} points`);
