import { createHash } from "node:crypto";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { deflateSync } from "node:zlib";

const scriptDir = dirname(fileURLToPath(import.meta.url));
const projectRoot = dirname(scriptDir);
const outputDir = join(projectRoot, "docs", "assets", "routing-globe-axis-tilt-experiment");
const mapPath = join(projectRoot, "docs", "assets", "routing-globe-map-master-experiment", "preview-world-map.json");
const phase3ManifestPath = join(projectRoot, "docs", "assets", "routing-globe-projection-experiment", "manifest.json");

const FRAME_COUNT = 16;
const FRAME_STEP_DEGREES = 360 / FRAME_COUNT;
const LOOP_DURATION_MS = 2000;
const BASE_CENTER_LONGITUDE = 122;
const CENTER_LATITUDE = 12;
const RADIUS_RATIO = 0.4375;
const INSPECTION_SIZE = 256;
const ICON_SIZES = [32, 24];
const SUPER_SAMPLE = 4;
const SOURCE_RASTER_WIDTH = 4096;
const SOURCE_RASTER_HEIGHT = SOURCE_RASTER_WIDTH / 2;
const TILT_DEGREES = [18, 20, 23.5];

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
const phase3Manifest = JSON.parse(await readFile(phase3ManifestPath, "utf8"));
const polygons = mapSource.polygons;
const mapSha256 = createHash("sha256").update(mapBytes).digest("hex");
const polygonsWithLatitudeBounds = polygons.map((polygon) => ({
  polygon,
  minimumLatitude: Math.min(...polygon.map(([, latitude]) => latitude)),
  maximumLatitude: Math.max(...polygon.map(([, latitude]) => latitude)),
}));

const frozenParameters = {
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
};
for (const [name, value] of Object.entries(frozenParameters)) {
  if (JSON.stringify(phase3Manifest[name]) !== JSON.stringify(value)) {
    throw new Error(`Frozen Phase 3 parameter changed: ${name}`);
  }
}

const normalizeLongitude = (longitude) => {
  let value = longitude;
  while (value <= -180) value += 360;
  while (value > 180) value -= 360;
  return value;
};

const tiltSlug = (degrees) => String(degrees).replace(".", "-");

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

