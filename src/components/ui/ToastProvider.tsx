import {
  createContext,
  useCallback,
  useContext,
  useMemo,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { AlertCircle, CheckCircle2, Info, Loader2, X } from "lucide-react";
import { cn } from "@/lib/utils";

type ToastIntent = "success" | "error" | "info" | "loading";

type ToastInput = {
  title: string;
  description?: string;
  intent?: ToastIntent;
  durationMs?: number;
};

type ToastItem = Required<Pick<ToastInput, "title" | "intent">> & {
  id: string;
  description?: string;
};

type ToastApi = {
  show: (toast: ToastInput) => string;
  success: (title: string, description?: string) => string;
  error: (title: string, description?: string) => string;
  info: (title: string, description?: string) => string;
  loading: (title: string, description?: string) => string;
  dismiss: (id: string) => void;
};

const ToastContext = createContext<ToastApi | null>(null);

export function ToastProvider({ children }: { children: ReactNode }) {
  const [toasts, setToasts] = useState<ToastItem[]>([]);
  const timers = useRef(new Map<string, number>());

  const dismiss = useCallback((id: string) => {
    const timer = timers.current.get(id);
    if (timer) {
      window.clearTimeout(timer);
      timers.current.delete(id);
    }
    setToasts((current) => current.filter((toast) => toast.id !== id));
  }, []);

  const show = useCallback((toast: ToastInput) => {
    const intent = toast.intent ?? "info";
    const id = `${Date.now()}-${Math.random().toString(16).slice(2)}`;
    const durationMs = toast.durationMs ?? defaultDuration(intent);
    setToasts((current) => [
      { id, title: toast.title, description: toast.description, intent },
      ...current.slice(0, 3),
    ]);

    if (durationMs > 0) {
      const timer = window.setTimeout(() => dismiss(id), durationMs);
      timers.current.set(id, timer);
    }
    return id;
  }, [dismiss]);

  const api = useMemo<ToastApi>(() => ({
    show,
    success: (title, description) => show({ title, description, intent: "success" }),
    error: (title, description) => show({ title, description, intent: "error" }),
    info: (title, description) => show({ title, description, intent: "info" }),
    loading: (title, description) => show({ title, description, intent: "loading", durationMs: 0 }),
    dismiss,
  }), [dismiss, show]);

  return (
    <ToastContext.Provider value={api}>
      {children}
      <div
        aria-live="polite"
        aria-relevant="additions text"
        className="pointer-events-none fixed top-4 left-1/2 z-[90] grid w-[min(360px,calc(100vw-32px))] -translate-x-1/2 gap-2"
      >
        {toasts.map((toast) => (
          <ToastCard key={toast.id} toast={toast} onDismiss={() => dismiss(toast.id)} />
        ))}
      </div>
    </ToastContext.Provider>
  );
}

export function useToast() {
  const api = useContext(ToastContext);
  if (!api) {
    throw new Error("useToast must be used within ToastProvider");
  }
  return api;
}

function ToastCard({ toast, onDismiss }: { toast: ToastItem; onDismiss: () => void }) {
  const Icon = toast.intent === "success"
    ? CheckCircle2
    : toast.intent === "error"
      ? AlertCircle
      : toast.intent === "loading"
        ? Loader2
        : Info;

  return (
    <div
      role={toast.intent === "error" ? "alert" : "status"}
      className={cn(
        "pointer-events-auto grid grid-cols-[auto_minmax(0,1fr)_auto] items-start gap-3 rounded-[var(--surface-radius)] border bg-popover px-3 py-3 text-sm shadow-popover motion-safe:animate-[toastIn_150ms_ease-out]",
        toast.intent === "success" && "border-success-border",
        toast.intent === "error" && "border-danger-border",
        toast.intent === "info" && "border-border",
        toast.intent === "loading" && "border-info-border",
      )}
    >
      <span
        className={cn(
          "mt-0.5 flex h-5 w-5 items-center justify-center",
          toast.intent === "success" && "text-success-foreground",
          toast.intent === "error" && "text-danger-foreground",
          toast.intent === "info" && "text-muted-foreground",
          toast.intent === "loading" && "text-info-foreground",
        )}
      >
        <Icon className={cn("h-4 w-4", toast.intent === "loading" && "animate-spin")} />
      </span>
      <div className="min-w-0">
        <div className="font-medium text-foreground">{toast.title}</div>
        {toast.description ? (
          <div className="mt-0.5 line-clamp-3 text-xs leading-5 text-muted-foreground">
            {toast.description}
          </div>
        ) : null}
      </div>
      <button
        type="button"
        aria-label="关闭提示"
        className="cursor-pointer rounded-md p-1 text-muted-foreground transition-colors hover:bg-hover hover:text-foreground focus:outline-none focus:ring-2 focus:ring-ring/30"
        onClick={onDismiss}
      >
        <X className="h-3.5 w-3.5" />
      </button>
    </div>
  );
}

function defaultDuration(intent: ToastIntent) {
  if (intent === "success") return 2200;
  if (intent === "error") return 5000;
  if (intent === "loading") return 0;
  return 2800;
}
