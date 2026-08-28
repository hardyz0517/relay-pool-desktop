import { driver as driverJs, type Config, type Driver, type DriveStep, type Popover } from "driver.js";
import type { TourDriverPort } from "./tourTypes";

/** The small subset of Driver.js used by the tour layer. */
export type DriverInstance = Pick<Driver, "highlight" | "destroy">;

/** Driver hooks are intentionally narrowed to no-arg callbacks at the adapter
 * boundary. Driver.js passes context arguments, but the tour layer does not
 * use them and keeping them out of the port makes fakes easier to write. */
export type DriverFactoryConfig = Omit<
  Config,
  "onNextClick" | "onPrevClick" | "onCloseClick" | "onDestroyed"
> & {
  onNextClick?: () => void;
  onPrevClick?: () => void;
  onCloseClick?: () => void;
  onDestroyed?: () => void;
};

export type DriverFactory = (config: DriverFactoryConfig) => DriverInstance;

export type TourDriverAdapterOptions = {
  create: DriverFactory;
  /** Optional class name used by the host application to theme the popover. */
  popoverClass?: string;
};

export type DriverDestroyReason = Parameters<TourDriverPort["destroy"]>[0];

/**
 * Driver.js lifecycle adapter. This module is the only place where a concrete
 * Driver.js factory should be injected, leaving TourManager free of DOM/vendor
 * concerns and straightforward to test with a fake factory.
 */
export class TourDriverAdapter implements TourDriverPort {
  private instance: DriverInstance | null = null;
  private callbacks: TourDriverPort["showStep"] extends (input: infer I) => void
    ? I extends { callbacks: infer C }
      ? C
      : never
    : never;
  private readonly ownedDestroyedInstances = new WeakSet<object>();
  private popoverClass?: string;
  private focusTarget: HTMLElement | null = null;

  constructor(options: TourDriverAdapterOptions) {
    this.callbacks = {} as typeof this.callbacks;
    this.create = options.create;
    this.popoverClass = options.popoverClass;
  }

  private readonly create: DriverFactory;

  beginSession(): void {
    // A new accepted session begins before preparation and navigation. Capture
    // once here; showStep is intentionally too late for cross-page tours.
    this.focusTarget =
      typeof document !== "undefined" && document.activeElement instanceof HTMLElement
        ? document.activeElement
        : null;
  }

  showStep(input: Parameters<TourDriverPort["showStep"]>[0]): void {
    this.destroy("step-change");
    const callbacks = input.callbacks;
    this.callbacks = callbacks;
    let createdInstance: DriverInstance | null = null;

    const instance = this.create({
      showProgress: true,
      allowClose: true,
      // A tour explains existing controls; it must not let a highlighted
      // business control submit, save, delete, or open a mutation flow.
      disableActiveInteraction: true,
      overlayClickBehavior: "close",
      onNextClick: () => callbacks.next(),
      onPrevClick: () => callbacks.previous(),
      onCloseClick: () => callbacks.close(),
      onDestroyed: () => {
        // A delayed callback from an old Driver instance must not affect the
        // current instance or steal focus from the user.
        if (this.instance !== createdInstance) return;
        const wasOwned = createdInstance !== null && this.ownedDestroyedInstances.has(createdInstance);
        if (createdInstance !== null) this.ownedDestroyedInstances.delete(createdInstance);
        this.instance = null;
        if (!wasOwned) {
          this.restoreFocus();
          callbacks.destroyed();
        }
      },
    });
    createdInstance = instance;
    this.instance = instance;

    const popover: Popover = {
      title: input.title,
      description: input.description,
      side: input.side ?? "bottom",
      align: input.align ?? "center",
      showButtons: ["previous", "next", "close"],
      nextBtnText: input.stepIndex >= input.stepCount - 1 ? "完成" : "下一步",
      prevBtnText: "上一步",
      doneBtnText: "完成",
      progressText: `${input.stepIndex + 1} / ${input.stepCount}`,
    };
    if (this.popoverClass) popover.popoverClass = this.popoverClass;
    const step: DriveStep = { element: input.element, popover };
    instance.highlight(step);
  }

  destroy(reason: DriverDestroyReason): void {
    const instance = this.instance;
    if (!instance) {
      // Creation/highlight may fail after beginSession but before an instance is
      // retained. A terminal exit must still complete focus restoration.
      if (reason !== "step-change") this.restoreFocus();
      return;
    }
    this.ownedDestroyedInstances.add(instance);
    try {
      instance.destroy();
    } finally {
      // Driver.js normally invokes onDestroyed synchronously, but a fake or a
      // future version may not. Release the reference regardless of callback.
      this.instance = null;
      this.callbacks = {} as typeof this.callbacks;
      if (reason !== "step-change") this.restoreFocus();
    }
  }

  private restoreFocus(): void {
    const target = this.focusTarget;
    this.focusTarget = null;
    if (!target?.isConnected || hasNonInteractiveAncestor(target)) return;
    try {
      target.focus({ preventScroll: true });
    } catch {
      target.focus();
    }
  }
}

function hasNonInteractiveAncestor(element: HTMLElement): boolean {
  for (let current: HTMLElement | null = element; current; current = current.parentElement) {
    if (
      current.hasAttribute("inert") ||
      ("inert" in current && current.inert) ||
      current.getAttribute("aria-hidden")?.toLowerCase() === "true"
    ) {
      return true;
    }
  }
  return false;
}

/**
 * Factory helper for the application composition root. Tests can still inject
 * a fake factory, while production uses the package import kept in this file.
 */
/** Production composition helper. Driver.js remains isolated to this module. */
export function createDriverJsAdapter(popoverClass?: string): TourDriverAdapter {
  return new TourDriverAdapter({
    popoverClass,
    create: (config) => driverJs(config),
  });
}
