import { Channel } from "@tauri-apps/api/core";

import { invokeCommand } from "@/lib/bridge/generated";
import type { StationKeyConnectivityInputDto } from "@/lib/bridge/generated";
import type {
  StationKeyConnectivityTestEvent,
  StationKeyConnectivityTestEventEnvelope,
  StationKeyConnectivityTestResult,
} from "@/lib/types/stationKeys";

export const STATION_KEY_CONNECTIVITY_EVENT_SCHEMA_VERSION = 1;

export class IncompatibleStreamingEventError extends Error {
  readonly code = "incompatible_streaming_event";

  constructor(message: string) {
    super(message);
    this.name = "IncompatibleStreamingEventError";
  }
}

export type StreamingSubscription = {
  readonly cancelCapability: "detach_only";
  close(): void;
};

type ChannelLike<Event> = {
  onmessage?: (event: Event) => void;
};

type ChannelConstructor<Event> = new () => ChannelLike<Event>;

type StationKeyConnectivityInvoker = (args: {
  input: StationKeyConnectivityInputDto;
  progress: ChannelLike<unknown>;
}) => Promise<StationKeyConnectivityTestResult>;

type StreamState = {
  runId: string | null;
  nextSequence: number;
  terminalSeen: boolean;
};

export type StationKeyConnectivityStreamOptions = {
  onEvent?: (event: StationKeyConnectivityTestEvent) => void;
  ChannelConstructor?: ChannelConstructor<unknown>;
  invoke?: StationKeyConnectivityInvoker;
};

export function openStationKeyConnectivityStream(
  input: StationKeyConnectivityInputDto,
  options: StationKeyConnectivityStreamOptions = {},
) {
  const ChannelConstructor = options.ChannelConstructor ?? Channel;
  const invoke = options.invoke ?? invokeStationKeyConnectivityCommand;
  const state: StreamState = { runId: null, nextSequence: 0, terminalSeen: false };
  let detached = false;
  let settled = false;
  let rejectStream: (error: unknown) => void = () => undefined;
  const progress = new ChannelConstructor();
  const subscription: StreamingSubscription = {
    cancelCapability: "detach_only",
    close() {
      detached = true;
    },
  };

  progress.onmessage = (rawEvent) => {
    if (detached || settled) {
      return;
    }
    try {
      const envelope = validateStationKeyConnectivityEvent(rawEvent, state);
      options.onEvent?.(envelope.event);
    } catch (error) {
      settled = true;
      detached = true;
      rejectStream(error);
    }
  };

  const promise = new Promise<StationKeyConnectivityTestResult>((resolve, reject) => {
    rejectStream = reject;
    invoke({ input, progress })
      .then((result) => {
        if (settled) {
          return;
        }
        if (!state.terminalSeen) {
          settled = true;
          reject(new IncompatibleStreamingEventError("Streaming command resolved without a terminal event."));
          return;
        }
        settled = true;
        resolve(result);
      })
      .catch((error) => {
        if (!settled) {
          settled = true;
          reject(error);
        }
      });
  });

  return { promise, subscription };
}

export function invokeStationKeyConnectivityStream(
  input: StationKeyConnectivityInputDto,
  options: Omit<StationKeyConnectivityStreamOptions, "invoke" | "ChannelConstructor"> = {},
) {
  return openStationKeyConnectivityStream(input, options).promise;
}

function invokeStationKeyConnectivityCommand(args: {
  input: StationKeyConnectivityInputDto;
  progress: ChannelLike<unknown>;
}) {
  return invokeCommand<StationKeyConnectivityTestResult>("test_station_key_connectivity", args);
}

export function validateStationKeyConnectivityEvent(
  rawEvent: unknown,
  state: StreamState = { runId: null, nextSequence: 0, terminalSeen: false },
): StationKeyConnectivityTestEventEnvelope {
  const event = asRecord(rawEvent, "Streaming event must be an object.");
  if (event.schemaVersion !== STATION_KEY_CONNECTIVITY_EVENT_SCHEMA_VERSION) {
    throw new IncompatibleStreamingEventError("Streaming event schema version is incompatible.");
  }
  const runId = readString(event, "runId");
  const sequence = readSequence(event);
  const terminal = readBoolean(event, "terminal");
  if (event.cancelCapability !== "detach_only") {
    throw new IncompatibleStreamingEventError("Streaming event cancellation capability is incompatible.");
  }
  if (state.terminalSeen) {
    throw new IncompatibleStreamingEventError("Streaming event arrived after the terminal event.");
  }
  if (state.runId === null) {
    state.runId = runId;
  } else if (state.runId !== runId) {
    throw new IncompatibleStreamingEventError("Streaming event run id changed.");
  }
  if (sequence !== state.nextSequence) {
    throw new IncompatibleStreamingEventError("Streaming event sequence is not contiguous.");
  }
  const payload = validatePayload(event.event, terminal);
  state.nextSequence += 1;
  if (terminal) {
    state.terminalSeen = true;
  }
  return {
    schemaVersion: STATION_KEY_CONNECTIVITY_EVENT_SCHEMA_VERSION,
    runId,
    sequence,
    terminal,
    cancelCapability: "detach_only",
    event: payload,
  };
}

function validatePayload(rawPayload: unknown, terminal: boolean): StationKeyConnectivityTestEvent {
  const payload = asRecord(rawPayload, "Streaming event payload must be an object.");
  const type = readString(payload, "type");
  if (type === "attemptStarted") {
    rejectTerminalPayload(type, terminal);
    return {
      type,
      model: readString(payload, "model"),
      protocol: readString(payload, "protocol"),
    };
  }
  if (type === "delta") {
    rejectTerminalPayload(type, terminal);
    return { type, text: readString(payload, "text") };
  }
  if (type === "fallback") {
    rejectTerminalPayload(type, terminal);
    return { type, reason: readString(payload, "reason") };
  }
  if (type === "completed") {
    requireTerminalPayload(type, terminal);
    return { type, ok: readBoolean(payload, "ok") };
  }
  if (type === "failed") {
    requireTerminalPayload(type, terminal);
    return { type, message: readString(payload, "message") };
  }
  throw new IncompatibleStreamingEventError("Streaming event payload type is incompatible.");
}

function rejectTerminalPayload(type: string, terminal: boolean) {
  if (terminal) {
    throw new IncompatibleStreamingEventError(`Streaming ${type} payload cannot be terminal.`);
  }
}

function requireTerminalPayload(type: string, terminal: boolean) {
  if (!terminal) {
    throw new IncompatibleStreamingEventError(`Streaming ${type} payload must be terminal.`);
  }
}

function asRecord(value: unknown, message: string): Record<string, unknown> {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new IncompatibleStreamingEventError(message);
  }
  return value as Record<string, unknown>;
}

function readString(record: Record<string, unknown>, field: string) {
  const value = record[field];
  if (typeof value !== "string" || value.length === 0) {
    throw new IncompatibleStreamingEventError(`Streaming event field ${field} is invalid.`);
  }
  return value;
}

function readBoolean(record: Record<string, unknown>, field: string) {
  const value = record[field];
  if (typeof value !== "boolean") {
    throw new IncompatibleStreamingEventError(`Streaming event field ${field} is invalid.`);
  }
  return value;
}

function readSequence(record: Record<string, unknown>) {
  const value = record.sequence;
  if (typeof value !== "number" || !Number.isSafeInteger(value) || value < 0) {
    throw new IncompatibleStreamingEventError("Streaming event sequence is invalid.");
  }
  return value;
}
