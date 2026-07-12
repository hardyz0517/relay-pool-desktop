export function completeTransientPageExit(
  page: unknown,
  onExitComplete?: () => void,
): void {
  if (page === null) {
    onExitComplete?.();
  }
}
