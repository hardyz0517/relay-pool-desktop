import { createContext, useContext, useEffect, useRef, type ReactNode } from "react";

type PageActivation = {
  isInitial: boolean;
};

const PageActivityContext = createContext(true);

export function PageActivityProvider({ active, children }: { active: boolean; children: ReactNode }) {
  return <PageActivityContext.Provider value={active}>{children}</PageActivityContext.Provider>;
}

export function usePageActivation(onActivate: (activation: PageActivation) => void) {
  const active = useContext(PageActivityContext);
  const onActivateRef = useRef(onActivate);
  const wasActiveRef = useRef(false);
  const hasActivatedRef = useRef(false);

  onActivateRef.current = onActivate;

  useEffect(() => {
    if (active && !wasActiveRef.current) {
      onActivateRef.current({ isInitial: !hasActivatedRef.current });
      hasActivatedRef.current = true;
    }
    wasActiveRef.current = active;
  }, [active]);
}
