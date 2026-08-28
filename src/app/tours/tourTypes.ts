import type { AppPageId, AppRouteId } from "@/lib/types/navigation";

/** All tour ids understood by the progress format. Only published ids are in the catalog. */
export type TourId =
  | "full"
  | "basic"
  | "dashboard"
  | "stations"
  | "key-pool"
  | "routing"
  | "pricing"
  | "channels"
  | "changes"
  | "logs"
  | "settings"
  | "proxy"
  | "station-setup"
  | "monitoring"
  | "advanced";

export type PublishedTourId =
  | "full"
  | "basic"
  | "dashboard"
  | "stations"
  | "key-pool"
  | "routing"
  | "pricing"
  | "channels"
  | "changes"
  | "logs"
  | "settings";

export type TourCategory = "recommended" | "page";

export type TourSource = "auto" | "settings" | "test";

export type TourStepSide = "top" | "right" | "bottom" | "left";
export type TourStepAlign = "start" | "center" | "end";

/**
 * Preparation is deliberately expressed as a key rather than an arbitrary callback.
 * The explicit union keeps catalog declarations reviewable and prevents a
 * misspelled preparation from surviving until runtime.
 */
export type TourPreparationKey =
  | "none"
  | "routing-status-tab"
  | "routing-settings-tab"
  | "channels-local-tab"
  | "channels-official-tab"
  | "channels-monitoring-tab";

export type TourStepRequirement = "always" | "developer-mode";

export type TourStep = {
  id: string;
  route: AppPageId;
  target: { anchor: string };
  title: string;
  description: string;
  side?: TourStepSide;
  align?: TourStepAlign;
  optional?: boolean;
  prepareKey?: TourPreparationKey;
  requires?: TourStepRequirement;
};

export type TourDefinition<TId extends TourId = TourId> = {
  id: TId;
  category: TourCategory;
  order: number;
  title: string;
  summary: string;
  revision: number;
  estimatedMinutes?: number;
  steps: readonly TourStep[];
  requires?: TourStepRequirement;
};

export type TourProgressState = "completed" | "skipped";

export type TourProgressEntry = {
  revision: number;
  state: TourProgressState;
  updatedAt: number;
};

export type TourProgressV1 = {
  schemaVersion: 1;
  tours: Partial<Record<TourId, TourProgressEntry>>;
};

export type TourManagerPhase =
  | "idle"
  | "preparing"
  | "running"
  | "waiting-target"
  | "completed"
  | "skipped"
  | "error";

export type TourManagerSnapshot = {
  phase: TourManagerPhase;
  tourId: TourId | null;
  stepIndex: number;
  stepCount: number;
  source: TourSource | null;
  message: string | null;
};

export type TourManagerApi = {
  /** Returns false when the request was rejected before a session was created. */
  start(tourId: TourId, source?: TourSource): boolean;
  next(): void;
  previous(): void;
  retry(): void;
  skip(): void;
  close(): void;
  resetProgress(tourId?: TourId): void;
  getSnapshot(): TourManagerSnapshot;
  subscribe(listener: (snapshot: TourManagerSnapshot) => void): () => void;
  dispose(): void;
};

export type TourDriverDestroyReason = "step-change" | "skip" | "close" | "complete" | "dispose";

export type TourDriverPort = {
  /** Capture the focus origin before any preparation or cross-page navigation. */
  beginSession(): void;
  showStep(input: {
    element: HTMLElement;
    title: string;
    description: string;
    side?: TourStepSide;
    align?: TourStepAlign;
    stepIndex: number;
    stepCount: number;
    callbacks: {
      next: () => void;
      previous: () => void;
      close: () => void;
      destroyed: () => void;
    };
  }): void;
  destroy(reason: TourDriverDestroyReason): void;
};

export type TourNavigationRequest = {
  routeId: AppPageId;
  sessionId: number;
  requestToken: number;
  afterSequence: number;
  signal: AbortSignal;
};

export type NavigationReadySnapshot = {
  routeId: AppPageId;
  shellRouteId: AppRouteId;
  sequence: number;
};

export type TourNavigationCurrent = {
  routeId: AppPageId;
  shellRouteId: AppRouteId;
  sequence: number;
  pending: boolean;
};

export type TourNavigationPort = {
  navigate(routeId: AppPageId, requestToken: number): void;
  getCurrent(): TourNavigationCurrent;
  waitForReady(request: TourNavigationRequest): Promise<NavigationReadySnapshot>;
};

export type TourTargetResolver = {
  waitForTarget(anchor: string, route: AppPageId, signal: AbortSignal): Promise<HTMLElement>;
};

export type TourPreparationContext = {
  route: AppPageId;
  tourId: TourId;
  stepId: string;
};

export type TourPreparationCleanup = () => void;

export type TourPreparationRegistry = {
  has(key: TourPreparationKey): boolean;
  run(
    key: TourPreparationKey,
    context: TourPreparationContext,
    signal: AbortSignal,
  ): Promise<TourPreparationCleanup | null>;
};

export type TourProgressStore = {
  getSnapshot(): TourProgressV1;
  commitCompletion(tourId: TourId, revision: number, updatedAt?: number): boolean;
  commitSkipped(tourId: TourId, revision: number, updatedAt?: number): boolean;
  reset(tourId?: TourId): boolean;
};
