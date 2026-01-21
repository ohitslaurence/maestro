import {
  STREAM_SCHEMA_VERSION,
  type StreamEvent,
  type StreamEventType,
  type StreamPayloadMap,
} from "../../types/streaming";

type StreamEventInput<T extends StreamEventType> = {
  sessionId: string;
  harness: string;
  streamId: string;
  seq: number;
  type: T;
  payload: StreamPayloadMap[T];
  messageId?: string;
  parentMessageId?: string;
  provider?: string;
};

function generateEventId(): string {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return `evt_${crypto.randomUUID()}`;
  }
  const random = Math.random().toString(16).slice(2);
  return `evt_${Date.now()}_${random}`;
}

export function createStreamEvent<T extends StreamEventType>(
  input: StreamEventInput<T>,
): StreamEvent<T> {
  const event: StreamEvent<T> = {
    schemaVersion: STREAM_SCHEMA_VERSION,
    eventId: generateEventId(),
    sessionId: input.sessionId,
    harness: input.harness,
    provider: input.provider,
    streamId: input.streamId,
    seq: input.seq,
    timestampMs: Date.now(),
    type: input.type,
    payload: input.payload,
    messageId: input.messageId,
    parentMessageId: input.parentMessageId,
  };

  return event;
}
