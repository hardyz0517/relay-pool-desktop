import { createHash } from "node:crypto";
import { mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { deflateSync } from "node:zlib";

const scriptDir = dirname(fileURLToPath(import.meta.url));
const projectRoot = dirname(scriptDir);
const outputDir = join(projectRoot, "docs", "assets", "routing-globe-projection-experiment");
const mapPath = join(projectRoot, "docs", "assets", "routing-globe-map-master-experiment", "preview-world-map.json");

const INSPECTION_SIZE = 256;
const ICON_SIZES = [32, 24];
const SUPER_SAMPLE = 4;
const SOURCE_RASTER_WIDTH = 4096;
const SOURCE_RASTER_HEIGHT = SOURCE_RASTER_WIDTH / 2;
const FRAME_COUNT = 16;
const FRAME_STEP_DEGREES = 360 / FRAME_COUNT;
const LOOP_DURATION_MS = 2000;
const BASE_CENTER_LONGITUDE = 122;
const CENTER_LATITUDE = 12;
const RADIUS_RATIO = 0.4375;
const OBSOLETE_OUTPUTS = [
  "routing-globe-contact-sheet.png",
  "routing-globe-contact-sheet.svg",
  "routing-globe-loop-preview.png",
  "routing-globe-loop-preview-light.html",
  "routing-globe-loop-preview-dark.html",
  "routing-globe-preview-24-32.png",
  "routing-globe-preview.html",
  "simplified-world-map.json",
  "routing-globe-single-frame-inspection-light.png",
  "routing-globe-single-frame-inspection-dark.png",
  "routing-globe-single-frame-32-light.png",
  "routing-globe-single-frame-32-dark.png",
  "routing-globe-single-frame-24-light.png",
  "routing-globe-single-frame-24-dark.png",
  ...Array.from({ length: 16 }, (_, index) => {
    const suffix = String(index).padStart(2, "0");
    return [`routing-globe-frame-${suffix}-light.png`, `routing-globe-frame-${suffix}-dark.png`];
  }).flat(),
];

const LIGHT = {
  page: "#eef3f2",
  ocean: [224, 243, 244],
  land: [49, 145, 101],
  rim: [38, 128, 121],
};
const DARK = {
  page: "#182221",
  ocean: [28, 67, 70],
  land: [113, 201, 156],
  rim: [104, 204, 192],
};

const mapBytes = await readFile(mapPath);
const mapSource = JSON.parse(mapBytes);
const polygons = mapSource.polygons;
const mapSha256 = createHash("sha256").update(mapBytes).digest("hex");
const polygonsWithLatitudeBounds = polygons.map((polygon) => ({
  polygon,
  minimumLatitude: Math.min(...polygon.map(([, latitude]) => latitude)),
  maximumLatitude: Math.max(...polygon.map(([, latitude]) => latitude)),
}));

const normalizeLongitude = (longitude) => {
  let value = longitude;
  while (value <= -180) value += 360;
  while (value > 180) value -= 360;
  return value;
};

const buildSourceLandRaster = () => {
  const raster = new Uint8Array(SOURCE_RASTER_WIDTH * SOURCE_RASTER_HEIGHT);
  for (let y = 0; y < SOURCE_RASTER_HEIGHT; y += 1) {
    const latitude = 90 - ((y + 0.5) / SOURCE_RASTER_HEIGHT) * 180;
    const rowStart = y * SOURCE_RASTER_WIDTH;
    for (const { polygon, minimumLatitude, maximumLatitude } of polygonsWithLatitudeBounds) {
      if (latitude < minimumLatitude || latitude > maximumLatitude) continue;
      const intersections = [];
      for (let index = 0, previous = polygon.length - 1; index < polygon.length; previous = index++) {
        const [longitude1, latitude1] = polygon[index];
        const [longitude2, latitude2] = polygon[previous];
        if (latitude1 > latitude === latitude2 > latitude) continue;
        intersections.push(longitude1 + ((latitude - latitude1) * (longitude2 - longitude1)) / (latitude2 - latitude1));
      }
      intersections.sort((left, right) => left - right);
      for (let index = 0; index + 1 < intersections.length; index += 2) {
        const minimumLongitude = Math.max(-180, intersections[index]);
        const maximumLongitude = Math.min(180, intersections[index + 1]);
        const start = Math.max(0, Math.ceil(((minimumLongitude + 180) / 360) * SOURCE_RASTER_WIDTH - 0.5));
        const end = Math.min(SOURCE_RASTER_WIDTH - 1, Math.floor(((maximumLongitude + 180) / 360) * SOURCE_RASTER_WIDTH - 0.5));
        if (end >= start) raster.fill(255, rowStart + start, rowStart + end + 1);
      }
    }
  }
  return raster;
};

const sourceLandRaster = buildSourceLandRaster();

const sampleSourceLand = (longitude, latitude) => {
  const x = Math.max(0, Math.min(SOURCE_RASTER_WIDTH - 1, Math.floor(((normalizeLongitude(longitude) + 180) / 360) * SOURCE_RASTER_WIDTH)));
  const y = Math.max(0, Math.min(SOURCE_RASTER_HEIGHT - 1, Math.floor(((90 - latitude) / 180) * SOURCE_RASTER_HEIGHT)));
  return sourceLandRaster[y * SOURCE_RASTER_WIDTH + x];
};

const renderLandCoverage = (size, centerLongitude) => {
  const renderSize = size * SUPER_SAMPLE;
  const center = renderSize / 2;
  const radius = size * RADIUS_RATIO * SUPER_SAMPLE;
  const coverage = new Uint8Array(renderSize * renderSize);
  const centerLatitudeRadians = (CENTER_LATITUDE * Math.PI) / 180;
  const sineCenterLatitude = Math.sin(centerLatitudeRadians);
  const cosineCenterLatitude = Math.cos(centerLatitudeRadians);
  for (let y = 0; y < renderSize; y += 1) {
    for (let x = 0; x < renderSize; x += 1) {
      const normalizedX = (x + 0.5 - center) / radius;
      const normalizedY = (center - (y + 0.5)) / radius;
      const squared = normalizedX * normalizedX + normalizedY * normalizedY;
      if (squared > 1) continue;
      const depth = Math.sqrt(Math.max(0, 1 - squared));
      const sineLatitude = depth * sineCenterLatitude + normalizedY * cosineCenterLatitude;
      const latitude = (Math.asin(Math.max(-1, Math.min(1, sineLatitude))) * 180) / Math.PI;
      const relativeLongitude = (Math.atan2(normalizedX, depth * cosineCenterLatitude - normalizedY * sineCenterLatitude) * 180) / Math.PI;
      const longitude = normalizeLongitude(centerLongitude + relativeLongitude);
      if (sampleSourceLand(longitude, latitude)) coverage[y * renderSize + x] = 255;
    }
  }
  return { coverage, renderSize, radius };
};

const clampByte = (value) => Math.max(0, Math.min(255, Math.round(value)));

const composeFrame = (rendered, size, palette) => {
  const { coverage, renderSize, radius } = rendered;
  const rgba = new Uint8Array(size * size * 4);
  const center = renderSize / 2;
  const downsample = SUPER_SAMPLE * SUPER_SAMPLE;
  for (let y = 0; y < size; y += 1) {
    for (let x = 0; x < size; x += 1) {
      let landSamples = 0;
      let sphereSamples = 0;
      for (let sy = 0; sy < SUPER_SAMPLE; sy += 1) {
        for (let sx = 0; sx < SUPER_SAMPLE; sx += 1) {
          const sampleX = x * SUPER_SAMPLE + sx + 0.5;
          const sampleY = y * SUPER_SAMPLE + sy + 0.5;
          const distance = Math.hypot(sampleX - center, sampleY - center);
          if (distance <= radius) sphereSamples += 1;
          if (coverage[(y * SUPER_SAMPLE + sy) * renderSize + x * SUPER_SAMPLE + sx]) landSamples += 1;
        }
      }
      if (!sphereSamples) continue;
      const rawLandAlpha = landSamples / downsample;
      const landAlpha = size <= 32 && rawLandAlpha > 0 ? Math.min(1, 0.12 + rawLandAlpha * 1.05) : rawLandAlpha;
      const sphereAlpha = sphereSamples / downsample;
      const distance = Math.hypot(x + 0.5 - size / 2, y + 0.5 - size / 2);
      const edge = Math.max(0, Math.min(1, (size * RADIUS_RATIO + 0.75) - distance));
      const rimWidth = Math.max(0.75, size / 128);
      const rim = Math.max(0, Math.min(1, (distance - (size * RADIUS_RATIO - rimWidth)) / rimWidth));
      const pixel = (y * size + x) * 4;
      rgba[pixel] = clampByte((palette.ocean[0] * (1 - landAlpha) + palette.land[0] * landAlpha) * (1 - rim) + palette.rim[0] * rim);
      rgba[pixel + 1] = clampByte((palette.ocean[1] * (1 - landAlpha) + palette.land[1] * landAlpha) * (1 - rim) + palette.rim[1] * rim);
      rgba[pixel + 2] = clampByte((palette.ocean[2] * (1 - landAlpha) + palette.land[2] * landAlpha) * (1 - rim) + palette.rim[2] * rim);
      rgba[pixel + 3] = clampByte(Math.max(edge, sphereAlpha) * 255);
    }
  }
  return rgba;
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
    Buffer.from(rgba.buffer, rgba.byteOffset + y * width * 4, width * 4).copy(scanlines, rowStart + 1);
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

const frameLongitudeLabel = (longitude) => `${Number.isInteger(longitude) ? longitude : longitude.toFixed(1)} deg`;

const contactSheetSvg = async (frames, size) => {
  const inspection = size === INSPECTION_SIZE;
  const columns = inspection ? 4 : FRAME_COUNT;
  const rows = Math.ceil(FRAME_COUNT / columns);
  const cellWidth = inspection ? 280 : 72;
  const cellHeight = inspection ? 302 : 82;
  const headerHeight = 46;
  const width = columns * cellWidth;
  const height = headerHeight + rows * cellHeight;
  const title = inspection ? "Inspection contact sheet (256px)" : `${size}px contact sheet`;
  const cells = await Promise.all(frames.map(async (frame) => {
    const column = frame.index % columns;
    const row = Math.floor(frame.index / columns);
    const x = column * cellWidth;
    const y = headerHeight + row * cellHeight;
    const imageX = x + (cellWidth - size) / 2;
    const imageY = y + 8;
    const png = await readFile(join(outputDir, frame.files[String(size)].light));
    return `<g><rect x="${x}" y="${y}" width="${cellWidth}" height="${cellHeight}" fill="#f8fbfa" stroke="#d7e3e1"/><image href="data:image/png;base64,${png.toString("base64")}" x="${imageX}" y="${imageY}" width="${size}" height="${size}"/><text x="${x + cellWidth / 2}" y="${imageY + size + 15}" text-anchor="middle" font-family="system-ui,sans-serif" font-size="${inspection ? 10 : 8.5}" fill="#405c57">F${String(frame.index).padStart(2, "0")} / ${frameLongitudeLabel(frame.centerLongitude)}</text></g>`;
  }));
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="0 0 ${width} ${height}" role="img" aria-label="${title}"><rect width="${width}" height="${height}" fill="#eef3f2"/><text x="16" y="29" font-family="system-ui,sans-serif" font-size="16" fill="#20312f">${title}</text>${cells.join("")}</svg>\n`;
};

const loopPanel = (frames, theme, size) => {
  const frameIntervalSeconds = LOOP_DURATION_MS / FRAME_COUNT / 1000;
  const images = frames.map((frame) => `<img src="./${frame.files[String(size)][theme]}" width="${size}" height="${size}" alt="" style="animation-delay:${(frame.index * frameIntervalSeconds).toFixed(3)}s"/>`).join("");
  return `<section class="loop-panel${theme === "dark" ? " loop-panel--dark" : ""}" style="--icon-size:${size}px"><h3>${theme === "dark" ? "Dark" : "Light"} / ${size}px loop</h3><div class="loop-stage" aria-label="${theme} theme ${size}px rotating globe loop">${images}</div></section>`;
};

const previewHtml = (frames) => `<!doctype html>
<meta charset="utf-8" />
<title>Routing globe orthographic rotation experiment</title>
<style>
  :root { font-family: system-ui, sans-serif; color: #20312f; background: #eef3f2; }
  body { margin: 0; padding: 28px; }
  main { max-width: 1200px; }
  h1 { margin: 0 0 8px; font-size: 20px; }
  h2 { margin: 0 0 10px; font-size: 14px; }
  h3 { margin: 0 0 12px; font-size: 13px; }
  p { max-width: 850px; margin: 0 0 24px; color: #536966; font-size: 13px; line-height: 1.5; }
  section.contact-section { margin: 0 0 28px; }
  .contact-sheet { display: block; max-width: 100%; height: auto; border: 1px solid #cadeda; background: #eef3f2; }
  .loop-grid { display: grid; grid-template-columns: repeat(2, minmax(220px, 1fr)); gap: 12px; max-width: 760px; }
  .loop-panel { padding: 16px; border: 1px solid #d7e1df; border-radius: 8px; background: #fff; }
  .loop-panel--dark { color: #e2f1ef; border-color: #31514e; background: #182221; }
  .loop-stage { position: relative; width: 72px; height: 72px; }
  .loop-stage img { position: absolute; top: 50%; left: 50%; width: var(--icon-size); height: var(--icon-size); opacity: 0; transform: translate(-50%, -50%); animation: globeFrames ${LOOP_DURATION_MS}ms steps(1, end) infinite; }
  @keyframes globeFrames { 0%, 6.249% { opacity: 1; } 6.25%, 100% { opacity: 0; } }
  code { display: block; margin-top: 20px; color: #536966; font-size: 12px; }
  @media (max-width: 620px) { .loop-grid { grid-template-columns: 1fr; } }
</style>
<main>
  <h1>Routing globe orthographic rotation experiment</h1>
  <p>Sixteen independently generated orthographic frames from the frozen Preview master. Every frame uses the accepted projection, palette, sphere geometry, center latitude, rasterization, and downsample pipeline; only center longitude changes by 22.5 degrees.</p>
  <section class="contact-section"><h2>1. Inspection contact sheet</h2><img class="contact-sheet" src="./routing-globe-contact-sheet-inspection.svg" alt="16-frame inspection-size contact sheet"/></section>
  <section class="contact-section"><h2>2. 32px contact sheet</h2><img class="contact-sheet" src="./routing-globe-contact-sheet-32.svg" alt="16-frame 32px contact sheet"/></section>
  <section class="contact-section"><h2>3. 24px contact sheet</h2><img class="contact-sheet" src="./routing-globe-contact-sheet-24.svg" alt="16-frame 24px contact sheet"/></section>
  <section><h2>4-7. Actual-size loop previews</h2><div class="loop-grid">${loopPanel(frames, "light", 32)}${loopPanel(frames, "light", 24)}${loopPanel(frames, "dark", 32)}${loopPanel(frames, "dark", 24)}</div></section>
  <code>16 frames / ${FRAME_STEP_DEGREES} degrees / ${LOOP_DURATION_MS}ms / center latitude ${CENTER_LATITUDE} degrees / frame 00 center longitude ${BASE_CENTER_LONGITUDE} degrees</code>
</main>
`;

await mkdir(outputDir, { recursive: true });
await Promise.all(OBSOLETE_OUTPUTS.map((filename) => rm(join(outputDir, filename), { force: true })));

const frameAssets = [];
for (let index = 0; index < FRAME_COUNT; index += 1) {
  const centerLongitude = normalizeLongitude(BASE_CENTER_LONGITUDE + index * FRAME_STEP_DEGREES);
  const frame = { index, centerLongitude, files: {} };
  for (const [size, label] of [[INSPECTION_SIZE, "inspection"], ...ICON_SIZES.map((iconSize) => [iconSize, String(iconSize)])]) {
    const rendered = renderLandCoverage(size, centerLongitude);
    frame.files[String(size)] = {};
    for (const [theme, palette] of [["light", LIGHT], ["dark", DARK]]) {
      const filename = `routing-globe-frame-${String(index).padStart(2, "0")}-${label}-${theme}.png`;
      const png = encodePng(size, size, composeFrame(rendered, size, palette));
      await writeFile(join(outputDir, filename), png);
      frame.files[String(size)][theme] = filename;
    }
  }
  frameAssets.push(frame);
}

const contactSheets = [
  ["routing-globe-contact-sheet-inspection.svg", INSPECTION_SIZE],
  ["routing-globe-contact-sheet-32.svg", 32],
  ["routing-globe-contact-sheet-24.svg", 24],
];
for (const [filename, size] of contactSheets) {
  await writeFile(join(outputDir, filename), await contactSheetSvg(frameAssets, size), "utf8");
}

const outputFiles = [
  ...contactSheets.map(([filename]) => filename),
  "routing-globe-preview.html",
  ...frameAssets.flatMap((frame) => Object.values(frame.files).flatMap(({ light, dark }) => [light, dark])),
];
const manifest = {
  stage: "rotation-animation-validation",
  sourceMap: "../routing-globe-map-master-experiment/preview-world-map.json",
  sourceMapSha256: mapSha256,
  projection: "orthographic-inverse-raster",
  frameCount: FRAME_COUNT,
  frameStepDegrees: FRAME_STEP_DEGREES,
  loopDurationMs: LOOP_DURATION_MS,
  baseCenterLongitude: BASE_CENTER_LONGITUDE,
  centerLatitude: CENTER_LATITUDE,
  radiusRatio: RADIUS_RATIO,
  inspectionSize: INSPECTION_SIZE,
  iconSizes: ICON_SIZES,
  supersample: SUPER_SAMPLE,
  frames: frameAssets,
  outputs: outputFiles,
  runtime: "individual-frame-preview-only",
};

await writeFile(join(outputDir, "routing-globe-preview.html"), previewHtml(frameAssets), "utf8");
await writeFile(join(outputDir, "manifest.json"), `${JSON.stringify(manifest, null, 2)}\n`, "utf8");
await writeFile(join(outputDir, "README.md"), `# Routing globe full-rotation projection experiment

Phase 3 validates one full rotation without changing the accepted Preview master or the accepted projection/rendering pipeline.

- 16 independently projected frames cover 360 degrees at 22.5 degrees per frame.
- Frame 00 is the accepted single-frame view at 122 degrees center longitude; only center longitude changes afterward.
- Inspection, 32px, and 24px contact sheets label every frame and center longitude.
- Four actual-size light/dark loops run at 2.0 seconds per rotation.
- Frames remain individual PNG files. This phase does not create a sprite sheet or modify the route UI.
`, "utf8");

console.log(`Generated ${FRAME_COUNT} frozen-pipeline orthographic frames from ${mapSha256}`);
