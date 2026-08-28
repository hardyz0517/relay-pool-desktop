import type { AppPageId } from "@/lib/types/navigation";
import type { TourTargetResolver as TourTargetResolverPort } from "@/app/tours/tourTypes";

export type TourTargetResolverErrorCode = "invalid-anchor" | "aborted" | "timeout";

/**
 * Errors returned by the target resolver are intentionally distinguishable so
 * the manager can turn an unavailable target into a required/optional step
 * result without exposing a DOM exception to the application shell.
 */
export class TourTargetResolverError extends Error {
  readonly code: TourTargetResolverErrorCode;
  readonly anchor: string;
  readonly route: AppPageId;

  constructor(
    code: TourTargetResolverErrorCode,
    anchor: string,
    route: AppPageId,
  ) {
    const message =
      code === "timeout"
        ? `Tour target timed out: ${anchor}`
        : code === "aborted"
          ? `Tour target wait aborted: ${anchor}`
          : `Invalid tour target anchor: ${anchor}`;
    super(message);
    this.name = "TourTargetResolverError";
    this.code = code;
    this.anchor = anchor;
    this.route = route;
  }
}

export type TourTargetResolverOptions = {
  /** Root used for target lookup. Defaults to the current document. */
  root?: Document | HTMLElement;
  /** Maximum time to wait for a target. Defaults to the plan's three seconds. */
  timeoutMs?: number;
};

const DEFAULT_TIMEOUT_MS = 3_000;
const MAX_ANCHOR_LENGTH = 256;

/**
 * Escapes an anchor for use in an attribute selector. CSS.escape is available
 * in browser WebViews, while the small fallback keeps tests and older WebViews
 * safe without interpolating untrusted text into a selector.
 */
export function escapeCssIdent(value: string): string {
  if (typeof value !== "string") {
    return "";
  }
  const cssEscape = (globalThis as { CSS?: { escape?: (input: string) => string } }).CSS?.escape;
  if (typeof cssEscape === "function") {
    return cssEscape(value);
  }

  let result = "";
  for (let index = 0; index < value.length; index += 1) {
    const codePoint = value.charCodeAt(index);
    const character = value[index];

    if (codePoint === 0) {
      result += "\uFFFD";
      continue;
    }

    const isControl =
      (codePoint >= 1 && codePoint <= 31) || codePoint === 127;
    const isFirstDigit = index === 0 && codePoint >= 48 && codePoint <= 57;
    const isSecondDigitAfterHyphen =
      index === 1 && value[0] === "-" && codePoint >= 48 && codePoint <= 57;

    if (isControl || isFirstDigit || isSecondDigitAfterHyphen) {
      result += `\\${codePoint.toString(16)} `;
      continue;
    }

    if (
      index === 0 &&
      character === "-" &&
      value.length === 1
    ) {
      result += "\\-";
      continue;
    }

    if (
      codePoint >= 128 ||
      character === "-" ||
      character === "_" ||
      (codePoint >= 48 && codePoint <= 57) ||
      (codePoint >= 65 && codePoint <= 90) ||
      (codePoint >= 97 && codePoint <= 122)
    ) {
      result += character;
      continue;
    }

    result += `\\${character}`;
  }
  return result;
}

function normaliseAnchor(anchor: string): string {
  if (typeof anchor !== "string") {
    return "";
  }
  const normalised = anchor.trim();
  if (normalised.length === 0 || normalised.length > MAX_ANCHOR_LENGTH) {
    return "";
  }
  return normalised;
}

function getOwnerDocument(root: Document | HTMLElement): Document | null {
  if (root.nodeType === 9) {
    return root as Document;
  }
  return root.ownerDocument;
}

function isAriaHidden(element: Element): boolean {
  for (let current: Element | null = element; current; current = current.parentElement) {
    if (current.getAttribute("aria-hidden")?.toLowerCase() === "true") {
      return true;
    }
  }
  return false;
}

function isInert(element: Element): boolean {
  for (let current: Element | null = element; current; current = current.parentElement) {
    if (current.hasAttribute("inert")) {
      return true;
    }
    if ("inert" in current && (current as HTMLElement).inert) {
      return true;
    }
  }
  return false;
}

function hasHiddenStyle(element: Element, ownerDocument: Document): boolean {
  let current: Element | null = element;
  while (current) {
    if (current.hasAttribute("hidden")) {
      return true;
    }
    const style = ownerDocument.defaultView?.getComputedStyle(current);
    if (style) {
      if (
        style.display === "none" ||
        style.visibility === "hidden" ||
        style.visibility === "collapse" ||
        Number.parseFloat(style.opacity || "1") === 0
      ) {
        return true;
      }
    }
    current = current.parentElement;
  }
  return false;
}

function hasLayout(element: HTMLElement): boolean {
  const rect = element.getBoundingClientRect();
  return rect.width > 0 && rect.height > 0;
}

