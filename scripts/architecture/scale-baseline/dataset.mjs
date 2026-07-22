import { createHash } from "node:crypto";

export const DATASET_VERSION = "architecture-scale-dataset-v1";
export const DATASET_SEED = 0x5a17c0de;
export const DATASET_SIZES = [10, 100, 500];

function generator(seed) {
  let state = seed >>> 0;
  return () => {
    state = (Math.imul(state, 1664525) + 1013904223) >>> 0;
    return state / 0x1_0000_0000;
  };
}

export function generateDataset(size) {
  if (!DATASET_SIZES.includes(size)) throw new Error(`unsupported scale dataset size: ${size}`);
  const random = generator(DATASET_SEED ^ size);
  const stations = Array.from({ length: size }, (_, index) => {
    const ordinal = String(index + 1).padStart(4, "0");
    const keyCount = 1 + Math.floor(random() * 3);
    return {
      id: `station-${ordinal}`,
      name: `Fixture Station ${ordinal}`,
      provider: ["newapi", "sub2api", "openai-compatible"][index % 3],
      enabled: index % 7 !== 0,
      endpointRevision: 1 + (index % 5),
      keys: Array.from({ length: keyCount }, (_, keyIndex) => ({
        id: `key-${ordinal}-${keyIndex + 1}`,
        label: `Fixture Key ${ordinal}-${keyIndex + 1}`,
        enabled: keyIndex === 0 || random() > 0.25,
        maskedCredential: "sk-fixture-...redacted",
      })),
    };
  });
  return { version: DATASET_VERSION, seed: DATASET_SEED, size, stations };
}

export function canonicalJson(value) {
  return `${JSON.stringify(value)}\n`;
}

export function sha256(value) {
  return createHash("sha256").update(value).digest("hex");
}

export function generateFixtureManifest() {
  const datasets = Object.fromEntries(DATASET_SIZES.map((size) => {
    const value = generateDataset(size);
    const json = canonicalJson(value);
    return [String(size), { sha256: sha256(json), json_bytes: Buffer.byteLength(json), value }];
  }));
  return {
    schema_version: 1,
    dataset_version: DATASET_VERSION,
    seed: DATASET_SEED,
    datasets,
  };
}
