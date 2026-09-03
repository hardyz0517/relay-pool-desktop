import {
  useCallback,
  useEffect,
  useLayoutEffect,
  useState,
  type RefCallback,
} from "react";

const SHELL_PAGE_SCROLL_CONTAINER_SELECTOR = "[data-shell-page-scroll-container]";
const useIsomorphicLayoutEffect = typeof window === "undefined" ? useEffect : useLayoutEffect;

type ShellPageVirtualizerTarget<T extends HTMLElement> = {
  targetRef: RefCallback<T>;
  scrollElement: HTMLElement | null;
  scrollMargin: number;
  resolved: boolean;
};

/**
 * Connects a virtualized result list to the shell page's native scroll surface.
 * The target must be the element whose top edge is the start of the virtual list.
 */
export function useShellPageVirtualizerTarget<T extends HTMLElement>(): ShellPageVirtualizerTarget<T> {
  const [target, setTarget] = useState<T | null>(null);
  const [scrollElement, setScrollElement] = useState<HTMLElement | null>(null);
  const [scrollMargin, setScrollMargin] = useState(0);
  const [resolved, setResolved] = useState(false);

  const targetRef = useCallback<RefCallback<T>>((node) => {
    setTarget(node);
  }, []);

  useIsomorphicLayoutEffect(() => {
    if (!target) {
      setScrollElement(null);
      setScrollMargin(0);
      setResolved(false);
      return;
    }

    const nextScrollElement = target.closest<HTMLElement>(
      SHELL_PAGE_SCROLL_CONTAINER_SELECTOR,
    );
    setScrollElement(nextScrollElement);
    setResolved(true);

    if (!nextScrollElement) {
      setScrollMargin(0);
      return;
    }

    const updateScrollMargin = () => {
      const scrollRect = nextScrollElement.getBoundingClientRect();
      // Retained background pages are display:none. Keep their last valid
      // measurement until the page becomes visible and ResizeObserver fires.
      if (scrollRect.width <= 0 && scrollRect.height <= 0) return;

      const targetRect = target.getBoundingClientRect();
      const nextMargin = Math.max(
        0,
        targetRect.top - scrollRect.top + nextScrollElement.scrollTop,
      );
      setScrollMargin((current) => (
        Math.abs(current - nextMargin) < 0.5 ? current : nextMargin
      ));
    };

    updateScrollMargin();

    const targetWindow = nextScrollElement.ownerDocument.defaultView;
    targetWindow?.addEventListener("resize", updateScrollMargin);

    const ResizeObserverConstructor = targetWindow?.ResizeObserver;
    const resizeObserver = ResizeObserverConstructor
      ? new ResizeObserverConstructor(updateScrollMargin)
      : null;
    const pageContent = target.closest<HTMLElement>(".app-page-transition-content");
    const observedElements = new Set<HTMLElement>([
      target,
      nextScrollElement,
      ...(pageContent ? [pageContent] : []),
    ]);
    observedElements.forEach((element) => resizeObserver?.observe(element));

    return () => {
      targetWindow?.removeEventListener("resize", updateScrollMargin);
      resizeObserver?.disconnect();
    };
  }, [target]);

  return { targetRef, scrollElement, scrollMargin, resolved };
}
