import { useEffect, useMemo, useRef, type ReactNode } from "react";
import {
  createPageVisibility,
  PageVisibilityProvider,
  usePageQueryEnabled,
  usePageVisibility,
  type PageVisibility as CanonicalPageVisibility,
} from "@/app/navigation/PageVisibility";
import { useInteractionActivity } from "@/components/ui/InteractionActivity";

export type PageActivity = {
  interactive: boolean;
  refreshEnabled: boolean;
};

type PageActivation = {
  isInitial: boolean;
};

export function PageActivityProvider({
  active,
  refreshEnabled = active,
  visibility,
  children,
}: {
  active?: boolean;
  refreshEnabled?: boolean;
  visibility?: CanonicalPageVisibility;
  children: ReactNode;
}) {
  const resolvedVisibility = visibility ?? createPageVisibility({
    kind: active && refreshEnabled ? "foreground" : "background",
    interactive: Boolean(active),
    queryEnabled: Boolean(active && refreshEnabled),
    reason: active ? "active" : "inactive",
  });
  return (
    <PageVisibilityProvider visibility={resolvedVisibility}>
      {children}
    </PageVisibilityProvider>
  );
}

export function usePageActivity() {
  const visibility = usePageVisibility();
  return useMemo<PageActivity>(
    () => ({
      interactive: visibility.interactive,
      refreshEnabled: visibility.queryEnabled,
    }),
    [visibility.interactive, visibility.queryEnabled],
  );
}

export function usePageRefreshEnabled() {
  return usePageQueryEnabled();
}

export function usePageActivation(onActivate: (activation: PageActivation) => void) {
  const refreshEnabled = usePageRefreshEnabled();
  const interactive = useInteractionActivity();
  const onActivateRef = useRef(onActivate);
  const wasActiveRef = useRef(false);
  const hasActivatedRef = useRef(false);

  onActivateRef.current = onActivate;

  useEffect(() => {
    const active = interactive && refreshEnabled;
    if (active && !wasActiveRef.current) {
      onActivateRef.current({ isInitial: !hasActivatedRef.current });
      hasActivatedRef.current = true;
    }
    wasActiveRef.current = active;
  }, [interactive, refreshEnabled]);
}
