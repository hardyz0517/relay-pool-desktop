export const PRICING_GROUP_MONITORING_SCHEMA_VERSION = 1 as const;
export const MAX_PRICING_GROUP_REFS = 500;

export type PricingGroupRefInput = {
  stationId: string;
  groupBindingId: string | null;
  groupIdHash: string | null;
  groupKeyHash: string;
};

export type CanonicalPricingGroupRef = PricingGroupRefInput & {
  canonicalKey: string;
};

export type PricingGroupRefErrorCode =
  | "invalid-input"
  | "duplicate-ref"
  | "unresolved-group"
  | "hash-mismatch";

export class PricingGroupRefError extends Error {
  readonly code: PricingGroupRefErrorCode;

  constructor(code: PricingGroupRefErrorCode, message: string) {
    super(message);
    this.name = "PricingGroupRefError";
    this.code = code;
  }
}

export function canonicalPricingGroupRefKey(input: PricingGroupRefInput): string {
  const stationId = input.stationId.trim();
  if (!stationId) {
    throw new PricingGroupRefError("invalid-input", "stationId must not be empty");
  }
  const bindingId = input.groupBindingId?.trim() ?? "";
  const groupIdHash = input.groupIdHash?.trim() ?? "";
  const groupKeyHash = input.groupKeyHash.trim();
  if (bindingId) return `station:${stationId}:binding:${bindingId}`;
  if (groupIdHash) return `station:${stationId}:group-id:${groupIdHash}`;
  if (groupKeyHash) return `station:${stationId}:group-key:${groupKeyHash}`;
  throw new PricingGroupRefError("unresolved-group", "group reference cannot be resolved");
}

export function normalizePricingGroupDisplayRefs(
  inputs: readonly PricingGroupRefInput[],
): CanonicalPricingGroupRef[] {
  if (inputs.length > MAX_PRICING_GROUP_REFS) {
    throw new PricingGroupRefError(
      "invalid-input",
      `group reference count exceeds ${MAX_PRICING_GROUP_REFS}`,
    );
  }
  const refs = inputs
    .map((input) => ({
      ...input,
      canonicalKey: canonicalPricingGroupRefKey(input),
    }))
    .sort((left, right) => compareUtf8(left.canonicalKey, right.canonicalKey));
  for (let index = 1; index < refs.length; index += 1) {
    if (refs[index - 1].canonicalKey === refs[index].canonicalKey) {
      throw new PricingGroupRefError(
        "duplicate-ref",
        `duplicate group reference: ${refs[index].canonicalKey}`,
      );
    }
  }
  return refs;
}

export function canonicalPricingGroupKeys(
  inputs: readonly PricingGroupRefInput[],
): string[] {
  return normalizePricingGroupDisplayRefs(inputs).map((ref) => ref.canonicalKey);
}

export async function hashPricingGroupDisplayRefs(
  inputs: readonly PricingGroupRefInput[],
): Promise<string> {
  const keys = canonicalPricingGroupKeys(inputs);
  const bytes = new TextEncoder().encode(keys.join("\n"));
  // Some Tauri WebView versions expose `crypto` but not `crypto.subtle`.
  // Keep the contract usable there without weakening the backend hash check.
  try {
    const subtle = globalThis.crypto?.subtle;
    if (subtle) {
      const digest = await subtle.digest("SHA-256", bytes);
      return [...new Uint8Array(digest)]
        .map((byte) => byte.toString(16).padStart(2, "0"))
        .join("");
    }
  } catch {
    // Fall through to the deterministic local implementation.
  }
  return sha256Hex(bytes);
}

const SHA256_K = [
  0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
  0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
  0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
  0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
  0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
  0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
  0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
  0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f,
  0x682e6ff3, 0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb,
  0xbef9a3f7, 0xc67178f2,
];

function sha256Hex(input: Uint8Array): string {
  const paddedLength = ((input.length + 9 + 63) >> 6) << 6;
  const padded = new Uint8Array(paddedLength);
  padded.set(input);
  padded[input.length] = 0x80;
  const bitLength = input.length * 8;
  padded[paddedLength - 4] = (bitLength >>> 24) & 0xff;
  padded[paddedLength - 3] = (bitLength >>> 16) & 0xff;
  padded[paddedLength - 2] = (bitLength >>> 8) & 0xff;
  padded[paddedLength - 1] = bitLength & 0xff;

  let h0 = 0x6a09e667;
  let h1 = 0xbb67ae85;
  let h2 = 0x3c6ef372;
  let h3 = 0xa54ff53a;
  let h4 = 0x510e527f;
  let h5 = 0x9b05688c;
  let h6 = 0x1f83d9ab;
  let h7 = 0x5be0cd19;

  for (let offset = 0; offset < padded.length; offset += 64) {
    const words = new Uint32Array(64);
    for (let index = 0; index < 16; index += 1) {
      const position = offset + index * 4;
      words[index] =
        (padded[position] << 24) |
        (padded[position + 1] << 16) |
        (padded[position + 2] << 8) |
        padded[position + 3];
    }
    for (let index = 16; index < 64; index += 1) {
      const valueA = words[index - 15];
      const valueB = words[index - 2];
      const smallSigma0 = rotateRight(valueA, 7) ^ rotateRight(valueA, 18) ^ (valueA >>> 3);
      const smallSigma1 = rotateRight(valueB, 17) ^ rotateRight(valueB, 19) ^ (valueB >>> 10);
      words[index] = (words[index - 16] + smallSigma0 + words[index - 7] + smallSigma1) >>> 0;
    }

    let a = h0;
    let b = h1;
    let c = h2;
    let d = h3;
    let e = h4;
    let f = h5;
    let g = h6;
    let h = h7;
    for (let index = 0; index < 64; index += 1) {
      const bigSigma1 = rotateRight(e, 6) ^ rotateRight(e, 11) ^ rotateRight(e, 25);
      const choice = (e & f) ^ (~e & g);
      const temporary1 = (h + bigSigma1 + choice + SHA256_K[index] + words[index]) >>> 0;
      const bigSigma0 = rotateRight(a, 2) ^ rotateRight(a, 13) ^ rotateRight(a, 22);
      const majority = (a & b) ^ (a & c) ^ (b & c);
      const temporary2 = (bigSigma0 + majority) >>> 0;
      h = g;
      g = f;
      f = e;
      e = (d + temporary1) >>> 0;
      d = c;
      c = b;
      b = a;
      a = (temporary1 + temporary2) >>> 0;
    }
    h0 = (h0 + a) >>> 0;
    h1 = (h1 + b) >>> 0;
    h2 = (h2 + c) >>> 0;
    h3 = (h3 + d) >>> 0;
    h4 = (h4 + e) >>> 0;
    h5 = (h5 + f) >>> 0;
    h6 = (h6 + g) >>> 0;
    h7 = (h7 + h) >>> 0;
  }

  return [h0, h1, h2, h3, h4, h5, h6, h7]
    .map((value) => value.toString(16).padStart(8, "0"))
    .join("");
}

function rotateRight(value: number, bits: number): number {
  return (value >>> bits) | (value << (32 - bits));
}

function compareUtf8(left: string, right: string) {
  const leftBytes = new TextEncoder().encode(left);
  const rightBytes = new TextEncoder().encode(right);
  const length = Math.min(leftBytes.length, rightBytes.length);
  for (let index = 0; index < length; index += 1) {
    if (leftBytes[index] !== rightBytes[index]) return leftBytes[index] - rightBytes[index];
  }
  return leftBytes.length - rightBytes.length;
}
