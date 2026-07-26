import { useCallback, useEffect, useRef, useState } from "react";
import {
  runStationKeyConnectivityOperation,
  type ConnectivityOperationInput,
  type ConnectivityOperationRunOptions,
} from "./connectivityOperationController";

export function useConnectivityOperation() {
  const abortRef = useRef<AbortController | null>(null);
  const [operationId, setOperationId] = useState<string | null>(null);
  const [running, setRunning] = useState(false);

  const cancel = useCallback(() => {
    abortRef.current?.abort();
  }, []);

  const run = useCallback(
    async (
      input: ConnectivityOperationInput,
      options: Omit<ConnectivityOperationRunOptions, "signal" | "onOperationId"> = {},
    ) => {
      abortRef.current?.abort();
      const abortController = new AbortController();
      abortRef.current = abortController;
      setOperationId(null);
      setRunning(true);
      try {
        return await runStationKeyConnectivityOperation(input, {
          ...options,
          signal: abortController.signal,
          onOperationId: (nextOperationId) => {
            setOperationId(nextOperationId);
          },
        });
      } finally {
        if (abortRef.current === abortController) {
          abortRef.current = null;
          setOperationId(null);
          setRunning(false);
        }
      }
    },
    [],
  );

  useEffect(() => cancel, [cancel]);

  return { operationId, running, run, cancel };
}