const renderLandCoverage = (size, centerLongitude, axialTiltDegrees) => {
  const renderSize = size * SUPER_SAMPLE;
  const center = renderSize / 2;
  const radius = size * RADIUS_RATIO * SUPER_SAMPLE;
  const coverage = new Uint8Array(renderSize * renderSize);
  const centerLatitudeRadians = (CENTER_LATITUDE * Math.PI) / 180;
  const sineCenterLatitude = Math.sin(centerLatitudeRadians);
  const cosineCenterLatitude = Math.cos(centerLatitudeRadians);
  const rollRadians = (axialTiltDegrees * Math.PI) / 180;
  const sineRoll = Math.sin(rollRadians);
  const cosineRoll = Math.cos(rollRadians);
  for (let y = 0; y < renderSize; y += 1) {
    for (let x = 0; x < renderSize; x += 1) {
      const normalizedX = (x + 0.5 - center) / radius;
      const normalizedY = (center - (y + 0.5)) / radius;
      const squared = normalizedX * normalizedX + normalizedY * normalizedY;
      if (squared > 1) continue;
      const depth = Math.sqrt(Math.max(0, 1 - squared));

      // Inverse fixed screen-space roll: the geographic north axis is rotated
      // left-up once, then every longitude phase spins around that same axis.
      const orientedX = normalizedX * cosineRoll + normalizedY * sineRoll;
      const orientedY = -normalizedX * sineRoll + normalizedY * cosineRoll;
      const sineLatitude = depth * sineCenterLatitude + orientedY * cosineCenterLatitude;
      const latitude = (Math.asin(Math.max(-1, Math.min(1, sineLatitude))) * 180) / Math.PI;
      const relativeLongitude = (Math.atan2(orientedX, depth * cosineCenterLatitude - orientedY * sineCenterLatitude) * 180) / Math.PI;
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

const projectedNorthAxis = (axialTiltDegrees) => {
  const tiltRadians = (axialTiltDegrees * Math.PI) / 180;
  const centerLatitudeRadians = (CENTER_LATITUDE * Math.PI) / 180;
  const projectedLength = Math.cos(centerLatitudeRadians);
  return {
    x: Number((-Math.sin(tiltRadians) * projectedLength).toFixed(6)),
    y: Number((Math.cos(tiltRadians) * projectedLength).toFixed(6)),
    z: Number(Math.sin(centerLatitudeRadians).toFixed(6)),
  };
};

const loopMarkup = (variant, theme, size) => {
  const intervalSeconds = LOOP_DURATION_MS / FRAME_COUNT / 1000;
  const images = variant.frames.map((frame) => `<img src="./${frame.files[String(size)][theme]}" width="${size}" height="${size}" alt="" data-frame-index="${frame.index}" style="animation-delay:${(frame.index * intervalSeconds).toFixed(3)}s"/>`).join("");
  return `<section class="loop-panel${theme === "dark" ? " loop-panel--dark" : ""}" style="--icon-size:${size}px"><h3>${theme === "dark" ? "Dark" : "Light"} / ${size}px loop</h3><div class="loop-stage" aria-label="${variant.axialTiltDegrees} degree tilt ${theme} ${size}px loop">${images}</div></section>`;
};

const staticMarkup = (variant) => `<div class="single-grid"><section class="single-panel"><h3>Single frame / light</h3><img class="inspection" src="./${variant.inspection.light}" width="256" height="256" alt="${variant.axialTiltDegrees} degree axial tilt light inspection frame"/><div class="actual"><img src="./${variant.frames[0].files["32"].light}" width="32" height="32" alt=""/><img src="./${variant.frames[0].files["24"].light}" width="24" height="24" alt=""/></div></section><section class="single-panel single-panel--dark"><h3>Single frame / dark</h3><img class="inspection" src="./${variant.inspection.dark}" width="256" height="256" alt="${variant.axialTiltDegrees} degree axial tilt dark inspection frame"/><div class="actual"><img src="./${variant.frames[0].files["32"].dark}" width="32" height="32" alt=""/><img src="./${variant.frames[0].files["24"].dark}" width="24" height="24" alt=""/></div></section></div>`;

const previewHtml = (variants) => `<!doctype html>
<meta charset="utf-8"/>
<title>Routing globe axial tilt comparison</title>
<style>
  :root { font-family: system-ui, sans-serif; color: #20312f; background: #eef3f2; }
  body { margin: 0; padding: 28px; }
  main { max-width: 1120px; }
  h1 { margin: 0 0 8px; font-size: 20px; }
  h2 { margin: 0 0 12px; font-size: 16px; }
  h3 { margin: 0 0 10px; font-size: 13px; }
  p { max-width: 880px; margin: 0 0 24px; color: #536966; font-size: 13px; line-height: 1.5; }
  .variant { padding: 22px 0 28px; border-top: 1px solid #cadeda; }
  .single-grid, .loop-grid { display: grid; grid-template-columns: repeat(2, minmax(240px, 1fr)); gap: 12px; max-width: 760px; }
  .single-grid { margin-bottom: 12px; }
  .single-panel, .loop-panel { padding: 16px; border: 1px solid #d7e1df; border-radius: 8px; background: #fff; }
  .single-panel--dark, .loop-panel--dark { color: #e2f1ef; border-color: #31514e; background: #182221; }
  .inspection { display: block; width: 256px; max-width: 100%; height: auto; }
  .actual { display: flex; align-items: center; gap: 16px; min-height: 48px; margin-top: 8px; }
  .actual img { display: block; }
  .loop-stage { position: relative; width: 72px; height: 72px; }
  .loop-stage img { position: absolute; top: 50%; left: 50%; width: var(--icon-size); height: var(--icon-size); opacity: 0; transform: translate(-50%, -50%); animation: globeFrames ${LOOP_DURATION_MS}ms steps(1, end) infinite; }
  @keyframes globeFrames { 0%, 6.249% { opacity: 1; } 6.25%, 100% { opacity: 0; } }
  code { color: #536966; font-size: 12px; }
  @media (max-width: 620px) { .single-grid, .loop-grid { grid-template-columns: 1fr; } }
</style>
<main><h1>Routing globe axial tilt comparison</h1><p>The accepted map, 16-frame sequence, projection, palette, center latitude, sphere geometry, rasterization, and downsample pipeline are frozen. Each group changes only one fixed orientation roll, placing geographic north toward the same left-up screen direction for the whole loop.</p>${variants.map((variant) => `<section class="variant" data-tilt="${variant.axialTiltDegrees}"><h2>${variant.axialTiltDegrees} degree axial tilt</h2>${staticMarkup(variant)}<div class="loop-grid">${loopMarkup(variant, "light", 32)}${loopMarkup(variant, "light", 24)}${loopMarkup(variant, "dark", 32)}${loopMarkup(variant, "dark", 24)}</div></section>`).join("")}<code>Frozen Phase 3 pipeline / north points left-up / ${LOOP_DURATION_MS}ms / ${FRAME_COUNT} frames</code></main>`;

await mkdir(outputDir, { recursive: true });
const variants = [];
for (const axialTiltDegrees of TILT_DEGREES) {
  const slug = tiltSlug(axialTiltDegrees);
  const inspection = {};
  const inspectionRendered = renderLandCoverage(INSPECTION_SIZE, BASE_CENTER_LONGITUDE, axialTiltDegrees);
  for (const [theme, palette] of [["light", LIGHT], ["dark", DARK]]) {
    const filename = `routing-globe-tilt-${slug}-single-inspection-${theme}.png`;
    await writeFile(join(outputDir, filename), encodePng(INSPECTION_SIZE, INSPECTION_SIZE, composeFrame(inspectionRendered, INSPECTION_SIZE, palette)));
    inspection[theme] = filename;
  }

  const frames = [];
  for (let index = 0; index < FRAME_COUNT; index += 1) {
    const centerLongitude = normalizeLongitude(BASE_CENTER_LONGITUDE + index * FRAME_STEP_DEGREES);
    const frame = { index, centerLongitude, files: {} };
    for (const size of ICON_SIZES) {
      const rendered = renderLandCoverage(size, centerLongitude, axialTiltDegrees);
      frame.files[String(size)] = {};
      for (const [theme, palette] of [["light", LIGHT], ["dark", DARK]]) {
        const filename = `routing-globe-tilt-${slug}-frame-${String(index).padStart(2, "0")}-${size}-${theme}.png`;
        await writeFile(join(outputDir, filename), encodePng(size, size, composeFrame(rendered, size, palette)));
        frame.files[String(size)][theme] = filename;
      }
    }
    frames.push(frame);
  }
  variants.push({
    axialTiltDegrees,
    orientation: "fixed screen-space roll before inverse orthographic sampling",
    northAxisScreen: projectedNorthAxis(axialTiltDegrees),
    inspection,
    frames,
  });
}

const outputFiles = [
  "routing-globe-axis-tilt-preview.html",
  ...variants.flatMap((variant) => [
    variant.inspection.light,
    variant.inspection.dark,
    ...variant.frames.flatMap((frame) => Object.values(frame.files).flatMap(({ light, dark }) => [light, dark])),
  ]),
];
const manifest = {
  stage: "axis-tilt-comparison",
  sourceExperiment: "../routing-globe-projection-experiment/manifest.json",
  sourceMap: "../routing-globe-map-master-experiment/preview-world-map.json",
  ...frozenParameters,
  axialTiltCandidatesDegrees: TILT_DEGREES,
  northDirection: "left-up",
  variants,
  outputs: outputFiles,
  runtime: "individual-frame-preview-only",
};

await writeFile(join(outputDir, "routing-globe-axis-tilt-preview.html"), previewHtml(variants), "utf8");
await writeFile(join(outputDir, "manifest.json"), `${JSON.stringify(manifest, null, 2)}\n`, "utf8");
await writeFile(join(outputDir, "README.md"), `# Routing globe axial tilt comparison

This experiment preserves every accepted Phase 3 map, projection, palette, geometry, rasterization, and animation parameter. It compares fixed 18, 20, and 23.5 degree screen-space orientation rolls only.

Each roll is applied inside inverse orthographic sampling, not as a CSS or post-raster image rotation. Geographic north remains fixed toward screen-left/up while all 16 longitude phases rotate around that axis.
`, "utf8");

console.log(`Generated ${TILT_DEGREES.join(" / ")} degree axial-tilt comparisons from ${mapSha256}`);
