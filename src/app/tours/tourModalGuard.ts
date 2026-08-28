/**
 * Driver.js popovers use role="dialog" as part of their accessibility
 * contract. Keep the business-modal guard in the tour boundary so the
 * composition root does not need to know how Driver.js renders its DOM.
 *
 * Shared application modals carry an explicit marker. The role fallback keeps
 * independently implemented dialogs covered while excluding the tour's own
 * popover. Active transient pages are also blocking because they cover the
 * shell and may contain an editor or confirmation flow.
 */
export function hasBlockingBusinessModal(root?: ParentNode): boolean {
  if (!root) {
    if (typeof document === "undefined") return false;
    root = document;
  }

  if (root.querySelector('[data-tour-blocking="true"]')) {
    return true;
  }

  if (root.querySelector('[data-page-transition-kind="transient"][data-page-transition-state="active"]')) {
    return true;
  }

  const dialogs = root.querySelectorAll('[role="dialog"]');
  return Array.from(dialogs).some((dialog) => {
    // The generic class covers Driver.js itself; the application class keeps
    // this correct if a future Driver.js renderer changes its base class.
    return !dialog.closest(".driver-popover, .relay-pool-tour-popover");
  });
}
