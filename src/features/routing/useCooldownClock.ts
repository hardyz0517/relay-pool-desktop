import { useEffect, useRef, useState } from "react";

export type CooldownDeadline = { id: string; untilMs: number };

export function useCooldownClock({
  active,
  deadlines,
  onExpired,
}: {
  active: boolean;
  deadlines: CooldownDeadline[];
  onExpired: (ids: string[]) => void;
}) {
  const [nowMs, setNowMs] = useState(() => Date.now());
  const notifiedDeadlinesRef = useRef(new Set<string>());

  useEffect(() => {
    const currentKeys = new Set(deadlines.map(({ id, untilMs }) => `${id}:${untilMs}`));
    for (const key of notifiedDeadlinesRef.current) {
      if (!currentKeys.has(key)) notifiedDeadlinesRef.current.delete(key);
    }
  }, [deadlines]);

  useEffect(() => {
    if (!active) return;
    const tick = () => {
      const nextNowMs = Date.now();
      setNowMs(nextNowMs);
      const expiredIds: string[] = [];
      for (const { id, untilMs } of deadlines) {
        const deadlineKey = `${id}:${untilMs}`;
        if (untilMs <= nextNowMs && !notifiedDeadlinesRef.current.has(deadlineKey)) {
          notifiedDeadlinesRef.current.add(deadlineKey);
          expiredIds.push(id);
        }
      }
      if (expiredIds.length > 0) onExpired(expiredIds);
    };

    tick();
    const intervalId = window.setInterval(tick, 1_000);
    return () => window.clearInterval(intervalId);
  }, [active, deadlines, onExpired]);

  return nowMs;
}
