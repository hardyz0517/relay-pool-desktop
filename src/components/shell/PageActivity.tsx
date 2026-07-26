import { useMemo, type ReactNode } from "react";
import {
  createPageVisibility,
  PageVisibilityProvider,
  usePageQueryEnabled,
  usePageVisibility,
  type PageVisibility as CanonicalPageVisibility,
} from "@/app/navigation/PageVisibility";

export type PageActivity = {
  interactive: boolean;
  refreshEnabled: boolean;
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
