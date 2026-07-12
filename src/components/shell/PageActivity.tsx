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

export function PageActivityProvider({ active, children }: { active: boolean; children: ReactNode }) {
  const value = useMemo<PageActivity>(
    () => ({ interactive: active, refreshEnabled: active }),
    [active],
  );

  return (
    <PageActivityContext.Provider value={value}>
      <InteractionActivityProvider active={active}>
        {children}
      </InteractionActivityProvider>
    </PageActivityContext.Provider>
  );
}

export function usePageActivity() {
  return useContext(PageActivityContext);
}

export function usePageActivation(onActivate: (activation: PageActivation) => void) {
  const { refreshEnabled } = usePageActivity();
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
