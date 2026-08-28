import { motion, MotionConfig, type TargetAndTransition } from "framer-motion";
import { memo, useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import {
  markNavigation,
  measureNavigation,
  navigationMarks,
} from "@/app/navigationPerformance";
import { isLatestShellNavigationCompletion } from "@/app/navigationPolicy";
import {
  PageVisibilityProvider,
  shellPageVisibilityForState,
} from "@/app/navigation/PageVisibility";
import { getPageRetentionDecision } from "@/app/navigation/pageRetentionPolicy";
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
  actions: ShellPageActions;
  navigationSequence: number;
  onEnteringComplete: (routeId: AppRouteId, sequence: number) => void;
};

const ShellPageSlot = memo(function ShellPageSlot({
  routeId,
  state,
  actions,
  navigationSequence,
  onEnteringComplete,
}: ShellPageSlotProps) {
  const visibility = shellPageVisibilityForState(state);
  const inert = !visibility.interactive;

  return (
    <PageVisibilityProvider visibility={visibility}>
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
    </PageVisibilityProvider>
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
  onPageReady,
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
  onPageReady?: (routeId: AppRouteId, sequence: number) => void;
  onRememberShellFocusTarget: (target: EventTarget | null) => void;
  pending: boolean;
}) {
  const [completedNavigationSequence, setCompletedNavigationSequence] = useState(0);
  const reportedNavigationSequenceRef = useRef(0);
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
    setCompletedNavigationSequence((current) => {
      if (current >= sequence) {
        return current;
      }
      const completeMark = navigationMarks.complete(sequence);
      markNavigation(completeMark);
      measureNavigation(
        `navigation:${sequence}:handoff`,
        navigationMarks.intent(sequence),
        completeMark,
      );
      return sequence;
    });
    if (reportedNavigationSequenceRef.current < sequence) {
      reportedNavigationSequenceRef.current = sequence;
      onPageReady?.(routeId, sequence);
    }
  }, [committedNavigationSequence, intentNavigationSequence, intentShellRouteId, onPageReady]);

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
  const retainedRouteIds = routeIds.filter((routeId) =>
    getPageRetentionDecision({
      routeId,
      activeRouteId: activeShellRouteId,
      previousRouteId: previousShellRouteId,
    }).retain,
  );

  return (
    <div
      className="app-page-transition-stack"
      data-page-transition-handoff={handoffActive ? "shell" : "none"}
      data-page-transition-pending={pending ? "true" : "false"}
      onPointerDownCapture={(event) => onRememberShellFocusTarget(event.target)}
      onFocusCapture={(event) => onRememberShellFocusTarget(event.target)}
    >
      <MotionConfig reducedMotion="user">
        {retainedRouteIds.map((routeId) => {
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
          return (
            <ShellPageSlot
              key={routeId}
              actions={actions}
              navigationSequence={committedNavigationSequence}
              onEnteringComplete={completeEntering}
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
