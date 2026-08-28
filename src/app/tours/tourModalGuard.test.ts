// @vitest-environment jsdom

import { afterEach, describe, expect, it } from "vitest";
import { hasBlockingBusinessModal } from "./tourModalGuard";

describe("hasBlockingBusinessModal", () => {
  afterEach(() => {
    document.body.replaceChildren();
  });

  it("ignores the Driver.js popover dialog", () => {
    const popover = document.createElement("div");
    popover.className = "driver-popover";
    popover.setAttribute("role", "dialog");
    document.body.append(popover);

    expect(hasBlockingBusinessModal()).toBe(false);
  });

  it("ignores the application-owned tour popover class", () => {
    const popover = document.createElement("div");
    popover.className = "relay-pool-tour-popover";
    popover.setAttribute("role", "dialog");
    document.body.append(popover);

    expect(hasBlockingBusinessModal()).toBe(false);
  });

  it("blocks when a business dialog is open", () => {
    const dialog = document.createElement("div");
    dialog.setAttribute("role", "dialog");
    document.body.append(dialog);

    expect(hasBlockingBusinessModal()).toBe(true);
  });

  it("still blocks when a business dialog and tour popover coexist", () => {
    const businessDialog = document.createElement("div");
    businessDialog.setAttribute("role", "dialog");
    const popover = document.createElement("div");
    popover.className = "driver-popover";
    popover.setAttribute("role", "dialog");
    document.body.append(businessDialog, popover);

    expect(hasBlockingBusinessModal()).toBe(true);
  });

  it("blocks explicitly marked application modals even without a dialog role", () => {
    const modal = document.createElement("div");
    modal.dataset.tourBlocking = "true";
    document.body.append(modal);

    expect(hasBlockingBusinessModal()).toBe(true);
  });

  it("blocks an active transient page", () => {
    const transient = document.createElement("div");
    transient.dataset.pageTransitionKind = "transient";
    transient.dataset.pageTransitionState = "active";
    document.body.append(transient);

    expect(hasBlockingBusinessModal()).toBe(true);
  });
});
