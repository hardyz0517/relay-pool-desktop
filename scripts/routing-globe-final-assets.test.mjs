import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { access, readFile } from "node:fs/promises";
import { join } from "node:path";
import { inflateSync } from "node:zlib";

const projectRoot = join(import.meta.dirname, "..");
const outputDir = join(projectRoot, "src", "assets", "routing-globe");
const acceptedDir = join(projectRoot, "docs", "assets", "routing-globe-axis-tilt-experiment");
const previewDir = join(projectRoot, "docs", "assets", "routing-globe-final");
const manifest = JSON.parse(await readFile(join(outputDir, "manifest.json"), "utf8"));
const accepted = JSON.parse(await readFile(join(acceptedDir, "manifest.json"), "utf8"));
const acceptedVariant = accepted.variants.find(({ axialTiltDegrees }) => axialTiltDegrees === 23.5);

const readPng = async (path) => {
  const png = await readFile(path);
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
    assert.equal(scanlines[rowStart], 0, `${path} uses an unexpected PNG filter`);
    scanlines.copy(rgba, y * width * 4, rowStart + 1, rowStart + 1 + width * 4);
  }
  return { png, width, height, rgba };
};

assert.equal(manifest.stage, "final-routing-globe-assets");
assert.equal(manifest.sourceMapSha256, "5c1468c489229348b054b85354fe57efa8c55114a11141903eab12aefc6f27c9");
assert.equal(manifest.projection, "orthographic-inverse-raster");
assert.equal(manifest.axialTiltDegrees, 23.5);
assert.equal(manifest.frameCount, 16);
assert.equal(manifest.frameStepDegrees, 22.5);
assert.equal(manifest.loopDurationMs, 2000);
assert.equal(manifest.staticFrameIndex, null);
assert.equal(manifest.staticCenterLongitude, 122);
assert.equal(manifest.staticAxialTiltDegrees, 0);
assert.equal(manifest.staticOrientation, "front-facing orthographic frame with no axial tilt");
assert.equal(manifest.runtime, "css-steps-only");
assert.deepEqual(manifest.sizes, [24, 32]);
assert.deepEqual(manifest.themes, ["light", "dark"]);

for (const size of manifest.sizes) {
  for (const theme of manifest.themes) {
    const spritePath = join(outputDir, manifest.sprites[String(size)][theme]);
    const sprite = await readPng(spritePath);
    assert.equal(sprite.width, size * 16);
    assert.equal(sprite.height, size);

    for (let index = 0; index < 16; index += 1) {
      const acceptedFilename = acceptedVariant.frames[index].files[String(size)][theme];
      const acceptedFrame = await readPng(join(acceptedDir, acceptedFilename));
      const exportedPath = join(outputDir, manifest.frames[index].files[String(size)][theme]);
      const exportedFrame = await readPng(exportedPath);
      assert.deepEqual(exportedFrame.rgba, acceptedFrame.rgba, `exported frame ${index} ${size} ${theme} drifted`);
      for (let y = 0; y < size; y += 1) {
        const sliceStart = (y * size * 16 + index * size) * 4;
        const frameStart = y * size * 4;
        assert.deepEqual(
          sprite.rgba.subarray(sliceStart, sliceStart + size * 4),
          acceptedFrame.rgba.subarray(frameStart, frameStart + size * 4),
          `sprite slice ${index} ${size} ${theme} drifted`,
        );
      }
    }

    const staticFrame = await readPng(join(outputDir, manifest.statics[String(size)][theme]));
    const activeBaseline = await readPng(join(acceptedDir, acceptedVariant.frames[0].files[String(size)][theme]));
    assert.notDeepEqual(staticFrame.rgba, activeBaseline.rgba, `${size} ${theme} static frame must not be an active tilted frame`);
    assert.equal(manifest.staticHashes[manifest.statics[String(size)][theme]], createHash("sha256").update(staticFrame.png).digest("hex"));
    assert.equal(manifest.hashes[manifest.sprites[String(size)][theme]], createHash("sha256").update(sprite.png).digest("hex"));
  }
}

const preview = await readFile(join(previewDir, "routing-globe-final-preview.html"), "utf8");
assert.match(preview, /animation:routingGlobeFrames 2000ms steps\(16,end\) infinite/);
assert.match(preview, /--globe-static:url\('\.\/routing-globe-static-24-light\.png'\)/);
assert.match(preview, /0 degree front-facing static baseline/);
assert.match(preview, /--globe-end:-384px/);
assert.match(preview, /--globe-end:-512px/);
assert.match(preview, /@media\(prefers-reduced-motion:reduce\)/);
assert.match(preview, /Active/);
assert.match(preview, /Inactive/);
assert.match(preview, /Reduced motion/);
assert.doesNotMatch(preview, /setInterval|requestAnimationFrame|canvas|webgl/i);
await access(join(previewDir, "manifest.json"));

console.log("final routing globe sprites match every accepted 23.5 degree source frame");
