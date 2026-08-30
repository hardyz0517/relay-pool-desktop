import { createHash } from "node:crypto";
import { copyFile, mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { deflateSync, inflateSync } from "node:zlib";

const scriptDir = dirname(fileURLToPath(import.meta.url));
const projectRoot = dirname(scriptDir);
const sourceDir = join(projectRoot, "docs", "assets", "routing-globe-axis-tilt-experiment");
const outputDir = join(projectRoot, "src", "assets", "routing-globe");
const frameOutputDir = join(outputDir, "frames");
const previewDir = join(projectRoot, "docs", "assets", "routing-globe-final");
const acceptedManifest = JSON.parse(await readFile(join(sourceDir, "manifest.json"), "utf8"));
const acceptedVariant = acceptedManifest.variants.find(({ axialTiltDegrees }) => axialTiltDegrees === 23.5);

if (!acceptedVariant) throw new Error("The accepted 23.5 degree axial-tilt variant is missing");

const FRAME_COUNT = 16;
const LOOP_DURATION_MS = 2000;
const STATIC_FRAME_INDEX = 0;
const SIZES = [24, 32];
const THEMES = ["light", "dark"];
const EXPECTED_MASTER_SHA256 = "5c1468c489229348b054b85354fe57efa8c55114a11141903eab12aefc6f27c9";

const frozenParameters = {
  sourceMapSha256: acceptedManifest.sourceMapSha256,
  projection: acceptedManifest.projection,
  axialTiltDegrees: acceptedVariant.axialTiltDegrees,
  orientation: acceptedVariant.orientation,
  northDirection: acceptedManifest.northDirection,
  northAxisScreen: acceptedVariant.northAxisScreen,
  frameCount: acceptedManifest.frameCount,
  frameStepDegrees: acceptedManifest.frameStepDegrees,
  loopDurationMs: acceptedManifest.loopDurationMs,
  baseCenterLongitude: acceptedManifest.baseCenterLongitude,
  centerLatitude: acceptedManifest.centerLatitude,
  radiusRatio: acceptedManifest.radiusRatio,
  supersample: acceptedManifest.supersample,
  palettes: {
    light: { ocean: [224, 243, 244], land: [49, 145, 101], rim: [38, 128, 121] },
    dark: { ocean: [28, 67, 70], land: [113, 201, 156], rim: [104, 204, 192] },
  },
};

if (frozenParameters.sourceMapSha256 !== EXPECTED_MASTER_SHA256) throw new Error("The frozen map master changed");
if (frozenParameters.frameCount !== FRAME_COUNT) throw new Error("The frozen frame count changed");
if (frozenParameters.loopDurationMs !== LOOP_DURATION_MS) throw new Error("The frozen loop duration changed");

const readPng = async (path) => {
  const png = await readFile(path);
  if (!png.subarray(0, 8).equals(Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]))) {
    throw new Error(`${path} is not a PNG`);
  }
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
    if (scanlines[rowStart] !== 0) throw new Error(`${path} uses an unsupported PNG filter`);
    scanlines.copy(rgba, y * width * 4, rowStart + 1, rowStart + 1 + width * 4);
  }
  return { width, height, rgba };
};

const crc32 = (buffer) => {
  let crc = 0xffffffff;
  for (const byte of buffer) {
    crc ^= byte;
    for (let bit = 0; bit < 8; bit += 1) crc = (crc >>> 1) ^ (crc & 1 ? 0xedb88320 : 0);
  }
  return (crc ^ 0xffffffff) >>> 0;
};

const pngChunk = (type, data) => {
  const typeBuffer = Buffer.from(type, "ascii");
  const body = Buffer.concat([typeBuffer, data]);
  const length = Buffer.alloc(4);
  length.writeUInt32BE(data.length, 0);
  const checksum = Buffer.alloc(4);
  checksum.writeUInt32BE(crc32(body), 0);
  return Buffer.concat([length, body, checksum]);
};

const encodePng = (width, height, rgba) => {
  const scanlines = Buffer.alloc(height * (width * 4 + 1));
  for (let y = 0; y < height; y += 1) {
    const rowStart = y * (width * 4 + 1);
    scanlines[rowStart] = 0;
    rgba.copy(scanlines, rowStart + 1, y * width * 4, (y + 1) * width * 4);
  }
  const header = Buffer.alloc(13);
  header.writeUInt32BE(width, 0);
  header.writeUInt32BE(height, 4);
  header[8] = 8;
  header[9] = 6;
  return Buffer.concat([
    Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]),
    pngChunk("IHDR", header),
    pngChunk("IDAT", deflateSync(scanlines, { level: 9 })),
    pngChunk("IEND", Buffer.alloc(0)),
  ]);
};

const sha256 = async (path) => createHash("sha256").update(await readFile(path)).digest("hex");
const frameFilename = (index, size, theme) => `routing-globe-frame-${String(index).padStart(2, "0")}-${size}-${theme}.png`;
const spriteFilename = (size, theme) => `routing-globe-sprite-${size}-${theme}.png`;
const staticFilename = (size, theme) => `routing-globe-static-${size}-${theme}.png`;

