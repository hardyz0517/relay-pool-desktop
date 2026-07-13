import { useCallback, useRef, useState, useTransition } from "react";
import {
  resolveActiveShellRouteId,
  resolveTransientParentRouteId,
} from "@/app/pageTransitionPolicy";
import { markNavigation, navigationMarks } from "@/app/navigationPerformance";
import {
  commitNavigationIntent,
  createInitialNavigationIntent,
  createNavigationIntent,
  shouldNavigateToRoute,
  type CommittedNavigation,
} from "@/app/navigationPolicy";
import type { AppPageId, AppRouteId } from "@/lib/types/navigation";

export function useNavigationController(initialRouteId: AppRouteId) {
  const initialIntent = createInitialNavigationIntent(initialRouteId);
  const [intent, setIntent] = useState(initialIntent);
  const [committed, setCommitted] = useState<CommittedNavigation>({
    activeRouteId: initialRouteId,
    previousRouteId: null,
    transientParentRouteId: null,
    sequence: 0,
  });
  const [pending, startTransition] = useTransition();
  const intentRef = useRef(initialIntent);
  const sequenceRef = useRef(0);

  const navigate = useCallback((routeId: AppPageId) => {
    if (!shouldNavigateToRoute(intentRef.current, routeId)) {
      return;
    }
    const sequence = sequenceRef.current + 1;
    sequenceRef.current = sequence;
    const transientParentRouteId = resolveTransientParentRouteId(
      intentRef.current.routeId,
      routeId,
      intentRef.current.transientParentRouteId,
    );
    const nextIntent = createNavigationIntent(
      routeId,
      resolveActiveShellRouteId(routeId, transientParentRouteId),
      transientParentRouteId,
      sequence,
    );
    intentRef.current = nextIntent;
    markNavigation(navigationMarks.intent(sequence));
    setIntent(nextIntent);
    startTransition(() => {
      setCommitted((current) =>
        commitNavigationIntent(current, nextIntent, sequenceRef.current),
      );
    });
  }, []);

  return { intent, committed, pending, navigate };
}
