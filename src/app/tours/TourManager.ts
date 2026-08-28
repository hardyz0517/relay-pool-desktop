import type {
  TourDefinition,
  TourDriverPort,
  TourId,
  TourManagerApi,
  TourManagerPhase,
  TourManagerSnapshot,
  TourNavigationPort,
  TourPreparationCleanup,
  TourPreparationRegistry,
  TourProgressStore,
  TourSource,
  TourTargetResolver,
} from "./tourTypes";
import { TourNavigationCancelledError } from "./tourNavigation";
import { TourPreparationError } from "./tourPreparationRegistry";
import { TourTargetResolverError } from "./tourTargetResolver";

export type TourManagerDeps = {
  driver: TourDriverPort;
  navigation: TourNavigationPort;
  targetResolver: TourTargetResolver;
  preparation: TourPreparationRegistry;
  progress: TourProgressStore;
  catalog: readonly TourDefinition[] | Readonly<Record<string, TourDefinition>>;
  isDeveloperMode?: () => boolean;
  hasBlockingModal?: () => boolean;
  now?: () => number;
};

export type TourSnapshotListener = (snapshot: TourManagerSnapshot) => void;

type Session = {
  id: number;
  source: TourSource;
  definition: TourDefinition;
  index: number;
  abort: AbortController;
  stepAbort: AbortController | null;
  driverGeneration: number | null;
  preparationCleanup: TourPreparationCleanup | null;
};

const initialSnapshot = (): TourManagerSnapshot => ({
  phase: "idle",
  tourId: null,
  stepIndex: 0,
  stepCount: 0,
  source: null,
  message: null,
});

function isAbortError(error: unknown): boolean {
  if (error instanceof DOMException && error.name === "AbortError") return true;
  if (error instanceof Error && /abort|cancel/i.test(error.name + " " + error.message)) return true;
  return false;
}

/**
 * The sole owner of tour state and orchestration. It knows about ports, not
 * Driver.js, React, query caches, or business controllers.
 */
export class TourManager implements TourManagerApi {
  private readonly driver: TourDriverPort;
  private readonly navigation: TourNavigationPort;
  private readonly targetResolver: TourTargetResolver;
  private readonly preparation: TourPreparationRegistry;
  private readonly progress: TourProgressStore;
  private readonly catalog: ReadonlyMap<TourId, TourDefinition>;
  private readonly isDeveloperMode: () => boolean;
  private readonly hasBlockingModal: () => boolean;
  private readonly now: () => number;
  private readonly listeners = new Set<TourSnapshotListener>();
  private snapshot: TourManagerSnapshot = initialSnapshot();
  private session: Session | null = null;
  private nextSessionId = 0;
  private nextRequestToken = 0;
  private nextDriverGeneration = 0;
  private disposed = false;

  constructor(deps: TourManagerDeps) {
    this.driver = deps.driver;
    this.navigation = deps.navigation;
    this.targetResolver = deps.targetResolver;
    this.preparation = deps.preparation;
    this.progress = deps.progress;
    const definitions = Array.isArray(deps.catalog) ? deps.catalog : Object.values(deps.catalog);
    this.catalog = new Map(definitions.map((definition) => [definition.id, definition]));
    this.isDeveloperMode = deps.isDeveloperMode ?? (() => false);
    this.hasBlockingModal = deps.hasBlockingModal ?? (() => false);
    this.now = deps.now ?? Date.now;
  }

  subscribe(listener: TourSnapshotListener): () => void {
    this.listeners.add(listener);
    listener(this.getSnapshot());
    return () => this.listeners.delete(listener);
  }

  getSnapshot(): TourManagerSnapshot {
    // Snapshot values are replaced atomically in setSnapshot and contain only
    // primitives. Returning the stable reference is required by
    // useSyncExternalStore; cloning here would make every render look like a
    // store update and can cause an infinite re-render loop.
    return this.snapshot;
  }

