import type {
  TourId,
  TourProgressEntry,
  TourProgressState,
  TourProgressStore,
  TourProgressV1,
} from "./tourTypes";

export const TOUR_PROGRESS_STORAGE_KEY = "relay-pool.tours.progress.v1";
export const TOUR_PROGRESS_SCHEMA_VERSION = 1 as const;
export const MAX_TOUR_PROGRESS_PAYLOAD_LENGTH = 64 * 1024;

export type TourProgressStorage = Pick<Storage, "getItem" | "setItem"> &
  Partial<Pick<Storage, "removeItem">>;

export const ALL_TOUR_IDS: readonly TourId[] = [
  "full",
  "basic",
  "dashboard",
  "stations",
  "key-pool",
  "routing",
  "pricing",
  "channels",
  "changes",
  "logs",
  "settings",
  "proxy",
  "station-setup",
  "monitoring",
  "advanced",
];

function emptyProgress(): TourProgressV1 {
  return { schemaVersion: TOUR_PROGRESS_SCHEMA_VERSION, tours: {} };
}

function browserStorage(): TourProgressStorage | null {
  try {
    return typeof window === "undefined" ? null : window.localStorage;
  } catch {
    return null;
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isTourId(value: string): value is TourId {
  return ALL_TOUR_IDS.includes(value as TourId);
}

function isProgressState(value: unknown): value is TourProgressState {
  return value === "completed" || value === "skipped";
}

function parseEntry(value: unknown): TourProgressEntry | null {
  if (!isRecord(value)) return null;
  const revision = value.revision;
  const state = value.state;
  const updatedAt = value.updatedAt;
  if (
    typeof revision !== "number" ||
    !Number.isSafeInteger(revision) ||
    revision <= 0 ||
    !isProgressState(state) ||
    typeof updatedAt !== "number" ||
    !Number.isFinite(updatedAt) ||
    updatedAt < 0
  ) {
    return null;
  }
  return { revision, state, updatedAt };
}

/** Strictly parse the v1 payload, ignoring unknown tour ids and fields. */
export function parseTourProgress(value: unknown): TourProgressV1 {
  if (!isRecord(value) || value.schemaVersion !== TOUR_PROGRESS_SCHEMA_VERSION || !isRecord(value.tours)) {
    return emptyProgress();
  }

  const tours: Partial<Record<TourId, TourProgressEntry>> = {};
  for (const [tourId, entry] of Object.entries(value.tours)) {
    if (!isTourId(tourId)) continue;
    const parsed = parseEntry(entry);
    if (!parsed) return emptyProgress();
    tours[tourId] = parsed;
  }
  return { schemaVersion: TOUR_PROGRESS_SCHEMA_VERSION, tours };
}

export function readTourProgress(
  storage: TourProgressStorage | null = browserStorage(),
): TourProgressV1 {
  try {
    const raw = storage?.getItem(TOUR_PROGRESS_STORAGE_KEY);
    if (raw == null || raw.length > MAX_TOUR_PROGRESS_PAYLOAD_LENGTH) return emptyProgress();
    return parseTourProgress(JSON.parse(raw));
  } catch {
    return emptyProgress();
  }
}

export function writeTourProgress(
  progress: TourProgressV1,
  storage: TourProgressStorage | null = browserStorage(),
): boolean {
  try {
    if (!storage) return false;
    const normalized = parseTourProgress(progress);
    const serialized = JSON.stringify(normalized);
    if (serialized.length > MAX_TOUR_PROGRESS_PAYLOAD_LENGTH) return false;
    storage.setItem(TOUR_PROGRESS_STORAGE_KEY, serialized);
    return true;
  } catch {
    return false;
  }
}

export function resetTourProgress(
  tourId?: TourId,
  storage: TourProgressStorage | null = browserStorage(),
): boolean {
  try {
    if (!storage) return false;
    if (!tourId) {
      if (typeof storage.removeItem === "function") {
        storage.removeItem(TOUR_PROGRESS_STORAGE_KEY);
      } else {
        storage.setItem(TOUR_PROGRESS_STORAGE_KEY, JSON.stringify(emptyProgress()));
      }
      return true;
    }
    const progress = readTourProgress(storage);
    delete progress.tours[tourId];
    return writeTourProgress(progress, storage);
  } catch {
    return false;
  }
}

function commit(
  state: TourProgressState,
  tourId: TourId,
  revision: number,
  updatedAt: number,
  progress: TourProgressV1,
): TourProgressV1 | null {
  if (!Number.isSafeInteger(revision) || revision <= 0 || !Number.isFinite(updatedAt) || updatedAt < 0) return null;
  return {
    schemaVersion: TOUR_PROGRESS_SCHEMA_VERSION,
    tours: {
      ...progress.tours,
      [tourId]: { revision, state, updatedAt },
    },
  };
}

export function createTourProgressStore(
  storage: TourProgressStorage | null = browserStorage(),
  now: () => number = () => Date.now(),
): TourProgressStore {
  let progress = readTourProgress(storage);

  const save = (next: TourProgressV1): boolean => {
    progress = next;
    return writeTourProgress(next, storage);
  };

  return {
    getSnapshot: () => ({
      schemaVersion: progress.schemaVersion,
      tours: Object.fromEntries(
        Object.entries(progress.tours).map(([tourId, entry]) => [tourId, entry ? { ...entry } : entry]),
      ) as TourProgressV1["tours"],
    }),
    commitCompletion: (tourId, revision, updatedAt = now()) => {
      const next = commit("completed", tourId, revision, updatedAt, progress);
      return next ? save(next) : false;
    },
    commitSkipped: (tourId, revision, updatedAt = now()) => {
      const next = commit("skipped", tourId, revision, updatedAt, progress);
      return next ? save(next) : false;
    },
    reset: (tourId) => {
      if (!tourId) {
        progress = emptyProgress();
        return resetTourProgress(undefined, storage);
      }
      delete progress.tours[tourId];
      return resetTourProgress(tourId, storage);
    },
  };
}
