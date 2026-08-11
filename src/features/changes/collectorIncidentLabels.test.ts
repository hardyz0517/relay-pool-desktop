import { describe, expect, it } from "vitest";
import { collectorFailureTaskLabel } from "./collectorIncidentLabels";

function incident(collectorFailedTaskTypes: string[]) {
  return {
    eventType: "collector_failed",
    collectorFailedTaskTypes,
  };
}

describe("collectorFailureTaskLabel", () => {
  it("combines balance and group failures into one label", () => {
    expect(collectorFailureTaskLabel(incident(["groups", "balance"]))).toBe(
      "余额、分组采集",
    );
  });

  it("keeps a single failed task precise", () => {
    expect(collectorFailureTaskLabel(incident(["groups"]))).toBe("分组采集");
  });

  it("ignores unsupported task types supplied by the transport", () => {
    expect(collectorFailureTaskLabel(incident(["unknown"]))).toBeNull();
  });
});
