import { createContext, useContext, useEffect, useMemo, useRef, type ReactNode } from "react";
import {
  InteractionActivityProvider,
  useInteractionActivity,
} from "@/components/ui/InteractionActivity";

export type PageActivity = {
  interactive: boolean;
  refreshEnabled: boolean;
};

type PageActivation = {
  isInitial: boolean;
};

const PageActivityContext = createContext<PageActivity>({
  interactive: true,
  refreshEnabled: true,
});
const PageRefreshContext = createContext(true);

export function PageActivityProvider({
  active,
  refreshEnabled = active,
  children,
}: {
  active: boolean;
  refreshEnabled?: boolean;
  children: ReactNode;
}) {
  const value = useMemo<PageActivity>(
    () => ({ interactive: active, refreshEnabled }),
    [active, refreshEnabled],
  );

  return (
    <PageActivityContext.Provider value={value}>
      <PageRefreshContext.Provider value={value.refreshEnabled}>
        <InteractionActivityProvider active={active}>
          {children}
        </InteractionActivityProvider>
      </PageRefreshContext.Provider>
    </PageActivityContext.Provider>
  );
}

export function usePageActivity() {
  return useContext(PageActivityContext);
}

export function usePageRefreshEnabled() {
  return useContext(PageRefreshContext);
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