  start(tourId: TourId, source: TourSource = "settings"): boolean {
    if (this.disposed) return false;
    if (source === "auto" && this.session) return false;
    const definition = this.catalog.get(tourId);
    if (!definition) {
      this.reportStartError("教程暂不可用");
      return false;
    }
    if (definition.steps.length === 0) {
      this.reportStartError("教程没有可用步骤");
      return false;
    }
    if (definition.requires === "developer-mode" && !this.isDeveloperMode()) {
      this.reportStartError("当前教程仅在开发者模式下可用");
      return false;
    }
    if (source === "auto" && this.isAlreadyHandled(tourId)) return false;
    if (this.hasBlockingModal()) {
      this.reportStartError("请先关闭当前对话框，再开始教程");
      return false;
    }

    // Only a validated, non-blocked manual request may supersede a running
    // session. Invalid commands must not tear down a tour that is still usable.
    this.endSession("close", true);

    const session: Session = {
      id: ++this.nextSessionId,
      source,
      definition,
      index: 0,
      abort: new AbortController(),
      stepAbort: null,
      driverGeneration: null,
      preparationCleanup: null,
    };
    this.session = session;
    try {
      this.driver.beginSession();
    } catch {
      // Focus restoration is an accessibility enhancement. A host-specific
      // focus read failure must not reject an otherwise valid tutorial.
    }
    this.setSnapshot({ phase: "preparing", tourId, stepIndex: 0, stepCount: definition.steps.length, source, message: null });
    void this.present(session, 0);
    return true;
  }

  next(): void {
    const session = this.session;
    if (!session || this.snapshot.phase !== "running") return;
    if (session.index >= session.definition.steps.length - 1) {
      this.complete(session);
      return;
    }
    this.goTo(session, session.index + 1);
  }

  previous(): void {
    const session = this.session;
    if (!session || this.snapshot.phase !== "running" || session.index <= 0) return;
    this.goTo(session, session.index - 1);
  }

  retry(): void {
    const session = this.session;
    if (!session || this.snapshot.phase !== "error") return;
    this.cancelStep(session);
    this.destroyDriver(session, "step-change");
    this.setSnapshot({ ...this.snapshot, phase: "preparing", message: null });
    void this.present(session, session.index);
  }

  skip(): void {
    if (!this.session) return;
    this.finishSkipped(this.session, "教程已跳过");
  }

  close(): void {
    if (!this.session) return;
    this.finishSkipped(this.session, "教程已关闭");
  }

  resetProgress(tourId?: TourId): void {
    // Progress is the durable record of a completed session. Resetting it
    // while that session is still active makes the visible tour and persisted
    // history disagree, so require the user to exit before resetting.
    if (this.session) {
      this.setSnapshot({ ...this.snapshot, message: "请先退出当前教程再重置进度" });
      return;
    }
    try {
      const persisted = this.progress.reset(tourId);
      if (!persisted) {
        this.setSnapshot({ ...initialSnapshot(), message: "教程进度未能持久化" });
      }
    } catch {
      this.setSnapshot({ ...initialSnapshot(), message: "重置教程进度失败" });
    }
  }

  dispose(): void {
    if (this.disposed) return;
    this.disposed = true;
    this.endSession("dispose", false);
    this.listeners.clear();
  }

  private isAlreadyHandled(tourId: TourId): boolean {
    try {
      const entry = this.progress.getSnapshot().tours[tourId];
      // Automatic onboarding is installation-first-run behavior, not revision
      // migration behavior. Any prior handled revision keeps later revisions in
      // the tutorial center as an explicit "updated" replay instead of
      // interrupting an existing user after an application upgrade.
      return Boolean(entry && (entry.state === "completed" || entry.state === "skipped"));
    } catch {
      return false;
    }
  }

  private goTo(session: Session, index: number): void {
    if (this.session !== session || session.abort.signal.aborted) return;
    this.cancelStep(session);
    this.destroyDriver(session, "step-change");
    session.index = index;
    this.setSnapshot({ ...this.snapshot, phase: "preparing", stepIndex: index, message: null });
    void this.present(session, index);
  }

