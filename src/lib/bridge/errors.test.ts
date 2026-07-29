import { describe, expect, it } from "vitest";
import { BackendError, ResultUnknownError } from "./errors";

describe("BackendError", () => {
  it("uses stable code/details and never branches on message text", () => {
    const error = BackendError.fromUnknown({
      code: "conflict",
      message: "The resource changed.",
      retryable: false,
      details: { kind: "conflict", resource: "station", currentRevision: "7" },
      correlationId: "corr_123",
    });
    expect(error.code).toBe("conflict");
    expect(error.details?.kind).toBe("conflict");
    expect(error.correlationId).toBe("corr_123");
  });

  it("maps unknown codes and malicious details to a non-retryable internal error", () => {
    const error = BackendError.fromUnknown({
      code: "future_code",
      message: "https://example.test/?token=secret",
      retryable: true,
      details: { kind: "external", provider: "https://provider.test", upstreamStatus: 200 },
      correlationId: "corr_future_1",
    });
    expect(error.code).toBe("internal");
    expect(error.retryable).toBe(false);
    expect(error.message).toBe("The desktop operation failed.");
    expect(error.details).toBeUndefined();
    expect(error.correlationId).toBe("corr_future_1");
  });

  it("does not parse arbitrary transport strings as business errors", () => {
    expect(BackendError.fromUnknown("runtime unavailable").code).toBe("internal");
  });

  it("enforces the Rust UTF-8 byte limit rather than JavaScript character count", () => {
    const error = BackendError.fromUnknown({
      code: "invalid_input",
      message: "界".repeat(200),
      retryable: false,
    });
    expect(error.message).toBe("The desktop operation failed.");
  });

  it.each(["failed at /home/user/private.db", "failed at C:/Users/private/data.db"])(
    "rejects absolute paths in public text: %s",
    (message) => {
      expect(BackendError.fromUnknown({ code: "not_found", message }).message).toBe(
        "The desktop operation failed.",
      );
    },
  );
});

describe("ResultUnknownError", () => {
  it("exposes a stable typed terminal without leaking the transport message", () => {
    const error = new ResultUnknownError("create_station", new Error("socket closed with token=secret"));
    expect(error.name).toBe("ResultUnknownError");
    expect(error.command).toBe("create_station");
    expect(error.message).toBe(
      "The desktop operation may have completed, but its result could not be confirmed.",
    );
  });
});
