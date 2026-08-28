/**
 * A deliberately small registry for tour view preparation.
 *
 * Preparation actions may reveal an already existing view (for example, select
 * a tab) but must not mutate business data. Keeping the registry separate from
 * the catalog prevents serialised tour definitions from carrying closures.
 */
import type {
  TourPreparationCleanup,
  TourPreparationContext,
  TourPreparationKey,
  TourPreparationRegistry as TourPreparationRegistryPort,
} from "./tourTypes";

export type TourPreparationAction = (
  context: TourPreparationContext,
  signal: AbortSignal,
) => void | TourPreparationCleanup | Promise<void | TourPreparationCleanup>;

type TourPreparationErrorOptions = { cause?: unknown };

export class TourPreparationError extends Error {
  readonly code: "unregistered" | "aborted" | "failed";
  readonly cause?: unknown;

  constructor(code: "unregistered" | "aborted" | "failed", message: string, options?: TourPreparationErrorOptions) {
    super(message);
    this.name = "TourPreparationError";
    this.code = code;
    this.cause = options?.cause;
  }
}

export type TourPreparationRegistryOptions = {
  actions?: ReadonlyMap<TourPreparationKey, TourPreparationAction>;
};

export class TourPreparationRegistry implements TourPreparationRegistryPort {
  private readonly actions = new Map<TourPreparationKey, TourPreparationAction>();

  constructor(options: TourPreparationRegistryOptions = {}) {
    // `none` is reserved by the orchestration layer; view-specific keys are
    // supplied by the application composition root.
    this.actions.set("none", () => undefined);
    options.actions?.forEach((action, key) => this.register(key, action));
  }

  has(key: TourPreparationKey): boolean {
    return this.actions.has(key);
  }

  register(key: TourPreparationKey, action: TourPreparationAction): void {
    if (!key.trim()) throw new TypeError("A tour preparation key is required");
    if (key === "none") throw new TypeError('The reserved "none" preparation cannot be replaced');
    if (this.actions.has(key)) throw new Error(`Tour preparation key already registered: ${key}`);
    this.actions.set(key, action);
  }

  async run(
    key: TourPreparationKey,
    context: TourPreparationContext,
    signal: AbortSignal,
  ): Promise<TourPreparationCleanup | null> {
    if (signal.aborted) throw new TourPreparationError("aborted", "Tour preparation was cancelled");
    const action = this.actions.get(key);
    if (!action) throw new TourPreparationError("unregistered", `Tour preparation is not registered: ${key}`);

    try {
      const result = await action(context, signal);
      const cleanup = typeof result === "function" ? once(result) : null;
      if (signal.aborted) {
        safelyCleanup(cleanup);
        throw new TourPreparationError("aborted", "Tour preparation was cancelled");
      }
      return cleanup;
    } catch (error) {
      if (error instanceof TourPreparationError) throw error;
      if (signal.aborted) throw new TourPreparationError("aborted", "Tour preparation was cancelled", { cause: error });
      throw new TourPreparationError("failed", `Tour preparation failed: ${key}`, { cause: error });
    }
  }
}

function once(cleanup: TourPreparationCleanup): TourPreparationCleanup {
  let called = false;
  return () => {
    if (called) return;
    called = true;
    cleanup();
  };
}

function safelyCleanup(cleanup: TourPreparationCleanup | null): void {
  try {
    cleanup?.();
  } catch {
    // Restoring view-only state must not replace the original cancellation.
  }
}
