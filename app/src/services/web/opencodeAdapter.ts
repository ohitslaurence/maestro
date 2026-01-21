import type {
  StreamEvent,
  ToolCallStatus,
  ToolCallCompletedPayload,
  ToolCallDeltaPayload,
  TextDeltaPayload,
  ThinkingDeltaPayload,
  StatusPayload,
  ErrorPayload,
} from "../../types/streaming";
import { createStreamEvent } from "./streamEvent";

type OpenCodeDaemonEvent = {
  workspaceId: string;
  eventType: string;
  event: OpenCodeInnerEvent;
};

type OpenCodeInnerEvent = {
  type: string;
  properties?: Record<string, unknown>;
};

type PartTime = {
  start?: number;
  end?: number;
};

type PartData = {
  id?: string;
  messageID?: string;
  messageId?: string;
  sessionID?: string;
  sessionId?: string;
  type?: string;
  text?: string;
  content?: string;
  output?: string;
  tool?: string;
  toolCallID?: string;
  toolCallId?: string;
  input?: Record<string, unknown> | null;
  error?: string;
  time?: PartTime;
};

type SessionStatusProps = {
  sessionID?: string;
  sessionId?: string;
  status?: { type?: string };
};

type SessionErrorProps = {
  sessionID?: string;
  sessionId?: string;
  error?: string;
};

type WorkspaceStreamState = {
  streams: Map<string, StreamState>;
  completedTools: Set<string>;
};

class StreamState {
  streamId: string;
  seq: number;

  constructor(messageId: string) {
    this.streamId = `stream_${messageId}`;
    this.seq = 0;
  }

  nextSeq() {
    const current = this.seq;
    this.seq += 1;
    return current;
  }
}

function getRecord(value: unknown): Record<string, unknown> | null {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    return null;
  }
  return value as Record<string, unknown>;
}

function getString(value: unknown): string | undefined {
  return typeof value === "string" ? value : undefined;
}

function getPartData(value: unknown): PartData | null {
  return getRecord(value) as PartData | null;
}

function getSessionId(part: PartData | null, fallback: string) {
  return part?.sessionID ?? part?.sessionId ?? fallback;
}

function getMessageId(part: PartData | null): string | null {
  return part?.messageID ?? part?.messageId ?? null;
}

function getToolCallId(part: PartData | null): string | null {
  return part?.toolCallID ?? part?.toolCallId ?? part?.id ?? null;
}

function getToolStatus(error?: string | null): ToolCallStatus {
  return error ? "failed" : "completed";
}

function parseSessionStatus(props: Record<string, unknown>): SessionStatusProps {
  return props as SessionStatusProps;
}

function parseSessionError(props: Record<string, unknown>): SessionErrorProps {
  return props as SessionErrorProps;
}

export class OpenCodeAdapter {
  private workspaces = new Map<string, WorkspaceStreamState>();

  adapt(raw: unknown): StreamEvent[] | null {
    const event = getRecord(raw) as OpenCodeDaemonEvent | null;
    if (!event || typeof event.workspaceId !== "string" || !event.event) {
      return null;
    }

    const inner = event.event;
    if (typeof inner.type !== "string") {
      return null;
    }

    const props = inner.properties ? getRecord(inner.properties) : null;

    switch (inner.type) {
      case "message.part.updated":
        return this.adaptPartUpdated(event.workspaceId, props);
      case "session.status":
        return this.adaptSessionStatus(event.workspaceId, props);
      case "session.error":
        return this.adaptSessionError(event.workspaceId, props);
      case "session.idle":
        return this.adaptSessionIdle(event.workspaceId, props);
      default:
        return null;
    }
  }

  private getWorkspace(workspaceId: string): WorkspaceStreamState {
    const existing = this.workspaces.get(workspaceId);
    if (existing) {
      return existing;
    }
    const next: WorkspaceStreamState = {
      streams: new Map<string, StreamState>(),
      completedTools: new Set<string>(),
    };
    this.workspaces.set(workspaceId, next);
    return next;
  }

  private getStream(workspaceId: string, messageId: string): StreamState {
    const workspace = this.getWorkspace(workspaceId);
    const existing = workspace.streams.get(messageId);
    if (existing) {
      return existing;
    }
    const next = new StreamState(messageId);
    workspace.streams.set(messageId, next);
    return next;
  }

  private markToolCompleted(workspaceId: string, callId: string): boolean {
    const workspace = this.getWorkspace(workspaceId);
    if (workspace.completedTools.has(callId)) {
      return false;
    }
    workspace.completedTools.add(callId);
    return true;
  }

