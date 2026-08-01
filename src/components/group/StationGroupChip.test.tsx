import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { StationGroupOptionLabel, StationGroupTriggerLabel } from "./StationGroupChip";

describe("station group select labels", () => {
  it("uses a verbose multiplier in the menu and a compact multiplier in the trigger", () => {
    const option = {
      groupName: "plus",
      rateMultiplier: 0.5,
    };

    expect(renderToStaticMarkup(<StationGroupOptionLabel option={option} />)).toContain(
      "0.5x 倍率",
    );

    const triggerMarkup = renderToStaticMarkup(<StationGroupTriggerLabel option={option} />);
    expect(triggerMarkup).toContain("0.5x");
    expect(triggerMarkup).not.toContain("0.5x 倍率");
  });
});