function isActiveLayer(
  element: Element,
  route: AppPageId,
  root: Document | HTMLElement,
): boolean {
  const layer = element.closest<HTMLElement>("[data-page-transition-layer]");
  if (!layer) {
    const globalScope = element.closest<HTMLElement>('[data-tour-scope="global"]');
    return Boolean(globalScope && (root.nodeType === 9 || root.contains(globalScope)));
  }
  if (layer.dataset.pageTransitionPageId !== route) {
    return false;
  }
  if (root.nodeType !== 9 && !root.contains(layer)) {
    return false;
  }

  const kind = layer.dataset.pageTransitionKind;
  const state = layer.dataset.pageTransitionState;
  // Shell entering is still the foreground layer and is allowed while its
  // page-ready signal settles. Leaving/background/inactive layers are rejected.
  return (
    (kind === "shell" && (state === "active" || state === "entering")) ||
    (kind === "transient" && state === "active")
  );
}

function isCandidateVisible(
  element: Element,
  route: AppPageId,
  root: Document | HTMLElement,
  ownerDocument: Document,
): element is HTMLElement {
  if (!(element instanceof HTMLElement)) {
    return false;
  }
  if (!element.isConnected || !isActiveLayer(element, route, root)) {
    return false;
  }
  if (isInert(element) || isAriaHidden(element) || hasHiddenStyle(element, ownerDocument)) {
    return false;
  }
  return hasLayout(element);
}

export class TourTargetResolver implements TourTargetResolverPort {
  private readonly root: Document | HTMLElement | null;
  private readonly timeoutMs: number;

  constructor(options: TourTargetResolverOptions = {}) {
    this.root =
      options.root ??
      (typeof document !== "undefined" ? document : null);
    this.timeoutMs = Number.isFinite(options.timeoutMs)
      ? Math.max(0, options.timeoutMs ?? DEFAULT_TIMEOUT_MS)
      : DEFAULT_TIMEOUT_MS;
  }

  resolveTarget(anchor: string, route: AppPageId): HTMLElement | null {
    const normalisedAnchor = normaliseAnchor(anchor);
    if (!normalisedAnchor || !this.root) {
      return null;
    }

    const ownerDocument = getOwnerDocument(this.root);
    if (!ownerDocument) {
      return null;
    }

    const selector = `[data-tour="${escapeCssIdent(normalisedAnchor)}"]`;
    let candidates: NodeListOf<Element>;
    try {
      candidates = this.root.querySelectorAll(selector);
    } catch {
      // A malformed selector must never escape into the business application.
      return null;
    }

    for (const candidate of candidates) {
      if (isCandidateVisible(candidate, route, this.root, ownerDocument)) {
        return candidate;
      }
    }
    return null;
  }

  waitForTarget(
    anchor: string,
    route: AppPageId,
    signal: AbortSignal,
  ): Promise<HTMLElement> {
    const normalisedAnchor = normaliseAnchor(anchor);
    if (!normalisedAnchor) {
      return Promise.reject(
        new TourTargetResolverError("invalid-anchor", anchor, route),
      );
    }

    if (signal.aborted) {
      return Promise.reject(
        new TourTargetResolverError("aborted", normalisedAnchor, route),
      );
    }

    return new Promise<HTMLElement>((resolve, reject) => {
      const root = this.root;
      let settled = false;
      let timeoutHandle: ReturnType<typeof setTimeout> | null = null;
      let frameHandle: number | null = null;
      let frameTimerHandle: ReturnType<typeof setTimeout> | null = null;
      let observer: MutationObserver | null = null;

      const win = root
        ? getOwnerDocument(root)?.defaultView ?? null
        : typeof window !== "undefined"
          ? window
          : null;

      const cleanup = () => {
        if (timeoutHandle !== null) {
          clearTimeout(timeoutHandle);
          timeoutHandle = null;
        }
        if (frameHandle !== null) {
          win?.cancelAnimationFrame(frameHandle);
          frameHandle = null;
        }
        if (frameTimerHandle !== null) {
          clearTimeout(frameTimerHandle);
          frameTimerHandle = null;
        }
        observer?.disconnect();
        observer = null;
        signal.removeEventListener("abort", handleAbort);
      };

      const finish = (callback: () => void) => {
        if (settled) {
          return;
        }
        settled = true;
        cleanup();
        callback();
      };

      const handleAbort = () => {
        finish(() =>
          reject(new TourTargetResolverError("aborted", normalisedAnchor, route)),
        );
      };

      const check = () => {
        if (settled) {
          return;
        }
        const target = this.resolveTarget(normalisedAnchor, route);
        if (target) {
          finish(() => resolve(target));
          return;
        }
        scheduleFrame();
      };

      const onFrame = () => {
        frameHandle = null;
        frameTimerHandle = null;
        check();
      };

      const scheduleFrame = () => {
        if (settled || frameHandle !== null || frameTimerHandle !== null) {
          return;
        }
        if (typeof win?.requestAnimationFrame === "function") {
          frameHandle = win.requestAnimationFrame(onFrame);
        } else {
          frameTimerHandle = setTimeout(onFrame, 16);
        }
      };

      if (root) {
        const mutationObserverCtor =
          win?.MutationObserver ??
          (typeof MutationObserver !== "undefined" ? MutationObserver : null);
        if (mutationObserverCtor) {
          observer = new mutationObserverCtor(() => {
            check();
          });
          observer.observe(root, {
            subtree: true,
            childList: true,
            attributes: true,
          });
        }
      }

      signal.addEventListener("abort", handleAbort, { once: true });
      timeoutHandle = setTimeout(() => {
        finish(() =>
          reject(new TourTargetResolverError("timeout", normalisedAnchor, route)),
        );
      }, this.timeoutMs);
      check();
    });
  }
}
