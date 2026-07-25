import { createContext, useContext, useMemo, type ReactNode } from "react";
import { InteractionActivityProvider } from "@/components/ui/InteractionActivity";

export type PageVisibilityKind = "foreground" | "background";

export type PageVisibilityReason =
  | "active"
  | "entering"
  | "transient-active"
  | "transient-exiting"
  | "covered-by-transient"
  | "leaving"
  | "inactive";

export type PageVisibility = {
  kind: PageVisibilityKind;
  interactive: boolean;
  queryEnabled: boolean;
  reason: PageVisibilityReason;
};

const defaultVisibility = Object.freeze<PageVisibility>({
  kind: "foreground",
  interactive: true,
  queryEnabled: true,
  reason: "active",
});

const PageVisibilityContext = createContext<PageVisibility>(defaultVisibility);

export function createPageVisibility({
  kind,
  reason,
  interactive = kind === "foreground",
  queryEnabled = kind === "foreground",
}: {
  kind: PageVisibilityKind;
  reason: PageVisibilityReason;
  interactive?: boolean;
  queryEnabled?: boolean;
}): PageVisibility {
  return { kind, interactive, queryEnabled, reason };
}

export function shellPageVisibilityForState(
  state: "active" | "background" | "entering" | "leaving" | "inactive",
): PageVisibility {
  if (state === "active" || state === "entering") {
    return createPageVisibility({ kind: "foreground", reason: state });
  }
  return createPageVisibility({
    kind: "background",
    reason: state === "background" ? "covered-by-transient" : state,
  });
}

export function transientPageVisibility(isPresent: boolean): PageVisibility {
  return createPageVisibility({
    kind: isPresent ? "foreground" : "background",
    reason: isPresent ? "transient-active" : "transient-exiting",
  });
}

export function PageVisibilityProvider({
  visibility,
  children,
}: {
  visibility: PageVisibility;
  children: ReactNode;
}) {
  const value = useMemo(
    () => visibility,
    [visibility.kind, visibility.interactive, visibility.queryEnabled, visibility.reason],
  );

  return (
    <PageVisibilityContext.Provider value={value}>
      <InteractionActivityProvider active={value.interactive}>
        {children}
      </InteractionActivityProvider>
    </PageVisibilityContext.Provider>
  );
}

export function usePageVisibility() {
  return useContext(PageVisibilityContext);
}

export function usePageQueryEnabled() {
  return usePageVisibility().queryEnabled;
}