  private async present(session: Session, index: number): Promise<void> {
    const step = session.definition.steps[index];
    if (!step || !this.isCurrent(session)) return;
    const stepAbort = new AbortController();
    session.stepAbort = stepAbort;
    const abortStep = () => stepAbort.abort();
    session.abort.signal.addEventListener("abort", abortStep, { once: true });
    try {
      if (step.requires === "developer-mode" && !this.isDeveloperMode()) {
        if (step.optional) return this.advanceOptional(session, index);
        return this.fail(session, "当前步骤仅在开发者模式下可用");
      }
      const prepareKey = step.prepareKey ?? "none";
      if (!this.preparation.has(prepareKey)) {
        if (step.optional) return this.advanceOptional(session, index);
        return this.fail(session, "当前步骤缺少页面准备动作");
      }
      const current = this.navigation.getCurrent();
      const requestToken = ++this.nextRequestToken;
      this.navigation.navigate(step.route, requestToken);
      this.setSnapshot({ ...this.snapshot, phase: "waiting-target", stepIndex: index, message: null });
      await this.navigation.waitForReady({
        routeId: step.route,
        sessionId: session.id,
        requestToken,
        // The port explicitly treats a settled same-route request as ready;
        // sequence filtering remains relevant for real route transitions.
        afterSequence: current.sequence,
        signal: stepAbort.signal,
      });
      if (!this.isCurrent(session)) return;

      this.setSnapshot({ ...this.snapshot, phase: "preparing", stepIndex: index, message: null });
      const preparationCleanup = await this.preparation.run(
        prepareKey,
        { tourId: session.definition.id, stepId: step.id, route: step.route },
        stepAbort.signal,
      );
      if (!this.isCurrent(session) || stepAbort.signal.aborted) {
        this.safelyCleanup(preparationCleanup);
        return;
      }
      session.preparationCleanup = preparationCleanup;

      this.setSnapshot({ ...this.snapshot, phase: "waiting-target", stepIndex: index, message: null });
      const element = await this.targetResolver.waitForTarget(step.target.anchor, step.route, stepAbort.signal);
      if (!this.isCurrent(session)) return;
      session.stepAbort = null;
      this.setSnapshot({ ...this.snapshot, phase: "running", stepIndex: index, message: null });
      const driverGeneration = ++this.nextDriverGeneration;
      session.driverGeneration = driverGeneration;
      this.driver.showStep({
        element,
        title: step.title,
        description: step.description,
        side: step.side,
        align: step.align,
        stepIndex: index,
        stepCount: session.definition.steps.length,
        callbacks: {
          next: () => this.onDriverNext(session.id, index, driverGeneration),
          previous: () => this.onDriverPrevious(session.id, index, driverGeneration),
          close: () => this.onDriverClose(session.id, index, driverGeneration),
          destroyed: () => this.onDriverDestroyed(session.id, index, driverGeneration),
        },
      });
    } catch (error) {
      if (!this.isCurrent(session) || isAbortError(error) || stepAbort.signal.aborted) return;
      if (step.optional) return this.advanceOptional(session, index);
      this.fail(session, this.messageFor(error));
    } finally {
      session.abort.signal.removeEventListener("abort", abortStep);
      if (session.stepAbort === stepAbort) session.stepAbort = null;
    }
  }

  private advanceOptional(session: Session, index: number): void {
    if (!this.isCurrent(session) || session.index !== index) return;
    this.cancelStep(session);
    if (index >= session.definition.steps.length - 1) {
      this.complete(session);
      return;
    }
    session.index = index + 1;
    this.setSnapshot({ ...this.snapshot, phase: "preparing", stepIndex: session.index, message: null });
    void this.present(session, session.index);
  }

  private complete(session: Session): void {
    if (!this.isCurrent(session)) return;
    this.cancelStep(session);
    let persisted: boolean;
    try {
      const completedAt = this.now();
      persisted = this.progress.commitCompletion(
        session.definition.id,
        session.definition.revision,
        completedAt,
      );
    } catch {
      persisted = false;
    }
    this.destroyDriver(session, "complete");
    session.abort.abort();
    this.session = null;
    // Completion is a user-visible session result even when localStorage is
    // unavailable. The store may still retain the state in memory; surface a
    // non-blocking warning so the UI can explain that auto-start may recur.
    this.setSnapshot({
      phase: "completed",
      tourId: session.definition.id,
      stepIndex: session.index,
      stepCount: session.definition.steps.length,
      source: session.source,
      message: persisted ? null : "教程已完成，但进度未能持久化",
    });
  }

