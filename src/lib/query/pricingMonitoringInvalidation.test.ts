import { describe, expect, it, vi } from "vitest";
import { invalidatePricingMonitoringQueries } from "./pricingMonitoringInvalidation";

describe("pricing monitoring invalidation", () => {
  it("invalidates pricing workspace and all summary variants", async () => {
    const invalidateQueries = vi.fn().mockResolvedValue(undefined);
    await invalidatePricingMonitoringQueries({ invalidateQueries } as never);

    expect(invalidateQueries).toHaveBeenCalledWith({ queryKey: ["pricing"] });
    expect(invalidateQueries).toHaveBeenCalledWith({
      queryKey: ["pricingGroupMonitorStatus"],
    });
    expect(invalidateQueries).toHaveBeenCalledWith({ queryKey: ["channelMonitoring"] });
    expect(invalidateQueries).toHaveBeenCalledWith({ queryKey: ["channelMonitors"] });
    expect(invalidateQueries).toHaveBeenCalledWith({ queryKey: ["channelMonitorLatestSummary"] });
    expect(invalidateQueries).toHaveBeenCalledWith({ queryKey: ["channelMonitorTemplates"] });
    expect(invalidateQueries).toHaveBeenCalledWith({ queryKey: ["channelStatus"] });
    expect(invalidateQueries).toHaveBeenCalledTimes(7);
  });
});
