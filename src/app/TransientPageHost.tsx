import { AnimatePresence, motion, MotionConfig, useIsPresent } from "framer-motion";
import { useCallback, useLayoutEffect, useRef, type ReactNode } from "react";
import {
  completeTransientPageExit,
  type TransientPageExitSnapshot,
} from "@/app/transientPageExitPolicy";
import { PageActivityProvider } from "@/components/shell/PageActivity";
import type { TransientPageId } from "@/lib/types/navigation";

declare module "react" {
  interface HTMLAttributes<T> {
    inert?: "" | undefined;
  }
}

export type TransientPageDescriptor = {
  pageId: TransientPageId;
  instanceKey: string;
  node: ReactNode;
};

type TransientPageHostProps = {
  page: TransientPageDescriptor | null;
  onExitComplete?: () => void;
};

const transientPageTransition = {
  duration: 0.2,
};

const ACTIONABLE_ELEMENT_SELECTOR = [
  "button:not([disabled])",
  "a[href]",
  'input:not([disabled]):not([type="hidden"])',
  "select:not([disabled])",
  "textarea:not([disabled])",
  '[tabindex]:not([tabindex^="-"])',
].join(", ");

function TransientPageLayer({ page }: { page: TransientPageDescriptor }) {
  const isPresent = useIsPresent();
  const rootRef = useRef<HTMLDivElement>(null);

  useLayoutEffect(() => {
    const root = rootRef.current;
    if (!root) {
      return;
    }

    const focusTarget =
      root.querySelector<HTMLElement>("[data-page-autofocus]") ??
      root.querySelector<HTMLElement>(ACTIONABLE_ELEMENT_SELECTOR);
    focusTarget?.focus({ preventScroll: true });
  }, []);

  return (
    <div
      ref={rootRef}
      className="app-page-transition-layer app-page-transition-overlay"
      data-page-transition-layer
      data-page-transition-kind="transient"
      data-page-transition-page-id={page.pageId}
      data-page-transition-state={isPresent ? "active" : "exiting"}
    >
      <PageActivityProvider active={isPresent}>
        <motion.div
          aria-hidden={!isPresent}
          className="app-page-transition-content"
          inert={isPresent ? undefined : ""}
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          exit={{ opacity: 0 }}
          transition={transientPageTransition}
        >
          {page.node}
        </motion.div>
      </PageActivityProvider>
    </div>
  );
}

export function TransientPageHost({ page, onExitComplete }: TransientPageHostProps) {
  const latestExitSnapshotRef = useRef<TransientPageExitSnapshot>({
    hasActivePage: page !== null,
    onExitComplete,
  });

  useLayoutEffect(() => {
    latestExitSnapshotRef.current = {
      hasActivePage: page !== null,
      onExitComplete,
    };
  }, [page, onExitComplete]);

  const handleExitComplete = useCallback(() => {
    completeTransientPageExit(latestExitSnapshotRef.current);
  }, []);

  return (
    <MotionConfig reducedMotion="user">
      <AnimatePresence
        initial={false}
        mode="wait"
        onExitComplete={handleExitComplete}
      >
        {page ? <TransientPageLayer key={page.instanceKey} page={page} /> : null}
      </AnimatePresence>
    </MotionConfig>
  );
}
