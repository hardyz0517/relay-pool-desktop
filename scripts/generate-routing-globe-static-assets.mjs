import { createHash } from "node:crypto";
import { copyFile, mkdir, readFile, writeFile } from "node:fs/promises";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { deflateSync } from "node:zlib";

const scriptDir = dirname(fileURLToPath(import.meta.url));
const projectRoot = dirname(scriptDir);
const mapPath = join(projectRoot, "docs", "assets", "routing-globe-map-master-experiment", "preview-world-map.json");
const finalDir = join(projectRoot, "src", "assets", "routing-globe");
const previewDir = join(projectRoot, "docs", "assets", "routing-globe-final");
const mapBytes = await readFile(mapPath);
const finalManifest = JSON.parse(await readFile(join(finalDir, "manifest.json"), "utf8"));
const phase3Manifest = JSON.parse(await readFile(join(projectRoot, "docs", "assets", "routing-globe-projection-experiment", "manifest.json"), "utf8"));
const mapSource = JSON.parse(mapBytes);

const EXPECTED_MASTER_SHA256 = "5c1468c489229348b054b85354fe57efa8c55114a11141903eab12aefc6f27c9";
const STATIC_AXIAL_TILT_DEGREES = 0;
const STATIC_CENTER_LONGITUDE = 122;
const INSPECTION_SIZE = 256;
const SUPER_SAMPLE = 4;
const SOURCE_RASTER_WIDTH = 4096;
const SOURCE_RASTER_HEIGHT = SOURCE_RASTER_WIDTH / 2;
const palettes = finalManifest.palettes;
const polygons = mapSource.polygons;

const mapSha256 = createHash("sha256").update(mapBytes).digest("hex");
if (mapSha256 !== EXPECTED_MASTER_SHA256 || finalManifest.sourceMapSha256 !== EXPECTED_MASTER_SHA256) {
  throw new Error("The frozen map master changed");
}
for (const name of ["projection", "centerLatitude", "radiusRatio", "supersample"]) {
  if (finalManifest[name] !== phase3Manifest[name]) throw new Error(`Frozen parameter changed: ${name}`);
}

const normalizeLongitude = (longitude) => {
  let value = longitude;
  while (value <= -180) value += 360;
  while (value > 180) value -= 360;
  return value;
};

const polygonsWithLatitudeBounds = polygons.map((polygon) => ({
  polygon,
  minimumLatitude: Math.min(...polygon.map(([, latitude]) => latitude)),
  maximumLatitude: Math.max(...polygon.map(([, latitude]) => latitude)),
}));

const sourceLandRaster = new Uint8Array(SOURCE_RASTER_WIDTH * SOURCE_RASTER_HEIGHT);
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
      if (end >= start) sourceLandRaster.fill(255, rowStart + start, rowStart + end + 1);
    }
  }
}

const sampleSourceLand = (longitude, latitude) => {
  const x = Math.max(0, Math.min(SOURCE_RASTER_WIDTH - 1, Math.floor(((normalizeLongitude(longitude) + 180) / 360) * SOURCE_RASTER_WIDTH)));
  const y = Math.max(0, Math.min(SOURCE_RASTER_HEIGHT - 1, Math.floor(((90 - latitude) / 180) * SOURCE_RASTER_HEIGHT)));
  return sourceLandRaster[y * SOURCE_RASTER_WIDTH + x];
};

const renderLandCoverage = (size, centerLongitude) => {
  const renderSize = size * SUPER_SAMPLE;
  const center = renderSize / 2;
  const radius = size * finalManifest.radiusRatio * SUPER_SAMPLE;
  const coverage = new Uint8Array(renderSize * renderSize);
  const centerLatitudeRadians = (finalManifest.centerLatitude * Math.PI) / 180;
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
      if (sampleSourceLand(normalizeLongitude(centerLongitude + relativeLongitude), latitude)) {
        coverage[y * renderSize + x] = 255;
      }
    }
  }
  return { coverage, renderSize, radius };
};

const clampByte = (value) => Math.max(0, Math.min(255, Math.round(value)));
const composeFrame = (rendered, size, palette) => {
  const { coverage, renderSize, radius } = rendered;
  const rgba = Buffer.alloc(size * size * 4);
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
          if (Math.hypot(sampleX - center, sampleY - center) <= radius) sphereSamples += 1;
          if (coverage[(y * SUPER_SAMPLE + sy) * renderSize + x * SUPER_SAMPLE + sx]) landSamples += 1;
        }
      }
      if (!sphereSamples) continue;
      const rawLandAlpha = landSamples / downsample;
      const landAlpha = size <= 32 && rawLandAlpha > 0 ? Math.min(1, 0.12 + rawLandAlpha * 1.05) : rawLandAlpha;
      const sphereAlpha = sphereSamples / downsample;
      const distance = Math.hypot(x + 0.5 - size / 2, y + 0.5 - size / 2);
      const edge = Math.max(0, Math.min(1, size * finalManifest.radiusRatio + 0.75 - distance));
      const rimWidth = Math.max(0.75, size / 128);
      const rim = Math.max(0, Math.min(1, (distance - (size * finalManifest.radiusRatio - rimWidth)) / rimWidth));
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

const staticFilename = (size, theme) => `routing-globe-static-${size}-${theme}.png`;
const inspectionFilename = (theme) => `routing-globe-static-inspection-${theme}.png`;
const staticHashes = {};
for (const [theme, palette] of Object.entries(palettes)) {
  const inspection = encodePng(INSPECTION_SIZE, INSPECTION_SIZE, composeFrame(renderLandCoverage(INSPECTION_SIZE, STATIC_CENTER_LONGITUDE), INSPECTION_SIZE, palette));
  const inspectionName = inspectionFilename(theme);
  await writeFile(join(previewDir, inspectionName), inspection);
  for (const size of finalManifest.sizes) {
    const name = staticFilename(size, theme);
    const png = encodePng(size, size, composeFrame(renderLandCoverage(size, STATIC_CENTER_LONGITUDE), size, palette));
    await writeFile(join(finalDir, name), png);
    await copyFile(join(finalDir, name), join(previewDir, name));
    staticHashes[name] = createHash("sha256").update(png).digest("hex");
  }
}

const updatedManifest = {
  ...finalManifest,
  staticFrameIndex: null,
  staticCenterLongitude: STATIC_CENTER_LONGITUDE,
  staticAxialTiltDegrees: STATIC_AXIAL_TILT_DEGREES,
  staticOrientation: "front-facing orthographic frame with no axial tilt",
  staticInspection: {
    light: inspectionFilename("light"),
    dark: inspectionFilename("dark"),
  },
  hashes: {
    ...finalManifest.hashes,
    ...staticHashes,
  },
  staticHashes,
};
await writeFile(join(finalDir, "manifest.json"), `${JSON.stringify(updatedManifest, null, 2)}\n`, "utf8");
await writeFile(join(previewDir, "manifest.json"), `${JSON.stringify(updatedManifest, null, 2)}\n`, "utf8");
console.log("Generated dedicated 0 degree static routing globe frames");