  private adaptPartUpdated(
    workspaceId: string,
    props: Record<string, unknown> | null,
  ): StreamEvent[] | null {
    if (!props) {
      return null;
    }

    const part = getPartData(props.part);
    const messageId = getMessageId(part);
    if (!part || !messageId) {
      return null;
    }

    const stream = this.getStream(workspaceId, messageId);
    const seq = stream.nextSeq();
    const sessionId = getSessionId(part, workspaceId);
    const delta = getString(props.delta);
    const partType = part.type ?? "";

    if (partType === "text") {
      const payload: TextDeltaPayload = {
        text: delta ?? part.text ?? "",
        role: "assistant",
      };
      return [
        createStreamEvent({
          sessionId,
          harness: "open_code",
          streamId: stream.streamId,
          seq,
          type: "text_delta",
          payload,
          messageId,
        }),
      ];
    }

    if (partType === "reasoning") {
      const payload: ThinkingDeltaPayload = {
        text: delta ?? part.content ?? part.text ?? "",
      };
      return [
        createStreamEvent({
          sessionId,
          harness: "open_code",
          streamId: stream.streamId,
          seq,
          type: "thinking_delta",
          payload,
          messageId,
        }),
      ];
    }

    if (partType === "tool") {
      const callId = getToolCallId(part);
      if (!callId) {
        return null;
      }

      const toolName = part.tool ?? "unknown";
      const isCompleted =
        part.output !== undefined ||
        typeof part.time?.end === "number" ||
        !!part.error;

      if (isCompleted) {
        if (!this.markToolCompleted(workspaceId, callId)) {
          return null;
        }

        const payload: ToolCallCompletedPayload = {
          callId,
          toolName,
          arguments: part.input ?? {},
          output: part.output ?? "",
          status: getToolStatus(part.error),
          errorMessage: part.error ?? undefined,
        };
        return [
          createStreamEvent({
            sessionId,
            harness: "open_code",
            streamId: stream.streamId,
            seq,
            type: "tool_call_completed",
            payload,
            messageId,
          }),
        ];
      }

      const payload: ToolCallDeltaPayload = {
        callId,
        toolName,
        argumentsDelta: delta ?? "",
      };
      return [
        createStreamEvent({
          sessionId,
          harness: "open_code",
          streamId: stream.streamId,
          seq,
          type: "tool_call_delta",
          payload,
          messageId,
        }),
      ];
    }

    return null;
  }

  private adaptSessionStatus(
    workspaceId: string,
    props: Record<string, unknown> | null,
  ): StreamEvent[] | null {
    if (!props) {
      return null;
    }

    const statusProps = parseSessionStatus(props);
    const sessionId = statusProps.sessionID ?? statusProps.sessionId ?? workspaceId;
    const statusType = statusProps.status?.type;
    if (!statusType) {
      return null;
    }

    const state = statusType === "busy" ? "processing" : statusType === "idle" ? "idle" : null;
    if (!state) {
      return null;
    }

    const payload: StatusPayload = { state };
    return [
      createStreamEvent({
        sessionId,
        harness: "open_code",
        streamId: `status_${sessionId}`,
        seq: 0,
        type: "status",
        payload,
      }),
    ];
  }

  private adaptSessionError(
    workspaceId: string,
    props: Record<string, unknown> | null,
  ): StreamEvent[] | null {
    if (!props) {
      return null;
    }

    const errorProps = parseSessionError(props);
    const sessionId = errorProps.sessionID ?? errorProps.sessionId ?? workspaceId;
    const message = errorProps.error ?? "Unknown error";

    const payload: ErrorPayload = {
      code: "provider_error",
      message,
      recoverable: true,
    };

    return [
      createStreamEvent({
        sessionId,
        harness: "open_code",
        streamId: `error_${sessionId}`,
        seq: 0,
        type: "error",
        payload,
      }),
    ];
  }

  private adaptSessionIdle(
    workspaceId: string,
    props: Record<string, unknown> | null,
  ): StreamEvent[] | null {
    const sessionId = getString(props?.sessionID) ?? getString(props?.sessionId) ?? workspaceId;
    const payload: StatusPayload = { state: "idle" };

    return [
      createStreamEvent({
        sessionId,
        harness: "open_code",
        streamId: `status_${sessionId}`,
        seq: 0,
        type: "status",
        payload,
      }),
    ];
  }
}
