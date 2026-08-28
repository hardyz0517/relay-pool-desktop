/** Versioned metadata owned by the single frontend IPC adapter. */
export type InteractionId = `int_${string}`;
export type IpcContextSessionId = `ctx_${string}`;

export type IpcRuntimeContextV1 = {
  contextSessionId: IpcContextSessionId;
  interactionId: InteractionId | null;
};

const TOKEN_HEX_LENGTH = 32;
const SESSION_PREFIX = "ctx_";
const INTERACTION_PREFIX = "int_";

let contextSessionId: IpcContextSessionId | null = null;
let activeInteractionId: InteractionId | null = null;

function isToken(value: string, prefix: string): boolean {
  return (
    value.length === prefix.length + TOKEN_HEX_LENGTH &&
    value.startsWith(prefix) &&
    /^[0-9a-f]+$/.test(value.slice(prefix.length))
  );
}

function randomToken(prefix: string): string {
  const bytes = new Uint8Array(TOKEN_HEX_LENGTH / 2);
  globalThis.crypto.getRandomValues(bytes);
  return `${prefix}${Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("")}`;
}

/** Called once by bootstrap after the backend grants its in-memory capability. */
export function configureRuntimeContextSession(value: string): void {
  if (!isToken(value, SESSION_PREFIX)) {
    throw new Error("invalid runtime context session");
  }
  contextSessionId = value as IpcContextSessionId;
  activeInteractionId = null;
}

export function clearRuntimeContextSession(): void {
  contextSessionId = null;
  activeInteractionId = null;
}

export function currentRuntimeContext(): IpcRuntimeContextV1 | null {
  if (!contextSessionId) {
    return null;
  }
  return {
    contextSessionId,
    interactionId: activeInteractionId,
  };
}

/**
 * Runs the synchronous dispatch portion of a user gesture with one anonymous
 * interaction id. The value is restored before an async continuation runs;
 * browser JavaScript has no portable async-local storage, and retaining a
 * mutable global across `await` would let overlapping gestures cross-wire.
 * Commands dispatched synchronously (including a Promise.all fan-out) carry
 * the id; later continuations must start a new explicit scope.
 */
export async function runUserInteraction<T>(callback: () => T | Promise<T>): Promise<T> {
  const previous = activeInteractionId;
  if (!previous) {
    activeInteractionId = randomToken(INTERACTION_PREFIX) as InteractionId;
  }
  let result: T | Promise<T>;
  try {
    result = callback();
  } finally {
    activeInteractionId = previous;
  }
  return await result;
}
