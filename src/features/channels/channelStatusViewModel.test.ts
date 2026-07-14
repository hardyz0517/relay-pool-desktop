import { describe, expect, it } from "vitest";

import { availabilityTone } from "./channelStatusViewModel";

describe("availabilityTone", () => {
  it.each([
    [{ status: "disabled", availabilityPercent: 99 }, "muted"],
    [{ status: "healthy", availabilityPercent: null }, "muted"],
    [{ status: "healthy", availabilityPercent: 49.9 }, "danger"],
    [{ status: "healthy", availabilityPercent: 50 }, "warning"],
    [{ status: "healthy", availabilityPercent: 74.9 }, "warning"],
    [{ status: "healthy", availabilityPercent: 75 }, "success"],
  ] as const)("maps %o to %s", (channel, tone) => {
    expect(availabilityTone(channel)).toBe(tone);
  });
});
