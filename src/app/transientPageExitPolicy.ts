export type TransientPageExitSnapshot = Readonly<{
  hasActivePage: boolean;
  onExitComplete?: () => void;
}>;

export function completeTransientPageExit(
  snapshot: TransientPageExitSnapshot,
): void {
  if (!snapshot.hasActivePage) {
    snapshot.onExitComplete?.();
  }
}
