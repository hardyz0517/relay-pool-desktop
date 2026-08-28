export type TourAutoStartOptions = {
  canAttempt: () => boolean;
  hasTarget: () => boolean;
  start: () => boolean;
  onAccepted: () => void;
  retryMs?: number;
  maxWaitMs?: number;
};

/** Wait for the first anchor without making React own another tour state machine. */
export function scheduleTourAutoStart({
  canAttempt,
  hasTarget,
  start,
  onAccepted,
  retryMs = 300,
  maxWaitMs = 15_000,
}: TourAutoStartOptions): () => void {
  const deadline = Date.now() + maxWaitMs;
  let stopped = false;
  let timerId: number | null = null;
  let observer: MutationObserver | null = null;

  const stop = () => {
    if (stopped) return;
    stopped = true;
    observer?.disconnect();
    observer = null;
    document.removeEventListener("visibilitychange", attempt);
    if (timerId !== null) window.clearTimeout(timerId);
    timerId = null;
  };

  const scheduleRetry = () => {
    if (stopped || timerId !== null || Date.now() >= deadline) return;
    timerId = window.setTimeout(() => {
      timerId = null;
      attempt();
    }, retryMs);
  };

  function attempt() {
    if (stopped) return;
    if (Date.now() >= deadline) return stop();
    if (!canAttempt() || !hasTarget() || !start()) return scheduleRetry();
    onAccepted();
    stop();
  }

  observer = new MutationObserver(attempt);
  observer.observe(document.body, { subtree: true, childList: true, attributes: true });
  document.addEventListener("visibilitychange", attempt);
  const frameId = window.requestAnimationFrame(attempt);
  return () => {
    window.cancelAnimationFrame(frameId);
    stop();
  };
}
