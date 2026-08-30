import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { access, readFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { inflateSync } from "node:zlib";

const scriptDir = dirname(fileURLToPath(import.meta.url));
const projectRoot = join(scriptDir, "..");
const root = join(projectRoot, "docs", "assets", "routing-globe-axis-tilt-experiment");
const mapPath = join(projectRoot, "docs", "assets", "routing-globe-map-master-experiment", "preview-world-map.json");
const phase3Manifest = JSON.parse(await readFile(join(projectRoot, "docs", "assets", "routing-globe-projection-experiment", "manifest.json"), "utf8"));
const manifest = JSON.parse(await readFile(join(root, "manifest.json"), "utf8"));
const generator = await readFile(join(scriptDir, "generate-routing-globe-axis-tilt-experiment.mjs"), "utf8");
const preview = await readFile(join(root, "routing-globe-axis-tilt-preview.html"), "utf8");
const mapBytes = await readFile(mapPath);
const expectedMapSha256 = createHash("sha256").update(mapBytes).digest("hex");

const frozenParameterNames = [
  "sourceMapSha256",
  "projection",
  "frameCount",
  "frameStepDegrees",
  "loopDurationMs",
  "baseCenterLongitude",
  "centerLatitude",
  "radiusRatio",
  "inspectionSize",
  "iconSizes",
  "supersample",
];
const candidates = [18, 20, 23.5];
const sizes = [32, 24];
const themes = ["light", "dark"];

const normalizeLongitude = (longitude) => {
  let value = longitude;
  while (value <= -180) value += 360;
  while (value > 180) value -= 360;
  return value;
};

const readPng = async (filename) => {
  const png = await readFile(join(root, filename));
  assert.deepEqual([...png.subarray(0, 8)], [137, 80, 78, 71, 13, 10, 26, 10]);
  const width = png.readUInt32BE(16);
  const height = png.readUInt32BE(20);
  const idat = [];
  for (let offset = 8; offset < png.length;) {
    const length = png.readUInt32BE(offset);
    const type = png.toString("ascii", offset + 4, offset + 8);
    if (type === "IDAT") idat.push(png.subarray(offset + 8, offset + 8 + length));
    offset += length + 12;
  }
  const scanlines = inflateSync(Buffer.concat(idat));
  const rgba = Buffer.alloc(width * height * 4);
  for (let y = 0; y < height; y += 1) {
    const rowStart = y * (width * 4 + 1);
    assert.equal(scanlines[rowStart], 0, `${filename} uses an unexpected PNG filter`);
    scanlines.copy(rgba, y * width * 4, rowStart + 1, rowStart + 1 + width * 4);
  }
  return { png, width, height, rgba };
};

const alphaChannel = (rgba) => {
  const alpha = Buffer.alloc(rgba.length / 4);
  for (let pixel = 0; pixel < alpha.length; pixel += 1) alpha[pixel] = rgba[pixel * 4 + 3];
  return alpha;
};

assert.equal(manifest.stage, "axis-tilt-comparison");
assert.equal(manifest.sourceMapSha256, expectedMapSha256);
assert.deepEqual(manifest.axialTiltCandidatesDegrees, candidates);
assert.equal(manifest.northDirection, "left-up");
assert.equal(manifest.runtime, "individual-frame-preview-only");
for (const name of frozenParameterNames) assert.deepEqual(manifest[name], phase3Manifest[name], `Frozen Phase 3 parameter changed: ${name}`);
assert.equal(manifest.variants.length, candidates.length);

const referenceAlphaBySize = new Map();
const singleFrameHashes = new Set();
for (const [variantIndex, variant] of manifest.variants.entries()) {
  assert.equal(variant.axialTiltDegrees, candidates[variantIndex]);
  assert.equal(variant.orientation, "fixed screen-space roll before inverse orthographic sampling");
  assert.ok(variant.northAxisScreen.x < 0, `${variant.axialTiltDegrees} degree north axis must point left`);
  assert.ok(variant.northAxisScreen.y > 0, `${variant.axialTiltDegrees} degree north axis must point up`);
  assert.equal(variant.northAxisScreen.z, manifest.variants[0].northAxisScreen.z);
  if (variantIndex > 0) {
    assert.ok(Math.abs(variant.northAxisScreen.x) > Math.abs(manifest.variants[variantIndex - 1].northAxisScreen.x));
    assert.ok(variant.northAxisScreen.y < manifest.variants[variantIndex - 1].northAxisScreen.y);
  }

  for (const theme of themes) {
    const inspection = await readPng(variant.inspection[theme]);
    assert.equal(inspection.width, 256);
    assert.equal(inspection.height, 256);
    const alpha = alphaChannel(inspection.rgba);
    if (!referenceAlphaBySize.has(256)) referenceAlphaBySize.set(256, alpha);
    assert.deepEqual(alpha, referenceAlphaBySize.get(256), `${variant.inspection[theme]} changed the sphere shell`);
    if (theme === "light") singleFrameHashes.add(createHash("sha256").update(inspection.png).digest("hex"));
  }

  assert.equal(variant.frames.length, 16);
  for (const frame of variant.frames) {
    assert.equal(frame.centerLongitude, normalizeLongitude(122 + frame.index * 22.5));
    for (const size of sizes) {
      for (const theme of themes) {
        const image = await readPng(frame.files[String(size)][theme]);
        assert.equal(image.width, size);
        assert.equal(image.height, size);
        const alpha = alphaChannel(image.rgba);
        if (!referenceAlphaBySize.has(size)) referenceAlphaBySize.set(size, alpha);
        assert.deepEqual(alpha, referenceAlphaBySize.get(size), `${frame.files[String(size)][theme]} changed the sphere shell`);
      }
    }
  }
}
assert.equal(singleFrameHashes.size, candidates.length, "All axial-tilt candidates rendered the same orientation");

assert.equal(manifest.outputs.length, 1 + candidates.length * (2 + 16 * sizes.length * themes.length));
for (const filename of manifest.outputs) await access(join(root, filename));
assert.ok(manifest.outputs.every((filename) => !/sprite/i.test(filename)));

assert.match(generator, /const orientedX = normalizedX \* cosineRoll \+ normalizedY \* sineRoll/);
assert.match(generator, /const orientedY = -normalizedX \* sineRoll \+ normalizedY \* cosineRoll/);
assert.match(generator, /Math\.atan2\(orientedX, depth \* cosineCenterLatitude - orientedY \* sineCenterLatitude\)/);
assert.doesNotMatch(generator, /simplifyRing|buildPreviewMaster|setInterval|requestAnimationFrame|canvas|transform:\s*rotate/i);
assert.match(generator, /ocean: \[224, 243, 244\]/);
assert.match(generator, /land: \[49, 145, 101\]/);
assert.match(generator, /ocean: \[28, 67, 70\]/);
assert.match(generator, /land: \[113, 201, 156\]/);

for (const degrees of candidates) {
  assert.match(preview, new RegExp(`${String(degrees).replace(".", "\\.")} degree axial tilt`));
}
assert.equal((preview.match(/class="loop-stage"/g) ?? []).length, candidates.length * 4);
assert.equal((preview.match(/data-frame-index=/g) ?? []).length, candidates.length * 4 * 16);
assert.match(preview, /animation: globeFrames 2000ms steps\(1, end\) infinite/);
assert.doesNotMatch(preview, /transform:\s*rotate/i);

console.log("routing globe axial-tilt comparison assets passed");
