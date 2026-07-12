import { memo } from "react";
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

export type ShellPageState = "active" | "background" | "inactive";

type ShellPageSlotProps = {
  routeId: AppRouteId;
  state: ShellPageState;
  actions: ShellPageActions;
};

const ShellPageSlot = memo(function ShellPageSlot({
  routeId,
  state,
  actions,
}: ShellPageSlotProps) {
  const active = state === "active";
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
  onExitComplete,
  onRememberShellFocusTarget,
}: {
  mountedRouteIds: Set<AppRouteId>;
  activeShellRouteId: AppRouteId;
  transientActive: boolean;
  returningFromTransient: boolean;
  activeTransientPage: TransientPageDescriptor | null;
  actions: ShellPageActions;
  onExitComplete: () => void;
  onRememberShellFocusTarget: (target: EventTarget | null) => void;
}) {
  const routeIds = mountedRouteIds.has(activeShellRouteId)
    ? [...mountedRouteIds]
    : [...mountedRouteIds, activeShellRouteId];

  return (
    <div
      className="app-page-transition-stack"
      data-page-transition-handoff={returningFromTransient ? "transient-exit" : "none"}
      onPointerDownCapture={(event) => onRememberShellFocusTarget(event.target)}
      onFocusCapture={(event) => onRememberShellFocusTarget(event.target)}
    >
      {routeIds.map((routeId) => {
        const shellPageState: ShellPageState =
          routeId !== activeShellRouteId
            ? "inactive"
            : transientActive ? "background" : "active";

        return (
          <ShellPageSlot
            key={routeId}
            actions={actions}
            routeId={routeId}
            state={shellPageState}
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
