import { motion, MotionConfig, type TargetAndTransition } from "framer-motion";
import { memo, useCallback, useEffect, useLayoutEffect, useState } from "react";
import {
  markNavigation,
  measureNavigation,
  navigationMarks,
} from "@/app/navigationPerformance";
import { isLatestShellNavigationCompletion } from "@/app/navigationPolicy";
import { PageActivityProvider } from "@/components/shell/PageActivity";
import {
  TransientPageHost,
  type TransientPageDescriptor,
} from "@/app/TransientPageHost";
import { ShellPageErrorBoundary } from "@/app/ShellPageErrorBoundary";
import {
  ShellPageContent,
  type ShellPageActions,
} from "@/app/shellPageRegistry";
import type { AppRouteId } from "@/lib/types/navigation";

export type ShellPageState =
  | "active"
  | "background"
  | "entering"
  | "leaving"
  | "inactive";

const shellPageMotionTargets = {
  active: { opacity: 1, y: 0, transition: { duration: 0 } },
  background: { opacity: 1, y: 0, transition: { duration: 0 } },
  entering: {
    opacity: [0, 1],
    y: [4, 0],
    transition: { duration: 0.16, ease: "easeOut" },
  },
  leaving: { opacity: 1, transition: { duration: 0 } },
  inactive: { opacity: 1, y: 0, transition: { duration: 0 } },
} satisfies Record<ShellPageState, TargetAndTransition>;

type ShellPageSlotProps = {
  routeId: AppRouteId;
  state: ShellPageState;
  refreshEnabled: boolean;
  actions: ShellPageActions;
  navigationSequence: number;
  onEnteringComplete: (routeId: AppRouteId, sequence: number) => void;
};

const ShellPageSlot = memo(function ShellPageSlot({
  routeId,
  state,
  refreshEnabled,
  actions,
  navigationSequence,
  onEnteringComplete,
}: ShellPageSlotProps) {
  const interactive = state === "active" || state === "entering";
  const inert = !interactive;

  return (
    <PageActivityProvider active={interactive} refreshEnabled={refreshEnabled}>
      <div
        aria-hidden={inert}
        className="app-page-transition-layer"
        data-page-transition-kind="shell"
        data-page-transition-layer
        data-page-transition-page-id={routeId}
        data-page-transition-state={state}
        inert={inert ? "" : undefined}
      >
        <motion.div
          animate={shellPageMotionTargets[state]}
          className="app-page-transition-content"
          initial={state === "entering" ? { opacity: 0 } : false}
          onAnimationComplete={() => {
            if (state === "entering") {
              onEnteringComplete(routeId, navigationSequence);
            }
          }}
        >
          <ShellPageErrorBoundary>
            <ShellPageContent routeId={routeId} actions={actions} />
          </ShellPageErrorBoundary>
        </motion.div>
      </div>
    </PageActivityProvider>
  );
});

export const ShellPageHost = memo(function ShellPageHost({
  mountedRouteIds,
  activeShellRouteId,
  previousShellRouteId,
  intentShellRouteId,
  intentNavigationSequence,
  committedNavigationSequence,
  transientActive,
  activeTransientPage,
  actions,
  onExitComplete,
  onRememberShellFocusTarget,
  pending,
}: {
  mountedRouteIds: Set<AppRouteId>;
  activeShellRouteId: AppRouteId;
  previousShellRouteId: AppRouteId | null;
  intentShellRouteId: AppRouteId;
  intentNavigationSequence: number;
  committedNavigationSequence: number;
  transientActive: boolean;
  activeTransientPage: TransientPageDescriptor | null;
  actions: ShellPageActions;
  onExitComplete: () => void;
  onRememberShellFocusTarget: (target: EventTarget | null) => void;
  pending: boolean;
}) {
  const [completedNavigation, setCompletedNavigation] = useState(() => ({
    sequence: 0,
    refreshRouteId: activeShellRouteId,
  }));
  const {
    sequence: completedNavigationSequence,
    refreshRouteId,
  } = completedNavigation;
  const handoffActive =
    !transientActive &&
    previousShellRouteId !== null &&
    previousShellRouteId !== activeShellRouteId &&
    committedNavigationSequence > completedNavigationSequence;

  useLayoutEffect(() => {
    markNavigation(navigationMarks.content(committedNavigationSequence));
  }, [committedNavigationSequence]);

  const completeEntering = useCallback((routeId: AppRouteId, sequence: number) => {
    if (
      !isLatestShellNavigationCompletion(
        routeId,
        sequence,
        { shellRouteId: intentShellRouteId, sequence: intentNavigationSequence },
        { sequence: committedNavigationSequence },
      )
    ) {
      return;
    }
    setCompletedNavigation((current) => {
      if (current.sequence >= sequence) {
        return current;
      }
      const completeMark = navigationMarks.complete(sequence);
      markNavigation(completeMark);
      measureNavigation(
        `navigation:${sequence}:handoff`,
        navigationMarks.intent(sequence),
        completeMark,
      );
      return { sequence, refreshRouteId: routeId };
    });
  }, [committedNavigationSequence, intentNavigationSequence, intentShellRouteId]);

  useEffect(() => {
    if (!handoffActive) {
      return;
    }
    const timeoutId = window.setTimeout(
      () => completeEntering(activeShellRouteId, committedNavigationSequence),
      240,
    );
    return () => window.clearTimeout(timeoutId);
  }, [activeShellRouteId, committedNavigationSequence, completeEntering, handoffActive]);

  const routeIds = mountedRouteIds.has(activeShellRouteId)
    ? [...mountedRouteIds]
    : [...mountedRouteIds, activeShellRouteId];
  if (previousShellRouteId && !routeIds.includes(previousShellRouteId)) {
    routeIds.push(previousShellRouteId);
  }

  return (
    <div
      className="app-page-transition-stack"
      data-page-transition-handoff={handoffActive ? "shell" : "none"}
      data-page-transition-pending={pending ? "true" : "false"}
      onPointerDownCapture={(event) => onRememberShellFocusTarget(event.target)}
      onFocusCapture={(event) => onRememberShellFocusTarget(event.target)}
    >
      <MotionConfig reducedMotion="user">
        {routeIds.map((routeId) => {
          const shellPageState: ShellPageState = (() => {
            if (handoffActive) {
              if (routeId === activeShellRouteId) {
                return "entering";
              }
              if (routeId === previousShellRouteId) {
                return "leaving";
              }
              return "inactive";
            }
            if (routeId !== activeShellRouteId) {
              return "inactive";
            }
            if (transientActive) {
              return "background";
            }
            if (intentShellRouteId !== activeShellRouteId) {
              return "leaving";
            }
            return "active";
          })();
          const refreshEnabled =
            routeId === refreshRouteId &&
            (shellPageState === "active" || shellPageState === "leaving");

          return (
            <ShellPageSlot
              key={routeId}
              actions={actions}
              navigationSequence={committedNavigationSequence}
              onEnteringComplete={completeEntering}
              refreshEnabled={refreshEnabled}
              routeId={routeId}
              state={shellPageState}
            />
          );
        })}

        <TransientPageHost page={activeTransientPage} onExitComplete={onExitComplete} />
      </MotionConfig>
    </div>
  );
});
