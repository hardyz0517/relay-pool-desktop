import { afterEach, describe, expect, it, vi } from "vitest";
import {
  canonicalPricingGroupKeys,
  normalizePricingGroupDisplayRefs,
  hashPricingGroupDisplayRefs,
  PricingGroupRefError,
} from "./pricingGroupRefs";

const ref = (overrides: Partial<Parameters<typeof canonicalPricingGroupKeys>[0][number]> = {}) => ({
  stationId: "station-1",
  groupBindingId: "binding-1",
  groupIdHash: "group-id-1",
  groupKeyHash: "group-key-1",
  ...overrides,
});

describe("pricing group refs", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("uses binding, group id, then group key fallback", () => {
    expect(canonicalPricingGroupKeys([ref()])).toEqual(["station:station-1:binding:binding-1"]);
    expect(canonicalPricingGroupKeys([ref({ groupBindingId: null })])).toEqual([
      "station:station-1:group-id:group-id-1",
    ]);
    expect(
      canonicalPricingGroupKeys([ref({ groupBindingId: null, groupIdHash: null })]),
    ).toEqual(["station:station-1:group-key:group-key-1"]);
  });

  it("sorts by UTF-8 bytes and rejects duplicates", () => {
    expect(
      canonicalPricingGroupKeys([
        ref({ groupBindingId: "z" }),
        ref({ groupBindingId: "a" }),
      ]),
    ).toEqual([
      "station:station-1:binding:a",
      "station:station-1:binding:z",
    ]);
    expect(() => canonicalPricingGroupKeys([ref(), ref()])).toThrowError(PricingGroupRefError);
  });

  it("does not merge same names with different stable identities", () => {
    expect(
      canonicalPricingGroupKeys([
        ref({ groupBindingId: null, groupIdHash: "a" }),
        ref({ groupBindingId: null, groupIdHash: "b" }),
      ]),
    ).toHaveLength(2);
  });

  it("matches the deterministic empty-input SHA-256", async () => {
    await expect(hashPricingGroupDisplayRefs([])).resolves.toBe(
      "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
    );
  });

  it("hashes correctly when Web Crypto is unavailable in the WebView", async () => {
    vi.stubGlobal("crypto", undefined);
    await expect(hashPricingGroupDisplayRefs([ref()])).resolves.toBe(
      "5af1e07213f2259d1386c4bcee7e31027f72fe30532095ff56a7a3a54f28afb5",
    );
  });

  it("rejects unresolved and over-limit input", () => {
    expect(() =>
      canonicalPricingGroupKeys([
        ref({ groupBindingId: null, groupIdHash: null, groupKeyHash: "" }),
      ]),
    ).toThrowError(PricingGroupRefError);
    expect(() =>
      canonicalPricingGroupKeys(
        Array.from({ length: 501 }, (_, index) => ref({ groupBindingId: `binding-${index}` })),
      ),
    ).toThrowError(PricingGroupRefError);
  });

  it("returns a stable canonical projection", () => {
    expect(normalizePricingGroupDisplayRefs([ref()])).toEqual([
      { ...ref(), canonicalKey: "station:station-1:binding:binding-1" },
    ]);
  });
});
