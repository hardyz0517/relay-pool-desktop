import type { QueryClient, QueryKey } from "@tanstack/react-query";
import { listen, type Event, type UnlistenFn } from "@tauri-apps/api/event";
import { getStationAssetsRevision, getStationDetailRevision } from "@/lib/api/stations";
import type {
  DomainRevisionNoticeDto,
  DomainRevisionVectorEntryDto,
  ReadModelRevisionDto,
} from "@/lib/bridge/generated";
import { queryKeys } from "@/lib/query/queryKeys";
import { routingQueryKeys } from "@/lib/queries/routingQueries";

/** Versioned Tauri event emitted after a committed station read-model mutation. */
export const DOMAIN_REVISION_UPDATED_EVENT = "domain-revision-updated" as const;

const MAX_NOTICE_SCOPES = 64;
const MAX_STATION_ID_LENGTH = 200;

export type DomainRevisionVectorEntry = DomainRevisionVectorEntryDto;
export type DomainRevisionNotice = DomainRevisionNoticeDto;

type UnknownRecord = Record<string, unknown>;

type RevisionTracker = Map<string, number>;

/**
 * Validates the generated native event contract at the untrusted event boundary.
 */
export function normalizeDomainRevisionNotice(value: unknown): DomainRevisionNotice | null {
  if (!isRecord(value)) return null;

  const mutationId = readBoundedString(value.mutationId, 200);
  const affectedScopes = readScopes(value.affectedScopes);
  const revisionVector = readRevisionVector(value.revisionVector);
  if (!mutationId || (affectedScopes.length === 0 && revisionVector.length === 0)) return null;

  const mergedScopes = [...new Set([...affectedScopes, ...revisionVector.map((entry) => entry.scope)])];
  return {
    mutationId,
    affectedScopes: mergedScopes.slice(0, MAX_NOTICE_SCOPES),
    revisionVector,
  };
}

/**
 * Convert a notice into stable query-family keys. This is the only production
 * owner of station collection/authorization invalidation fan-out.
 */
export function queryKeysForDomainRevisionScopes(scopes: readonly string[]): QueryKey[] {
  const keys: QueryKey[] = [];
  const collectionStationIds = new Set<string>();
  const authorizationStationIds = new Set<string>();
  const detailStationIds = new Set<string>();
  let workspaceAssets = false;
  let workspaceStations = false;

  for (const scope of scopes) {
    if (scope === "read_model:station_assets") {
      workspaceAssets = true;
      workspaceStations = true;
      continue;
    }
    const collectionMatch = /^station_collection:(.+)$/.exec(scope);
    const authorizationMatch = /^station_authorization:(.+)$/.exec(scope);
    const detailMatch = /^read_model:station_detail:(.+)$/.exec(scope);
    const stationId = collectionMatch?.[1] ?? authorizationMatch?.[1] ?? detailMatch?.[1];
    if (!stationId || !isSafeStationId(stationId)) continue;

    detailStationIds.add(stationId);
    if (detailMatch) continue;
    workspaceStations = true;
    workspaceAssets = true;
    if (collectionMatch) collectionStationIds.add(stationId);
    if (authorizationMatch) authorizationStationIds.add(stationId);
  }

  if (workspaceStations) keys.push(queryKeys.stations);
  if (workspaceAssets) keys.push(queryKeys.stationAssets);
  // Station collection and authorization revisions can change the effective
  // multiplier or the set of keys eligible for scoring. The routing snapshot
  // must be rebuilt so its median reference and every derived cost score stay
  // aligned with the new durable facts.
  if (workspaceAssets) keys.push(routingQueryKeys.all);
  for (const stationId of detailStationIds) {
    keys.push(queryKeys.stationDetail(stationId));
  }
  if (collectionStationIds.size > 0) {
    keys.push(
      queryKeys.balanceSnapshots,
      queryKeys.keyPool,
      queryKeys.pricing,
      queryKeys.stationPublishedStatusRoot,
    );
  }

  for (const stationId of collectionStationIds) {
    keys.push(
      queryKeys.collectorSnapshots(stationId),
      queryKeys.collectorRuns(stationId),
    );
  }
  for (const stationId of authorizationStationIds) {
    keys.push(queryKeys.captureSessionStatus(stationId));
  }

  return dedupeQueryKeys(keys);
}