await mkdir(frameOutputDir, { recursive: true });
await mkdir(previewDir, { recursive: true });

const frames = [];
for (const sourceFrame of acceptedVariant.frames) {
  const files = {};
  for (const size of SIZES) {
    files[String(size)] = {};
    for (const theme of THEMES) {
      const source = join(sourceDir, sourceFrame.files[String(size)][theme]);
      const filename = frameFilename(sourceFrame.index, size, theme);
      const destination = join(frameOutputDir, filename);
      await copyFile(source, destination);
      files[String(size)][theme] = `frames/${filename}`;
    }
  }
  frames.push({ index: sourceFrame.index, centerLongitude: sourceFrame.centerLongitude, files });
}

const sprites = {};
const statics = {};
for (const size of SIZES) {
  sprites[String(size)] = {};
  statics[String(size)] = {};
  for (const theme of THEMES) {
    const spriteRgba = Buffer.alloc(size * FRAME_COUNT * size * 4);
    for (let index = 0; index < FRAME_COUNT; index += 1) {
      const sourcePath = join(frameOutputDir, frameFilename(index, size, theme));
      const source = await readPng(sourcePath);
      if (source.width !== size || source.height !== size) throw new Error(`${sourcePath} has the wrong dimensions`);
      for (let y = 0; y < size; y += 1) {
        const sourceStart = y * size * 4;
        const targetStart = (y * size * FRAME_COUNT + index * size) * 4;
        source.rgba.copy(spriteRgba, targetStart, sourceStart, sourceStart + size * 4);
      }
    }

    const sprite = spriteFilename(size, theme);
    const staticFrame = staticFilename(size, theme);
    await writeFile(join(outputDir, sprite), encodePng(size * FRAME_COUNT, size, spriteRgba));
    await copyFile(join(frameOutputDir, frameFilename(STATIC_FRAME_INDEX, size, theme)), join(outputDir, staticFrame));
    await copyFile(join(outputDir, sprite), join(previewDir, sprite));
    await copyFile(join(outputDir, staticFrame), join(previewDir, staticFrame));
    sprites[String(size)][theme] = sprite;
    statics[String(size)][theme] = staticFrame;
  }
}

const outputFiles = [
  ...SIZES.flatMap((size) => THEMES.flatMap((theme) => [spriteFilename(size, theme), staticFilename(size, theme)])),
  ...frames.flatMap((frame) => SIZES.flatMap((size) => THEMES.map((theme) => frame.files[String(size)][theme]))),
];
const hashes = {};
for (const filename of outputFiles) hashes[filename] = await sha256(join(outputDir, filename));

const manifest = {
  stage: "final-routing-globe-assets",
  sourceExperiment: "../../../docs/assets/routing-globe-axis-tilt-experiment/manifest.json",
  ...frozenParameters,
  staticFrameIndex: STATIC_FRAME_INDEX,
  staticCenterLongitude: frames[STATIC_FRAME_INDEX].centerLongitude,
  sizes: SIZES,
  themes: THEMES,
  sprites,
  statics,
  frames,
  hashes,
  runtime: "css-steps-only",
};

await writeFile(join(outputDir, "manifest.json"), `${JSON.stringify(manifest, null, 2)}\n`, "utf8");
await writeFile(join(outputDir, "README.md"), `# Routing globe assets\n\nFrozen 23.5 degree orthographic globe. The four sprites contain 16 horizontal frames and play over 2000ms with CSS \`steps(16, end)\`. Inactive and reduced-motion use a dedicated 0 degree front-facing frame at 122 degrees longitude.\n\nRegenerate with \`pnpm generate:routing-globe-final-assets\`.\n`, "utf8");

const demo = (theme, size, state) => {
  const label = state === "active" ? "Active" : state === "inactive" ? "Inactive" : "Reduced motion";
  const classes = ["globe", state === "active" ? "globe--active" : "", state === "reduced" ? "globe--reduced" : ""].filter(Boolean).join(" ");
  return `<article class="demo"><h3>${label}</h3><span class="${classes}" style="--globe-size:${size}px;--globe-end:${-size * 16}px;--globe-sprite:url('./${spriteFilename(size, theme)}');--globe-static:url('./${staticFilename(size, theme)}')" aria-label="${theme} ${size}px ${label.toLowerCase()}"></span></article>`;
};