  private finishSkipped(session: Session, message: string): void {
    if (!this.isCurrent(session)) return;
    this.cancelStep(session);
    let persisted = false;
    try {
      persisted = this.progress.commitSkipped(session.definition.id, session.definition.revision, this.now());
    } catch {
      // Exiting the overlay must remain possible even if localStorage is unavailable.
    }
    this.destroyDriver(session, message.includes("关闭") ? "close" : "skip");
    session.abort.abort();
    this.session = null;
    this.setSnapshot({
      phase: "skipped",
      tourId: session.definition.id,
      stepIndex: session.index,
      stepCount: session.definition.steps.length,
      source: session.source,
      message: persisted ? message : `${message}，但进度未能持久化`,
    });
  }

  private endSession(reason: "close" | "dispose", persist: boolean): void {
    const session = this.session;
    if (!session) return;
    if (persist) this.finishSkipped(session, "教程已关闭");
    else {
      this.cancelStep(session);
      this.destroyDriver(session, reason);
      session.abort.abort();
      this.session = null;
    }
  }

  private cancelStep(session: Session): void {
    session.stepAbort?.abort();
    session.stepAbort = null;
    const cleanup = session.preparationCleanup;
    session.preparationCleanup = null;
    this.safelyCleanup(cleanup);
  }

  private safelyCleanup(cleanup: TourPreparationCleanup | null): void {
    try {
      cleanup?.();
    } catch {
      // View restoration must never prevent session teardown or navigation.
    }
  }

  private isCurrent(session: Session): boolean {
    return !this.disposed && this.session === session && !session.abort.signal.aborted;
  }

  private onDriverNext(sessionId: number, stepIndex: number, generation: number): void {
    if (this.isCurrentDriver(sessionId, stepIndex, generation)) this.next();
  }

  private onDriverPrevious(sessionId: number, stepIndex: number, generation: number): void {
    if (this.isCurrentDriver(sessionId, stepIndex, generation)) this.previous();
  }

  private onDriverClose(sessionId: number, stepIndex: number, generation: number): void {
    if (this.isCurrentDriver(sessionId, stepIndex, generation)) this.close();
  }

  private onDriverDestroyed(sessionId: number, stepIndex: number, generation: number): void {
    const session = this.session;
    if (session && this.isCurrentDriver(sessionId, stepIndex, generation) && this.snapshot.phase === "running") {
      this.finishSkipped(session, "教程显示已关闭");
    }
  }

  private isCurrentDriver(sessionId: number, stepIndex: number, generation: number): boolean {
    return this.session?.id === sessionId &&
      this.session.index === stepIndex &&
      this.session.driverGeneration === generation;
  }

  private fail(session: Session, message: string): void {
    if (!this.isCurrent(session)) return;
    this.cancelStep(session);
    this.destroyDriver(session, "step-change");
    this.setSnapshot({ phase: "error", tourId: session.definition.id, stepIndex: session.index, stepCount: session.definition.steps.length, source: session.source, message });
  }

  private messageFor(error: unknown): string {
    if (error instanceof TourTargetResolverError) {
      if (error.code === "timeout") return "当前步骤加载超时，请重试";
      if (error.code === "invalid-anchor") return "当前步骤配置无效";
    }
    if (error instanceof TourPreparationError) return "当前页面准备失败，请重试";
    if (error instanceof TourNavigationCancelledError) return "页面切换已取消，请重试";
    return "当前步骤暂不可用，请重试或退出教程";
  }

  private reportStartError(message: string): void {
    // Do not replace a live session with an error caused by a rejected command.
    if (this.session) return;
    // A rejected command did not create a session, so do not expose the
    // session error controls (retry/close would be inert). React may surface
    // this message as a non-blocking toast.
    this.setSnapshot({ ...initialSnapshot(), message });
  }

  private destroyDriver(session: Session, reason: "step-change" | "skip" | "close" | "complete" | "dispose"): void {
    session.driverGeneration = null;
    try {
      this.driver.destroy(reason);
    } catch {
      // Teardown must never prevent navigation or disposal from completing.
    }
  }

  private setSnapshot(snapshot: TourManagerSnapshot): void {
    this.snapshot = snapshot;
    for (const listener of this.listeners) listener(this.getSnapshot());
  }
}

export type { TourManagerApi, TourManagerPhase, TourManagerSnapshot };
