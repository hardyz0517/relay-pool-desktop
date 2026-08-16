// @vitest-environment jsdom

import { act, type ReactElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const recordFrontendBoundaryFailure = vi.hoisted(() => vi.fn());
vi.mock("@/lib/bridge/generated", () => ({ recordFrontendBoundaryFailure }));

import { ShellPageErrorBoundary } from "./ShellPageErrorBoundary";

function ThrowingChild({ message = "fixture" }: { message?: string }): ReactElement {
  throw new Error(message);
}

describe("ShellPageErrorBoundary", () => {
  let host: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    host = document.createElement("div");
    document.body.appendChild(host);
    root = createRoot(host);
    recordFrontendBoundaryFailure.mockReset().mockResolvedValue(undefined);
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
  });

  it("recovers after retry and emits only the fixed boundary command", async () => {
    let shouldThrow = true;
    function Page() {
      return shouldThrow ? <ThrowingChild message="secret stack should stay local" /> : <div>healthy page</div>;
    }
    await act(async () => {
      root.render(
        <ShellPageErrorBoundary>
          <Page />
        </ShellPageErrorBoundary>,
      );
    });
    expect(host.querySelector('[role="alert"]')).not.toBeNull();
    expect(recordFrontendBoundaryFailure).toHaveBeenCalledTimes(1);
    expect(recordFrontendBoundaryFailure).toHaveBeenCalledWith();

    await act(async () => {
      shouldThrow = false;
      (host.querySelector("button") as HTMLButtonElement).click();
    });
    expect(host.textContent).toContain("healthy page");
  });

  it("does not recurse when the diagnostics command rejects", async () => {
    recordFrontendBoundaryFailure.mockRejectedValueOnce(new Error("transport failure"));
    await act(async () => {
      root.render(
        <ShellPageErrorBoundary>
          <ThrowingChild />
        </ShellPageErrorBoundary>,
      );
    });
    await act(async () => Promise.resolve());
    expect(host.querySelector('[role="alert"]')).not.toBeNull();
    expect(recordFrontendBoundaryFailure).toHaveBeenCalledTimes(1);
  });
});
