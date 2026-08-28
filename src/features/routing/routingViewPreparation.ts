export type RoutingViewPreparationPort = {
  /** Switches to the status view and returns an idempotent restore callback. */
  showStatusView(): () => void;
  /** Switches to the settings view and returns an idempotent restore callback. */
  showSettingsView(): () => void;
};
