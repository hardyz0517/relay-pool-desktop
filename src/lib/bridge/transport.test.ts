import { describe, expect, it } from "vitest";

import { BackendError, ResultUnknownError } from "./errors";
import { classifyNonIdempotentRejection } from "./transport";

describe("non-idempotent transport", () => {
  it("preserves typed backend failures as confirmed failures", async () => {
    const error = classifyNonIdempotentRejection("create_station", {
      code: "conflict",
      message: "The station already exists.",
      retryable: false,
    });
    expect(error).toBeInstanceOf(BackendError);
  });

  it("returns result unknown for an ambiguous post-dispatch rejection", async () => {
    const error = classifyNonIdempotentRejection(
      "create_station",
      new Error("response channel closed"),
    );
    expect(error).toBeInstanceOf(ResultUnknownError);
  });
});