export async function reconcileStationDetailReadModel(
  queryClient: QueryClient,
  stationId: string,
  tracker: RevisionTracker = new Map(),
  probe: (stationId: string) => Promise<ReadModelRevisionDto> = getStationDetailRevision,
): Promise<StationReadModelSynchronizationResult> {
  const expectedScope = `read_model:station_detail:${stationId}`;
  let durable: ReadModelRevisionDto;
  try {
    durable = await probe(stationId);
  } catch (error) {
    return { refreshed: false, invalidatedKeys: [], ignoredScopes: [], errors: [error] };
  }
  if (
    durable.scope !== expectedScope ||
    !Number.isSafeInteger(durable.revision) ||
    durable.revision < 1
  ) {
    return {
      refreshed: false,
      invalidatedKeys: [],
      ignoredScopes: [],
      errors: [new Error("invalid station detail revision probe")],
    };
  }
  const previous = tracker.get(expectedScope);
  if (previous !== undefined && durable.revision <= previous) {
    return { refreshed: true, invalidatedKeys: [], ignoredScopes: [expectedScope], errors: [] };
  }
  const invalidatedKeys = [queryKeys.stationDetail(stationId)];
  const errors = await invalidateQueryKeys(queryClient, invalidatedKeys);
  if (errors.length === 0) tracker.set(expectedScope, durable.revision);
  return { refreshed: errors.length === 0, invalidatedKeys, ignoredScopes: [], errors };
}

export type StationReadModelSynchronizationResult = {
  readonly refreshed: boolean;
  readonly invalidatedKeys: readonly QueryKey[];
  readonly ignoredScopes: readonly string[];
  readonly errors: readonly unknown[];
};

const STATION_ASSETS_READ_MODEL_SCOPE = "read_model:station_assets";

/**
 * Reconciles the station workspace after startup or a WebView resume. Native
 * revision events remain the fast path; this bounded probe repairs missed
 * best-effort events through the same query-family mapping owner.
 */
export async function reconcileStationReadModels(
  queryClient: QueryClient,
  tracker: RevisionTracker = new Map(),
  probe: () => Promise<ReadModelRevisionDto> = getStationAssetsRevision,
): Promise<StationReadModelSynchronizationResult> {
  let durable: ReadModelRevisionDto;
  try {
    durable = await probe();
  } catch (error) {
    return { refreshed: false, invalidatedKeys: [], ignoredScopes: [], errors: [error] };
  }
  if (
    durable.scope !== STATION_ASSETS_READ_MODEL_SCOPE ||
    !Number.isSafeInteger(durable.revision) ||
    durable.revision < 0
  ) {
    return {
      refreshed: false,
      invalidatedKeys: [],
      ignoredScopes: [],
      errors: [new Error("invalid station asset revision probe")],
    };
  }
  const previous = tracker.get(durable.scope);
  if (previous !== undefined && durable.revision <= previous) {
    return {
      refreshed: true,
      invalidatedKeys: [],
      ignoredScopes: [durable.scope],
      errors: [],
    };
  }
  const invalidatedKeys = queryKeysForDomainRevisionScopes([STATION_ASSETS_READ_MODEL_SCOPE]);
  const errors = await invalidateQueryKeys(queryClient, invalidatedKeys);
  if (errors.length === 0) tracker.set(durable.scope, durable.revision);
  return {
    refreshed: errors.length === 0,
    invalidatedKeys,
    ignoredScopes: [],
    errors,
  };
}

/**
 * Applies only newer revisions and invalidates the exact query families that
 * can be affected. Duplicate and out-of-order notices become no-ops.
 */
