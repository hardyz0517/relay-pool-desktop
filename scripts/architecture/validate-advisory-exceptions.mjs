import { assert, parseIsoDate, readRequiredManifest, runMain } from "./lib.mjs";

const ENTRY_KEYS = new Set([
  "ecosystem",
  "package",
  "advisory_id",
  "affected_assessment",
  "owner",
  "approved_on",
  "expires_on",
  "rationale",
]);

runMain(() => {
  const manifest = readRequiredManifest("docs/superpowers/audits/dependency-advisory-exceptions.json", ["schema_version", "exceptions"]);
  assert(manifest.schema_version === 1, "advisory exception schema_version must be 1");
  assert(Array.isArray(manifest.exceptions), "advisory exceptions must be an array");
  const today = new Date();
  today.setUTCHours(0, 0, 0, 0);
  const identities = new Set();
  for (const [index, entry] of manifest.exceptions.entries()) {
    assert(entry && typeof entry === "object" && !Array.isArray(entry), `exceptions[${index}] must be an object`);
    const unknown = Object.keys(entry).filter((key) => !ENTRY_KEYS.has(key));
    assert(unknown.length === 0, `exceptions[${index}] has unknown fields: ${unknown.join(", ")}`);
    for (const key of ENTRY_KEYS) assert(typeof entry[key] === "string" && entry[key].trim(), `exceptions[${index}].${key} is required`);
    assert(["npm", "cargo"].includes(entry.ecosystem), `exceptions[${index}].ecosystem must be npm or cargo`);
    assert(!/[!*]/.test(entry.package), `exceptions[${index}] must identify one exact package`);
    assert(!/[!*]/.test(entry.advisory_id), `exceptions[${index}] must identify one exact advisory`);
    assert(/^(?:not_affected|mitigated|accepted)$/.test(entry.affected_assessment), `exceptions[${index}].affected_assessment is invalid`);
    const approved = parseIsoDate(entry.approved_on, `exceptions[${index}].approved_on`);
    const expires = parseIsoDate(entry.expires_on, `exceptions[${index}].expires_on`);
    assert(approved <= today, `exceptions[${index}] approval is in the future`);
    assert(expires >= today, `exceptions[${index}] is expired`);
    const identity = `${entry.ecosystem}:${entry.package}:${entry.advisory_id}`;
    assert(!identities.has(identity), `duplicate advisory exception ${identity}`);
    identities.add(identity);
  }
  console.log(`Advisory exception manifest passed (${manifest.exceptions.length} exceptions)`);
});
