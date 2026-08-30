import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { access, readFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { inflateSync } from "node:zlib";

const scriptDir = dirname(fileURLToPath(import.meta.url));
const projectRoot = join(scriptDir, "..");
const root = join(projectRoot, "docs", "assets", "routing-globe-projection-experiment");
const mapPath = join(projectRoot, "docs", "assets", "routing-globe-map-master-experiment", "preview-world-map.json");
const manifest = JSON.parse(await readFile(join(root, "manifest.json"), "utf8"));
const generator = await readFile(join(scriptDir, "generate-routing-globe-projection-experiment.mjs"), "utf8");
const preview = await readFile(join(root, "routing-globe-preview.html"), "utf8");
const mapBytes = await readFile(mapPath);
const map = JSON.parse(mapBytes);
const expectedMapSha256 = createHash("sha256").update(mapBytes).digest("hex");

const FRAME_COUNT = 16;
const FRAME_STEP_DEGREES = 22.5;
const BASE_CENTER_LONGITUDE = 122;
const sizes = [256, 32, 24];
const themes = ["light", "dark"];
const baselineFrameHashes = {
  "256-dark": "8c6d70d50c6b4382de575aaa4109c62bb950ed6bcb5c939f2d48dee3304e937c",
  "256-light": "87d5ca6be696e4dc3ebfb314bc4adf770e71a90915283ad1224c7308f558a37a",
  "32-dark": "ab54a3c66117af68c1e74a22ece7d02385ab2516bfffd57bff4c2f09519a25fa",
  "32-light": "c35c50ac68cdd34def7bc7b8ce28a439e4c31649f20a4b353accb16078a1337b",
  "24-dark": "1117d206a4f0c07995375a64c68172f039cbad9ff97a6f6dd8b2f8f54509ffba",
  "24-light": "d3454e74f8c08d0d7a285cecfa74f04717c885c12e69508d677f5e64fd678711",
};

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

assert.equal(manifest.stage, "rotation-animation-validation");
assert.equal(manifest.sourceMap, "../routing-globe-map-master-experiment/preview-world-map.json");
assert.equal(manifest.sourceMapSha256, expectedMapSha256);
assert.equal(manifest.projection, "orthographic-inverse-raster");
assert.equal(manifest.frameCount, FRAME_COUNT);
assert.equal(manifest.frameStepDegrees, FRAME_STEP_DEGREES);
assert.equal(manifest.loopDurationMs, 2000);
assert.equal(manifest.baseCenterLongitude, BASE_CENTER_LONGITUDE);
assert.equal(manifest.centerLatitude, 12);
assert.equal(manifest.radiusRatio, 0.4375);
assert.equal(manifest.inspectionSize, 256);
assert.deepEqual(manifest.iconSizes, [32, 24]);
assert.equal(manifest.supersample, 4);
assert.equal(manifest.runtime, "individual-frame-preview-only");
assert.equal(manifest.frames.length, FRAME_COUNT);
assert.equal(map.source, "natural-earth-land-50m.geojson");
assert.ok(map.polygons.length > 900);

const referenceAlphaBySize = new Map();
for (const frame of manifest.frames) {
  assert.equal(frame.index, manifest.frames.indexOf(frame));
  assert.equal(frame.centerLongitude, normalizeLongitude(BASE_CENTER_LONGITUDE + frame.index * FRAME_STEP_DEGREES));
  for (const size of sizes) {
    for (const theme of themes) {
      const filename = frame.files[String(size)][theme];
      const image = await readPng(filename);
      assert.equal(image.width, size, `${filename} width`);
      assert.equal(image.height, size, `${filename} height`);
      const alpha = alphaChannel(image.rgba);
      if (!referenceAlphaBySize.has(size)) referenceAlphaBySize.set(size, alpha);
      assert.deepEqual(alpha, referenceAlphaBySize.get(size), `${filename} changed the fixed sphere outline`);
      if (frame.index === 0) {
        const hash = createHash("sha256").update(image.png).digest("hex");
        assert.equal(hash, baselineFrameHashes[`${size}-${theme}`], `${filename} no longer matches the accepted Phase 2 frame`);
      }
    }
  }
}

const contactSheetFiles = [
  "routing-globe-contact-sheet-inspection.svg",
  "routing-globe-contact-sheet-32.svg",
  "routing-globe-contact-sheet-24.svg",
];
for (const filename of contactSheetFiles) {
  const svg = await readFile(join(root, filename), "utf8");
  assert.equal((svg.match(/<image /g) ?? []).length, FRAME_COUNT);
  for (const frame of manifest.frames) assert.match(svg, new RegExp(`F${String(frame.index).padStart(2, "0")} /`));
}

assert.equal(manifest.outputs.length, 4 + FRAME_COUNT * sizes.length * themes.length);
for (const filename of manifest.outputs) await access(join(root, filename));
assert.ok(manifest.outputs.every((filename) => !/sprite/i.test(filename)));
for (const filename of [
  "routing-globe-single-frame-inspection-light.png",
  "routing-globe-single-frame-24-dark.png",
  "routing-globe-loop-preview-light.html",
  "simplified-world-map.json",
]) {
  await assert.rejects(access(join(root, filename)), { code: "ENOENT" });
}

assert.match(generator, /renderLandCoverage\(size, centerLongitude\)/);
assert.match(generator, /Math\.atan2\(normalizedX, depth \* cosineCenterLatitude - normalizedY \* sineCenterLatitude\)/);
assert.doesNotMatch(generator, /simplifyRing|buildPreviewMaster|natural-earth-land-50m|setInterval|requestAnimationFrame|canvas/i);
assert.match(preview, /Inspection contact sheet/);
assert.match(preview, /32px contact sheet/);
assert.match(preview, /24px contact sheet/);
assert.match(preview, /Light \/ 32px loop/);
assert.match(preview, /Light \/ 24px loop/);
assert.match(preview, /Dark \/ 32px loop/);
assert.match(preview, /Dark \/ 24px loop/);
assert.equal((preview.match(/animation-delay:/g) ?? []).length, FRAME_COUNT * 4);
assert.match(preview, /animation: globeFrames 2000ms steps\(1, end\) infinite/);

console.log("routing globe full-rotation projection assets passed");
