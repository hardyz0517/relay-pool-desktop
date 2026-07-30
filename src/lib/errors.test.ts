import { describe, expect, it } from "vitest";

import { BackendError } from "@/lib/bridge/errors";
import { readError } from "./errors";

describe("readError", () => {
  it("includes safe backend validation details", () => {
    const error = new BackendError(
      "invalid_input",
      "The command input is invalid.",
      false,
      {
        kind: "validation",
        fields: [
          {
            field: "input",
            code: "unknown_field",
            message: "The station key payload contains an unknown field.",
          },
        ],
      },
    );

    expect(readError(error)).toBe(
      "The command input is invalid. (input: The station key payload contains an unknown field.)",
    );
  });

  it("keeps ordinary error messages unchanged", () => {
    expect(readError(new Error("request failed"))).toBe("request failed");
  });
});
