import type { CommandErrorCode, PublicErrorDetails } from "./generated";

export type { CommandErrorCode, PublicErrorDetails, PublicFieldError } from "./generated";

const MAX_MESSAGE_BYTES = 512;
const MAX_CORRELATION_ID_BYTES = 32;
const MAX_FIELD_ERRORS = 16;
const FALLBACK_MESSAGE = "The desktop operation failed.";

const ERROR_CODES = new Set<CommandErrorCode>([
  "invalid_input",
  "not_found",
  "conflict",
  "permission_denied",
  "runtime_unavailable",
  "data_store_unavailable",
  "external_unavailable",
  "timeout",
  "overloaded",
  "unsupported",
  "internal",
]);

export class BackendError extends Error {
  readonly code: CommandErrorCode;
  readonly retryable: boolean;
  readonly details?: PublicErrorDetails;
  readonly correlationId?: string;

  constructor(
    code: CommandErrorCode,
    message: string,
    retryable = false,
    details?: PublicErrorDetails,
    correlationId?: string,
  ) {
    const safeCode = ERROR_CODES.has(code) ? code : "internal";
    const safeMessage = safePublicText(message, MAX_MESSAGE_BYTES) ?? FALLBACK_MESSAGE;
    super(safeMessage);
    this.name = "BackendError";
    this.code = safeCode;
    this.retryable = safeCode === "internal" ? false : retryable;
    this.details = safeDetails(details, this.retryable);
    this.correlationId = safeCorrelationId(correlationId);
  }

  static fromUnknown(value: unknown): BackendError {
    if (!isRecord(value)) return new BackendError("internal", FALLBACK_MESSAGE);
    const correlationId = typeof value.correlationId === "string" ? value.correlationId : undefined;
    if (!isCommandErrorCode(value.code)) {
      return new BackendError("internal", FALLBACK_MESSAGE, false, undefined, correlationId);
    }
    const code = value.code;
    const message = typeof value.message === "string" ? value.message : FALLBACK_MESSAGE;
    const retryable = typeof value.retryable === "boolean" ? value.retryable : false;
    return new BackendError(code, message, retryable, parseDetails(value.details), correlationId);
  }
}

export class ResultUnknownError extends Error {
  readonly command: string;
  readonly cause: unknown;

  constructor(command: string, cause: unknown) {
    super("The desktop operation may have completed, but its result could not be confirmed.");
    this.name = "ResultUnknownError";
    this.command = command;
    this.cause = cause;
  }
}

export function isBackendError(value: unknown): value is BackendError {
  return value instanceof BackendError;
}

export function toBackendError(value: unknown): BackendError {
  return value instanceof BackendError ? value : BackendError.fromUnknown(value);
}

export function isCommandErrorEnvelope(value: unknown): boolean {
  return isRecord(value) && isCommandErrorCode(value.code);
}

function isCommandErrorCode(value: unknown): value is CommandErrorCode {
  return typeof value === "string" && ERROR_CODES.has(value as CommandErrorCode);
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function parseDetails(value: unknown): PublicErrorDetails | undefined {
  if (!isRecord(value) || typeof value.kind !== "string") return undefined;
  switch (value.kind) {
    case "validation": {
      if (!Array.isArray(value.fields) || value.fields.length === 0 || value.fields.length > MAX_FIELD_ERRORS) return undefined;
      const fields = value.fields.flatMap((item) => {
        if (!isRecord(item)) return [];
        const field = safePublicText(item.field, 64);
        const code = safePublicText(item.code, 64);
        const message = safePublicText(item.message, 256);
        return field && code && message ? [{ field, code, message }] : [];
      });
      return fields.length === value.fields.length ? { kind: "validation", fields } : undefined;
    }
    case "conflict": {
      const resource = safePublicText(value.resource, 128);
      if (!resource) return undefined;
      const revision = value.currentRevision === undefined || value.currentRevision === null
        ? undefined
        : safePublicText(value.currentRevision, 128);
      if (value.currentRevision !== undefined && value.currentRevision !== null && !revision) return undefined;
      return { kind: "conflict", resource, currentRevision: revision ?? null };
    }
    case "retry": {
      const delay = value.retryAfterMs;
      if (delay !== undefined && delay !== null && (typeof delay !== "number" || !Number.isSafeInteger(delay) || delay < 0 || delay > 300_000)) return undefined;
      return { kind: "retry", retryAfterMs: delay === null || delay === undefined ? null : delay as number };
    }
    case "external": {
      const provider = value.provider === undefined || value.provider === null ? undefined : safePublicText(value.provider, 64);
      if (value.provider !== undefined && value.provider !== null && !provider) return undefined;
      const status = value.upstreamStatus;
      if (status !== undefined && status !== null && (typeof status !== "number" || !Number.isInteger(status) || status < 100 || status > 599)) return undefined;
      return {
        kind: "external",
        provider: provider ?? null,
        upstreamStatus: status === null || status === undefined ? null : status as number,
      };
    }
    default:
      return undefined;
  }
}

function safeDetails(details: PublicErrorDetails | undefined, retryable: boolean): PublicErrorDetails | undefined {
  if (!details) return undefined;
  if (details.kind === "retry" && !retryable) return undefined;
  return parseDetails(details);
}

function safePublicText(value: unknown, maxBytes: number): string | undefined {
  if (typeof value !== "string" || value.length === 0 || new TextEncoder().encode(value).byteLength > maxBytes || /[\u0000-\u001f\u007f]/.test(value)) return undefined;
  const lower = value.toLowerCase();
  if (["://", "?", "#", "cookie", "authorization", "bearer ", "token", "secret", "api_key", "apikey", ".sqlite", "\\"].some((marker) => lower.includes(marker))) return undefined;
  if (value.split(/\s+/).some(looksLikeAbsolutePath)) return undefined;
  return value;
}

function looksLikeAbsolutePath(value: string): boolean {
  const trimmed = value.replace(/^[()[\]{};,:'"]+|[()[\]{};,:'"]+$/g, "");
  return trimmed.startsWith("/") || trimmed.startsWith("\\") || /^[A-Za-z]:[\/\\]/.test(trimmed);
}

function safeCorrelationId(value: string | undefined): string | undefined {
  return value && value.length <= MAX_CORRELATION_ID_BYTES && /^[A-Za-z0-9_-]+$/.test(value) ? value : undefined;
}
