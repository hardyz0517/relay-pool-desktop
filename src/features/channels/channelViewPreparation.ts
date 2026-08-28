export type ChannelViewPreparationPort = {
  /** Switches to the local status view and returns an idempotent restore callback. */
  showLocalView(): () => void;
  /** Switches to the official status view and returns an idempotent restore callback. */
  showOfficialView(): () => void;
  /** Switches to the monitoring view and returns an idempotent restore callback. */
  showMonitoringView(): () => void;
};
