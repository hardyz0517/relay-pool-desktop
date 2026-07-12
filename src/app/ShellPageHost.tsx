import { memo, useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import {
  markNavigation,
  measureNavigation,
  navigationMarks,
} from "@/app/navigationPerformance";
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

export type ShellPageState = "active" | "background" | "entering" | "inactive";

type ShellPageSlotProps = {
  routeId: AppRouteId;
  state: ShellPageState;
  actions: ShellPageActions;
  onEnteringComplete: () => void;
};

const ShellPageSlot = memo(function ShellPageSlot({
  routeId,
  state,
  actions,
  onEnteringComplete,
}: ShellPageSlotProps) {
  const active = state === "active" || state === "entering";
  const inert = !active;

  return (
    <PageActivityProvider active={active}>
      <div
        aria-hidden={inert}
        className="app-page-transition-layer"
        data-page-transition-kind="shell"
        data-page-transition-layer
        data-page-transition-page-id={routeId}
        data-page-transition-state={state}
        inert={inert ? "" : undefined}
        onAnimationEnd={(event) => {
          if (state === "entering" && event.target === event.currentTarget) {
            onEnteringComplete();
          }
        }}
      >
        <div className="app-page-transition-content">
          <ShellPageErrorBoundary>
            <ShellPageContent routeId={routeId} actions={actions} />
          </ShellPageErrorBoundary>
        </div>
      </div>
    </PageActivityProvider>
  );
});

export const ShellPageHost = memo(function ShellPageHost({
  mountedRouteIds,
  activeShellRouteId,
  transientActive,
  returningFromTransient,
  activeTransientPage,
  actions,
  navigationSequence,
  onExitComplete,
  onRememberShellFocusTarget,
  pending,
}: {
  mountedRouteIds: Set<AppRouteId>;
  activeShellRouteId: AppRouteId;
  transientActive: boolean;
  returningFromTransient: boolean;
  activeTransientPage: TransientPageDescriptor | null;
  actions: ShellPageActions;
  navigationSequence: number;
  onExitComplete: () => void;
  onRememberShellFocusTarget: (target: EventTarget | null) => void;
  pending: boolean;
}) {
  const previousShellRouteIdRef = useRef(activeShellRouteId);
  const [visualHandoff, setVisualHandoff] = useState<{
    sequence: number;
    previousRouteId: AppRouteId | null;
  } | null>(null);

  useLayoutEffect(() => {
    markNavigation(navigationMarks.content(navigationSequence));
  }, [navigationSequence]);

  useEffect(() => {
    if (transientActive || previousShellRouteIdRef.current === activeShellRouteId) {
      previousShellRouteIdRef.current = activeShellRouteId;
      return;
    }

    setVisualHandoff({
      sequence: navigationSequence,
      previousRouteId: previousShellRouteIdRef.current,
    });
    previousShellRouteIdRef.current = activeShellRouteId;
  }, [activeShellRouteId, navigationSequence, transientActive]);

  const completeEntering = useCallback(() => {
    setVisualHandoff((current) => {
      if (!current) {
        return current;
      }
      const completeMark = navigationMarks.complete(current.sequence);
      markNavigation(completeMark);
      measureNavigation(
        `navigation:${current.sequence}:handoff`,
        navigationMarks.intent(current.sequence),
        completeMark,
      );
      return null;
    });
  }, []);

  useEffect(() => {
    if (!visualHandoff) {
      return;
    }
    const timeoutId = window.setTimeout(completeEntering, 200);
    return () => window.clearTimeout(timeoutId);
  }, [completeEntering, visualHandoff]);

  const routeIds = mountedRouteIds.has(activeShellRouteId)
    ? [...mountedRouteIds]
    : [...mountedRouteIds, activeShellRouteId];
  if (visualHandoff?.previousRouteId && !routeIds.includes(visualHandoff.previousRouteId)) {
    routeIds.push(visualHandoff.previousRouteId);
  }

  return (
    <div
      className="app-page-transition-stack"
      data-page-transition-handoff={returningFromTransient ? "transient-exit" : "none"}
      data-page-transition-pending={pending ? "true" : "false"}
      onPointerDownCapture={(event) => onRememberShellFocusTarget(event.target)}
      onFocusCapture={(event) => onRememberShellFocusTarget(event.target)}
    >
      {routeIds.map((routeId) => {
        const shellPageState: ShellPageState = (() => {
          if (visualHandoff?.sequence === navigationSequence && !transientActive) {
            if (routeId === activeShellRouteId) {
              return "entering";
            }
            if (routeId === visualHandoff.previousRouteId) {
              return "background";
            }
            return "inactive";
          }
          if (routeId !== activeShellRouteId) {
            return "inactive";
          }
          return transientActive ? "background" : "active";
        })();

        return (
          <ShellPageSlot
            key={routeId}
            actions={actions}
            onEnteringComplete={completeEntering}
            routeId={routeId}
            state={shellPageState === "entering" ? "entering" : shellPageState}
          />
        );
      })}

      <TransientPageHost
        page={activeTransientPage}
        onExitComplete={onExitComplete}
      />
    </div>
  );
});
