import { AnimatePresence, motion, MotionConfig, useIsPresent } from "framer-motion";
import type { ReactNode } from "react";
import { PageActivityProvider } from "@/components/shell/PageActivity";
import type { AppPageId } from "@/lib/types/navigation";

declare module "react" {
  interface HTMLAttributes<T> {
    inert?: "" | undefined;
  }
}

export type TransientPageDescriptor = {
  pageId: AppPageId;
  instanceKey: string;
  node: ReactNode;
};

type TransientPageHostProps = {
  page: TransientPageDescriptor | null;
};

const transientPageTransition = {
  duration: 0.2,
};

function TransientPageLayer({ page }: { page: TransientPageDescriptor }) {
  const isPresent = useIsPresent();

  return (
    <motion.div
      className="app-page-transition-layer app-page-transition-overlay"
      data-page-transition-layer
      data-page-transition-kind="transient"
      data-page-transition-page-id={page.pageId}
      data-page-transition-state={isPresent ? "active" : "exiting"}
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      transition={transientPageTransition}
    >
      <PageActivityProvider active={isPresent}>
        <div
          aria-hidden={!isPresent}
          className="app-page-transition-content"
          inert={isPresent ? undefined : ""}
        >
          {page.node}
        </div>
      </PageActivityProvider>
    </motion.div>
  );
}

export function TransientPageHost({ page }: TransientPageHostProps) {
  return (
    <MotionConfig reducedMotion="user">
      <AnimatePresence initial={false} mode="wait">
        {page ? <TransientPageLayer key={page.instanceKey} page={page} /> : null}
      </AnimatePresence>
    </MotionConfig>
  );
}