const previewHtml = `<!doctype html>
<html lang="en"><head><meta charset="utf-8"/><meta name="viewport" content="width=device-width,initial-scale=1"/><title>Final routing globe</title>
<style>
:root{font-family:Inter,ui-sans-serif,system-ui,sans-serif;color:#20312f;background:#eef3f2}*{box-sizing:border-box}body{margin:0;padding:28px}main{max-width:980px}h1{margin:0 0 8px;font-size:20px;letter-spacing:0}h2{margin:0 0 16px;font-size:15px;letter-spacing:0}h3{margin:0;font-size:12px;font-weight:600;letter-spacing:0}p{margin:0 0 22px;color:#536966;font-size:13px}.theme{margin-top:16px;padding:18px;border:1px solid #d7e1df;border-radius:8px;background:#fff}.theme--dark{color:#e2f1ef;border-color:#31514e;background:#182221}.state-grid{display:grid;grid-template-columns:repeat(3,minmax(120px,1fr));gap:10px}.size-group+.size-group{margin-top:18px}.demo{display:flex;min-height:72px;align-items:center;justify-content:space-between;gap:12px;padding:12px;border:1px solid currentColor;border-radius:6px;color:inherit}.theme:not(.theme--dark) .demo{border-color:#d7e1df}.theme--dark .demo{border-color:#31514e}.globe{--globe-size:24px;display:block;width:var(--globe-size);height:var(--globe-size);flex:0 0 var(--globe-size);background-image:var(--globe-static);background-repeat:no-repeat;background-position:0 0;background-size:100% 100%}.globe--active{background-image:var(--globe-sprite);background-size:auto 100%;animation:routingGlobeFrames 2000ms steps(16,end) infinite}.globe--reduced{animation:none;background-position:0 0}@keyframes routingGlobeFrames{from{background-position-x:0}to{background-position-x:var(--globe-end)}}@media(prefers-reduced-motion:reduce){.globe--active{animation:none;background-image:var(--globe-static);background-size:100% 100%;background-position:0 0}}.shell-preview{display:flex;height:150px;overflow:hidden;border:1px solid #d7e1df;border-radius:6px;background:#f7faf9}.theme--dark .shell-preview{border-color:#31514e;background:#12201f}.shell-sidebar{display:flex;width:64px;flex:0 0 64px;flex-direction:column;align-items:center;justify-content:space-between;border-right:1px solid #d7e1df;background:#fff;padding:8px}.theme--dark .shell-sidebar{border-color:#31514e;background:#182221}.shell-nav{display:grid;gap:4px}.shell-nav i{display:block;width:40px;height:40px;border-radius:4px;background:#edf3f1}.theme--dark .shell-nav i{background:#203330}.shell-status{display:flex;width:40px;height:40px;align-items:center;justify-content:center;border:1px solid #d7e1df;border-radius:4px}.theme--dark .shell-status{border-color:#31514e}.shell-content{flex:1;padding:18px}.shell-content span{display:block;height:8px;width:48%;margin-bottom:10px;border-radius:4px;background:#d7e1df}.theme--dark .shell-content span{background:#31514e}@media(max-width:620px){body{padding:18px}.state-grid{grid-template-columns:1fr}ul{columns:1}}
</style></head><body><main><h1>Final routing globe</h1><p>Frozen 23.5 degree tilt · 16 frames · 2000ms · CSS steps() playback · 0 degree front-facing static baseline.</p>
${THEMES.map((theme) => `<section class="theme${theme === "dark" ? " theme--dark" : ""}"><h2>${theme === "dark" ? "Dark" : "Light"} theme</h2>${SIZES.map((size) => `<div class="size-group"><h2>${size}px actual size</h2><div class="state-grid">${["active", "inactive", "reduced"].map((state) => demo(theme, size, state)).join("")}</div></div>`).join("")}<div class="size-group"><h2>AppShell sidebar integration</h2><div class="shell-preview"><aside class="shell-sidebar"><div class="shell-nav"><i></i><i></i><i></i></div><div class="shell-status"><span class="globe globe--active" style="--globe-size:24px;--globe-end:-384px;--globe-sprite:url('./${spriteFilename(24, theme)}');--globe-static:url('./${staticFilename(24, theme)}')" aria-label="${theme} sidebar active"></span></div></aside><div class="shell-content"><span></span><span></span></div></div></div></section>`).join("")}
<section class="theme"><h2>Project resources</h2><ul>${outputFiles.filter((file) => !file.startsWith("frames/")).map((file) => `<li><a href="../../../src/assets/routing-globe/${file}">${file}</a></li>`).join("")}<li><a href="../../../src/assets/routing-globe/manifest.json">manifest.json</a></li><li><a href="../../../src/assets/routing-globe/README.md">README.md</a></li></ul><details><summary>Individual frame exports</summary><ul>${outputFiles.filter((file) => file.startsWith("frames/")).map((file) => `<li><a href="../../../src/assets/routing-globe/${file}">${file}</a></li>`).join("")}</ul></details></section>
</main></body></html>`;

await writeFile(join(previewDir, "routing-globe-final-preview.html"), previewHtml, "utf8");
await writeFile(join(previewDir, "manifest.json"), `${JSON.stringify(manifest, null, 2)}\n`, "utf8");
await import("./generate-routing-globe-static-assets.mjs");
console.log(`Packed ${FRAME_COUNT} frozen frames into final 24px and 32px routing globe sprites`);
