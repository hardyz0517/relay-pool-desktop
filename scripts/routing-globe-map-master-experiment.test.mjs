import assert from "node:assert/strict";
import { access, readFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const scriptDir = dirname(fileURLToPath(import.meta.url));
const root = join(scriptDir, "..", "docs", "assets", "routing-globe-map-master-experiment");
const manifest = JSON.parse(await readFile(join(root, "manifest.json"), "utf8"));
const map = JSON.parse(await readFile(join(root, "preview-world-map.json"), "utf8"));
const generator = await readFile(join(scriptDir, "generate-routing-globe-map-master-experiment.mjs"), "utf8");
const previewPage = await readFile(join(root, "routing-globe-map-master-preview.html"), "utf8");
const eastAsiaComparison = await readFile(join(root, "routing-globe-east-asia-source-vs-preview.svg"), "utf8");

const expectedOutputs = [
  "routing-globe-source-outlines.svg",
  "routing-globe-map-master-preview.svg",
  "routing-globe-source-vs-preview-overlay.svg",
  "routing-globe-east-asia-source-vs-preview.svg",
  "routing-globe-map-master-preview.html",
  "preview-world-map.json",
];
const obsoleteOutputs = [
  "routing-globe-east-asia-preview.svg",
  "routing-globe-map-master-24.svg",
  "routing-globe-map-master-32.svg",
  "routing-globe-map-master.svg",
  "routing-globe-scale-preview.svg",
  "routing-globe-single-frame-light.png",
  "routing-globe-source-vs-simplified.svg",
  "simplified-world-map.json",
];
const requiredFeatureBounds = new Map([
  ["Taiwan", [119.8, 122.2, 21.7, 25.5]],
  ["Japan", [129, 146, 30, 46]],
  ["Philippines", [116, 128, 4, 22]],
  ["United Kingdom", [-12, 4, 48, 63]],
  ["New Zealand", [165, 180, -48, -33]],
]);

const ringBounds = (ring) => [
  Math.min(...ring.map(([longitude]) => longitude)),
  Math.max(...ring.map(([longitude]) => longitude)),
  Math.min(...ring.map(([, latitude]) => latitude)),
  Math.max(...ring.map(([, latitude]) => latitude)),
];

const ringCenterWithinBounds = (ring, bounds) => {
  const [minLongitude, maxLongitude, minLatitude, maxLatitude] = ringBounds(ring);
  const centerLongitude = (minLongitude + maxLongitude) / 2;
  const centerLatitude = (minLatitude + maxLatitude) / 2;
  return centerLongitude >= bounds[0]
    && centerLongitude <= bounds[1]
    && centerLatitude >= bounds[2]
    && centerLatitude <= bounds[3];
};

assert.equal(manifest.stage, "preview-master-only");
assert.equal(manifest.source, "natural-earth-land-50m.geojson");
assert.equal(manifest.coordinateSystem, "equirectangular / plate carree");
assert.equal(manifest.cleanup.simplificationToleranceDegrees, 0.045);
assert.equal(manifest.cleanup.globalMinimumAreaDegrees, 0.018);
assert.equal(manifest.cleanup.keyRegionMinimumAreaDegrees, 0.0045);
assert.deepEqual(manifest.outputs, expectedOutputs);

assert.equal(map.coordinateSystem, manifest.coordinateSystem);
assert.equal(map.source, manifest.source);
assert.equal(map.sourceLicense, "Public domain");
assert.deepEqual(map.cleanup, manifest.cleanup);
assert.deepEqual(map.requiredFeatures, [...requiredFeatureBounds.keys()]);
assert.ok(map.sourceRingCount > 0);
assert.ok(map.sourcePointCount > 0);
assert.ok(map.previewRingCount / map.sourceRingCount > 0.65, "Preview master removed too many land rings");
assert.ok(map.previewPointCount / map.sourcePointCount > 0.3, "Preview master simplified the coastline too aggressively");
assert.ok(map.previewPointCount < map.sourcePointCount, "Preview master should still perform light cleanup");
assert.equal(map.polygons.length, map.previewRingCount);

for (const ring of map.polygons) {
  assert.ok(ring.length >= 4);
  assert.deepEqual(ring[0], ring.at(-1), "Every land ring must remain closed");
  for (const [longitude, latitude] of ring) {
    assert.ok(Number.isFinite(longitude));
    assert.ok(Number.isFinite(latitude));
    assert.ok(longitude >= -180 && longitude <= 180);
    assert.ok(latitude > -60 && latitude <= 90);
  }
}

for (const [name, bounds] of requiredFeatureBounds) {
  assert.ok(
    map.polygons.some((ring) => ringCenterWithinBounds(ring, bounds)),
    `${name} must remain represented by an independent land ring`,
  );
}

for (const name of ["Taiwan", "Japan", "Philippines"]) {
  assert.match(eastAsiaComparison, new RegExp(name));
}

assert.match(generator, /natural-earth-land-50m\.geojson/);
assert.doesNotMatch(generator, /natural-earth-admin0-countries-50m\.geojson|natural-earth-land-110m\.geojson/);
assert.doesNotMatch(generator, /renderLandMask|rotateY\s*\(|scaleX\s*\(|setInterval|requestAnimationFrame|three\.js|WebGL/i);
assert.match(previewPage, /Natural Earth 1:50m source outlines/);
assert.match(previewPage, /New lightly cleaned preview master/);
assert.match(previewPage, /Source and preview master overlay/);
assert.match(previewPage, /East Asia detail comparison/);
assert.doesNotMatch(previewPage, /24px icon master|32px icon master|@keyframes|animation\s*:/i);

for (const filename of manifest.outputs) await access(join(root, filename));
await access(join(root, manifest.source));
for (const filename of obsoleteOutputs) {
  await assert.rejects(access(join(root, filename)), { code: "ENOENT" });
}

console.log("routing globe preview master assets passed");