export async function synchronizeStationReadModels(
  queryClient: QueryClient,
  notice: unknown,
  tracker: RevisionTracker = new Map(),
): Promise<StationReadModelSynchronizationResult> {
  const normalized = normalizeDomainRevisionNotice(notice);
  if (!normalized) {
    return { refreshed: true, invalidatedKeys: [], ignoredScopes: [], errors: [] };
  }

  const acceptedScopes: string[] = [];
  const ignoredScopes: string[] = [];
  const acceptedRevisions = new Map<string, number>();
  for (const entry of normalized.revisionVector) {
    const previous = tracker.get(entry.scope);
    if (previous !== undefined && entry.revision <= previous) {
      ignoredScopes.push(entry.scope);
      continue;
    }
    acceptedScopes.push(entry.scope);
    acceptedRevisions.set(entry.scope, entry.revision);
  }

  // Older producers may provide only affectedScopes. Treat those as a hint,
  // but do not let an unversioned duplicate cause unbounded invalidations.
  if (normalized.revisionVector.length === 0) {
    acceptedScopes.push(...normalized.affectedScopes.filter((scope) => !tracker.has(scope)));
    acceptedScopes.forEach((scope) => acceptedRevisions.set(scope, 0));
  }

  const invalidatedKeys = queryKeysForDomainRevisionScopes(acceptedScopes);
  const errors = await invalidateQueryKeys(queryClient, invalidatedKeys);
  if (errors.length === 0) {
    acceptedRevisions.forEach((revision, scope) => tracker.set(scope, revision));
  }
  return {
    refreshed: errors.length === 0,
    invalidatedKeys,
    ignoredScopes,
    errors,
  };
}

export type DomainRevisionEventSubscriber = (
  event: typeof DOMAIN_REVISION_UPDATED_EVENT,
  handler: (event: Event<unknown>) => void,
) => Promise<UnlistenFn>;

/**
 * Subscribes to native revision notices. The process-wide synchronizer installs
 * this listener before its initial reconciliation to close the startup race.
 */
export async function subscribeToDomainRevisionUpdates(
  queryClient: QueryClient,
  subscribe: DomainRevisionEventSubscriber = (event, handler) => listen(event, handler),
  tracker: RevisionTracker = new Map(),
): Promise<UnlistenFn> {
  const onEvent = (event: Event<unknown>) => {
    void synchronizeStationReadModels(queryClient, event.payload, tracker);
  };
  const unlisten = await subscribe(DOMAIN_REVISION_UPDATED_EVENT, onEvent);
  return unlisten;
}

async function invalidateQueryKeys(
  queryClient: QueryClient,
  queryKeysToInvalidate: readonly QueryKey[],
): Promise<unknown[]> {
  const refreshes = await Promise.allSettled(
    queryKeysToInvalidate.map((queryKey) => queryClient.invalidateQueries({ queryKey })),
  );
  return refreshes.flatMap((refresh) => (refresh.status === "rejected" ? [refresh.reason] : []));
}

function isRecord(value: unknown): value is UnknownRecord {
  return Boolean(value) && typeof value === "object" && !Array.isArray(value);
}

function readBoundedString(value: unknown, maxLength: number): string | null {
  return typeof value === "string" && value.trim().length > 0 && value.length <= maxLength
    ? value.trim()
    : null;
}

function readScopes(value: unknown): string[] {
  if (!Array.isArray(value)) return [];
  return value
    .slice(0, MAX_NOTICE_SCOPES)
    .map((scope) => readBoundedString(scope, MAX_STATION_ID_LENGTH + 64))
    .filter((scope): scope is string => scope !== null);
}

function readRevisionVector(value: unknown): DomainRevisionVectorEntry[] {
  if (!Array.isArray(value)) return [];
  const entries: DomainRevisionVectorEntry[] = [];
  for (const item of value.slice(0, MAX_NOTICE_SCOPES)) {
    let scope: unknown;
    let revision: unknown;
    if (isRecord(item)) {
      scope = item.scope;
      revision = item.revision;
    }
    const normalizedScope = readBoundedString(scope, MAX_STATION_ID_LENGTH + 64);
    const normalizedRevision = typeof revision === "number" && Number.isSafeInteger(revision) && revision >= 0
      ? revision
      : null;
    if (normalizedScope && normalizedRevision !== null) {
      entries.push({ scope: normalizedScope, revision: normalizedRevision });
    }
  }
  return entries;
}

function dedupeQueryKeys(keys: QueryKey[]): QueryKey[] {
  const seen = new Set<string>();
  return keys.filter((key) => {
    const identity = JSON.stringify(key);
    if (seen.has(identity)) return false;
    seen.add(identity);
    return true;
  });
}

function isSafeStationId(value: string): boolean {
  return value.length <= MAX_STATION_ID_LENGTH && /^[A-Za-z0-9][A-Za-z0-9_-]*$/.test(value);
}
