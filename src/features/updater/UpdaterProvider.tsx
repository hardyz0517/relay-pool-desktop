import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useReducer,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { UpdateDialog } from "./UpdateDialog";
import { initialUpdaterState, reduceUpdaterState, type UpdaterState } from "./updateState";
import {
  checkForAppUpdate,
  cleanupBeforeUpdate,
  closePendingUpdate,
  currentAppVersion,
  downloadPendingUpdate,
  installPendingUpdateAndRelaunch,
} from "@/lib/api/updater";
import { normalizeUpdaterError } from "@/lib/api/updaterErrors";
import { readError } from "@/lib/errors";
import { useToast } from "@/components/ui";

type UpdaterContextValue = {
  state: UpdaterState;
  checkNow: () => Promise<void>;
};

const UpdaterContext = createContext<UpdaterContextValue | null>(null);

export function UpdaterProvider({ children }: { children: ReactNode }) {
  const toast = useToast();
  const [state, dispatch] = useReducer(reduceUpdaterState, initialUpdaterState);
  const [dialogOpen, setDialogOpen] = useState(false);
  const checkingRef = useRef(false);

  const checkNow = useCallback(async () => {
    if (checkingRef.current) return;
    checkingRef.current = true;
    dispatch({ type: "CHECK_STARTED" });
    try {
      const result = await checkForAppUpdate();
      if (result.kind === "available") {
        dispatch({
          type: "UPDATE_AVAILABLE",
          currentVersion: result.update.currentVersion,
          version: result.update.version,
          notes: result.update.notes,
        });
        toast.info(`发现新版本 ${result.update.version}`);
        setDialogOpen(true);
      } else {
        dispatch({
          type: "UP_TO_DATE",
          currentVersion: result.currentVersion,
          checkedAt: new Date().toISOString(),
        });
        toast.success("已是最新");
        setDialogOpen(false);
      }
    } catch (error) {
      const message = normalizeUpdaterError(error);
      dispatch({ type: "FAILED", message });
      toast.error("检查更新未完成", message);
    } finally {
      checkingRef.current = false;
    }
  }, [toast]);

  useEffect(() => {
    void currentAppVersion()
      .then((version) => dispatch({ type: "CURRENT_VERSION", version }))
      .catch(() => undefined);
    const timer = window.setTimeout(() => void checkNow(), 5_000);
    return () => window.clearTimeout(timer);
  }, [checkNow]);

  const dismiss = useCallback(async () => {
    if (state.phase === "downloading" || state.phase === "cleaning" || state.phase === "installing") {
      return;
    }
    await closePendingUpdate();
    dispatch({ type: "DISMISSED" });
    setDialogOpen(false);
  }, [state.phase]);

  const install = useCallback(async () => {
    dispatch({ type: "DOWNLOAD_STARTED" });
    try {
      await downloadPendingUpdate((progress) => {
        dispatch({ type: "DOWNLOAD_PROGRESS", ...progress });
      });
      dispatch({ type: "CLEANUP_STARTED" });
      await cleanupBeforeUpdate();
      dispatch({ type: "INSTALL_STARTED" });
      await installPendingUpdateAndRelaunch();
    } catch (error) {
      dispatch({ type: "FAILED", message: readError(error) });
    }
  }, []);

  const value = useMemo(() => ({ state, checkNow }), [checkNow, state]);

  return (
    <UpdaterContext.Provider value={value}>
      {children}
      <UpdateDialog
        open={dialogOpen}
        state={state}
        onDismiss={() => void dismiss()}
        onInstall={() => void install()}
        onRetry={() => void checkNow()}
      />
    </UpdaterContext.Provider>
  );
}

export function useUpdater() {
  const context = useContext(UpdaterContext);
  if (!context) throw new Error("useUpdater must be used within UpdaterProvider");
  return context;
}
