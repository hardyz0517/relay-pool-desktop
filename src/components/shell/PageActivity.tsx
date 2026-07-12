import { useEffect, useRef, type ReactNode } from "react";
import {
  InteractionActivityProvider,
  useInteractionActivity,
} from "@/components/ui/InteractionActivity";

type PageActivation = {
  isInitial: boolean;
};

export function PageActivityProvider({ active, children }: { active: boolean; children: ReactNode }) {
  return (
    <InteractionActivityProvider active={active}>
      {children}
    </InteractionActivityProvider>
  );
}

export function usePageActivation(onActivate: (activation: PageActivation) => void) {
  const active = useInteractionActivity();
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
