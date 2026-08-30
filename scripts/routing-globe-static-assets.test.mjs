import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import { join } from "node:path";
import { inflateSync } from "node:zlib";

const projectRoot = join(import.meta.dirname, "..");
const outputDir = join(projectRoot, "src", "assets", "routing-globe");
const acceptedDir = join(projectRoot, "docs", "assets", "routing-globe-axis-tilt-experiment");
const manifest = JSON.parse(await readFile(join(outputDir, "manifest.json"), "utf8"));
const accepted = JSON.parse(await readFile(join(acceptedDir, "manifest.json"), "utf8"));
const acceptedVariant = accepted.variants.find(({ axialTiltDegrees }) => axialTiltDegrees === 23.5);

const readRgba = async (path) => {
  const png = await readFile(path);
  const width = png.readUInt32BE(16);
  const height = png.readUInt32BE(20);
  const chunks = [];
  for (let offset = 8; offset < png.length;) {
    const length = png.readUInt32BE(offset);
    if (png.toString("ascii", offset + 4, offset + 8) === "IDAT") chunks.push(png.subarray(offset + 8, offset + 8 + length));
    offset += length + 12;
  }
  const scanlines = inflateSync(Buffer.concat(chunks));
  const rgba = Buffer.alloc(width * height * 4);
  for (let y = 0; y < height; y += 1) scanlines.copy(rgba, y * width * 4, y * (width * 4 + 1) + 1, y * (width * 4 + 1) + 1 + width * 4);
  return { png, width, height, rgba };
};

assert.equal(manifest.staticAxialTiltDegrees, 0);
assert.equal(manifest.staticFrameIndex, null);
assert.equal(manifest.staticOrientation, "front-facing orthographic frame with no axial tilt");
assert.equal(manifest.staticCenterLongitude, 122);

for (const size of manifest.sizes) {
  for (const theme of manifest.themes) {
    const name = manifest.statics[String(size)][theme];
    const image = await readRgba(join(outputDir, name));
    const active = await readRgba(join(acceptedDir, acceptedVariant.frames[0].files[String(size)][theme]));
    assert.equal(image.width, size);
    assert.equal(image.height, size);
    assert.notDeepEqual(image.rgba, active.rgba, `${name} must be independent from the tilted active frame`);
    assert.equal(manifest.hashes[name], createHash("sha256").update(image.png).digest("hex"));
    assert.equal(manifest.staticHashes[name], createHash("sha256").update(image.png).digest("hex"));
  }
}

const generator = await readFile(join(projectRoot, "scripts", "generate-routing-globe-static-assets.mjs"), "utf8");
assert.match(generator, /STATIC_AXIAL_TILT_DEGREES = 0/);
assert.match(generator, /const relativeLongitude = \(Math\.atan2\(normalizedX/);
assert.doesNotMatch(generator, /cosineRoll|sineRoll|rotateY|scaleX/);

console.log("dedicated 0 degree static globe assets passed");
