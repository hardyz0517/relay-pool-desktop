import { createContext, useContext, type ReactNode } from "react";

const InteractionActivityContext = createContext(true);

export function InteractionActivityProvider({
  active,
  children,
}: {
  active: boolean;
  children: ReactNode;
}) {
  return (
    <InteractionActivityContext.Provider value={active}>
      {children}
    </InteractionActivityContext.Provider>
  );
}

export function useInteractionActivity() {
  return useContext(InteractionActivityContext);
}
