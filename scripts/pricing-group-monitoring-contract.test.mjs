import { createHash } from "node:crypto";
import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const fixturePath = path.join(root, "tests", "contracts", "pricing-group-monitoring.v1.json");
const fixture = JSON.parse(await readFile(fixturePath, "utf8"));

const SECRET_FIELD_PATTERN = /api.?key|cookie|authorization|token|password|secret|response.?body/i;
const MAX_GROUP_REFS = 500;

function assert(condition, message) {
  if (!condition) throw new Error(message);
}

function normalize(value) {
  return typeof value === "string" ? value.trim() : "";
}

function canonicalRef(group) {
  const stationId = normalize(group.stationId);
  const bindingId = normalize(group.groupBindingId);
  const groupIdHash = normalize(group.groupIdHash);
  const groupKeyHash = normalize(group.groupKeyHash);
  assert(stationId, "stationId must be non-empty");
  if (bindingId) return `station:${stationId}:binding:${bindingId}`;
  if (groupIdHash) return `station:${stationId}:group-id:${groupIdHash}`;
  if (groupKeyHash) return `station:${stationId}:group-key:${groupKeyHash}`;
  return null;
}

function canonicalize(groups) {
  assert(Array.isArray(groups), "groups must be an array");
  assert(groups.length <= MAX_GROUP_REFS, `groups must not exceed ${MAX_GROUP_REFS}`);
  const refs = groups.map(canonicalRef).filter(Boolean);
  refs.sort((left, right) =>
    Buffer.from(left, "utf8").compare(Buffer.from(right, "utf8")),
  );
  for (let index = 1; index < refs.length; index += 1) {
    assert(refs[index] !== refs[index - 1], `duplicate group reference: ${refs[index]}`);
  }
  return refs;
}

function hashRefs(refs) {
  return createHash("sha256").update(refs.join("\n"), "utf8").digest("hex");
}

function assertNoSecrets(value, pathLabel = "$") {
  if (Array.isArray(value)) {
    value.forEach((item, index) => assertNoSecrets(item, `${pathLabel}[${index}]`));
    return;
  }
  if (!value || typeof value !== "object") return;
  for (const [key, child] of Object.entries(value)) {
    assert(!SECRET_FIELD_PATTERN.test(key), `${pathLabel}.${key} looks like secret material`);
    assertNoSecrets(child, `${pathLabel}.${key}`);
  }
}

assert(fixture.schemaVersion === 1, "contract schemaVersion must be 1");
assert(fixture.maxGroupRefs === MAX_GROUP_REFS, "fixture maxGroupRefs drifted");
assert(Array.isArray(fixture.cases), "fixture cases must be an array");
assertNoSecrets(fixture);

for (const testCase of fixture.cases) {
  const refs = canonicalize(testCase.input);
  assert(JSON.stringify(refs) === JSON.stringify(testCase.expectedRefs), `${testCase.name}: canonical refs mismatch`);
  if (testCase.expectedHash) {
    assert(hashRefs(refs) === testCase.expectedHash, `${testCase.name}: hash mismatch`);
  }
}

assertThrows(() => canonicalize(fixture.boundaryCases.duplicateInput), "duplicate refs must be rejected");
assertThrows(() => canonicalize(Array.from({ length: fixture.boundaryCases.overLimitCount }, (_, index) => ({
  stationId: `station-${index}`,
  groupBindingId: null,
  groupIdHash: null,
  groupKeyHash: `group-${index}`,
}))), "over-limit input must be rejected");

assert(hashRefs([]) === createHash("sha256").update("", "utf8").digest("hex"), "empty hash must be deterministic");

console.log(`pricing group monitoring contract passed (${fixture.cases.length} cases)`);

function assertThrows(callback, message) {
  let threw = false;
  try {
    callback();
  } catch {
    threw = true;
  }
  assert(threw, message);
}
