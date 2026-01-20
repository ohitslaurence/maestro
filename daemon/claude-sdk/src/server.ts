import { query } from "@anthropic-ai/claude-agent-sdk";

type EventPayload = {
  type: string;
  properties?: Record<string, unknown>;
};

type PartInput = {
  id?: string;
  type: string;
  text?: string;
  [key: string]: unknown;
};

type SessionRecord = {
  id: string;
  slug: string;
  projectID: string;
  directory: string;
  parentID?: string;
  title: string;
  version: number;
  time: {
    created: number;
    updated: number;
  };
};

type SessionState = {
  record: SessionRecord;
  resumeId: string | null;
  activeRun: {
    abortController: AbortController;
    assistantMessageId: string;
    startedAt: number;
  } | null;
};

const workspaceDir = process.env.MAESTRO_WORKSPACE_DIR ?? process.cwd();
const serverHost = process.env.MAESTRO_HOST ?? "127.0.0.1";
const requestedPort = Number(process.env.MAESTRO_PORT ?? "0") || 0;
const defaultModelId = process.env.MAESTRO_CLAUDE_MODEL ?? "claude-sonnet-4-20250514";
const defaultAgent = process.env.MAESTRO_CLAUDE_AGENT ?? "claude-sdk";
const defaultProvider = "anthropic";

class EventHub {
  private subscribers = new Set<(eventType: string, payload: EventPayload) => void>();

  subscribe(handler: (eventType: string, payload: EventPayload) => void) {
    this.subscribers.add(handler);
    return () => this.subscribers.delete(handler);
  }

  emit(eventType: string, payload: EventPayload) {
    for (const handler of this.subscribers) {
      handler(eventType, payload);
    }
  }
}

const sessions = new Map<string, SessionState>();
const events = new EventHub();

function emitEvent(type: string, properties?: Record<string, unknown>) {
  events.emit(type, { type, properties });
}

function jsonResponse(data: unknown, status = 200) {
  return new Response(JSON.stringify(data), {
    status,
    headers: {
      "Content-Type": "application/json",
    },
  });
}

function slugify(input: string) {
  return input
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/(^-|-$)+/g, "");
}

function createId(prefix: string) {
  return `${prefix}_${crypto.randomUUID()}`;
}

function buildSessionRecord(title?: string, parentID?: string): SessionRecord {
  const now = Date.now();
  const finalTitle = title && title.trim().length > 0 ? title.trim() : `Session ${sessions.size + 1}`;
  return {
    id: createId("session"),
    slug: slugify(finalTitle),
    projectID: "maestro",
    directory: workspaceDir,
    parentID,
    title: finalTitle,
    version: 1,
    time: {
      created: now,
      updated: now,
    },
  };
}

function createSseResponse(options: { wrapWithDirectory: boolean }) {
  let unsubscribe: (() => void) | null = null;
  const stream = new ReadableStream({
    start(controller) {
      const send = (eventType: string, payload: EventPayload) => {
        const data = options.wrapWithDirectory
          ? { directory: workspaceDir, payload }
          : payload;
        controller.enqueue(`event: ${eventType}\n`);
        controller.enqueue(`data: ${JSON.stringify(data)}\n\n`);
      };

      unsubscribe = events.subscribe((eventType, payload) => {
        send(eventType, payload);
      });
    },
    cancel() {
      if (unsubscribe) {
        unsubscribe();
      }
    },
  });

  return new Response(stream, {
    headers: {
      "Content-Type": "text/event-stream",
      "Cache-Control": "no-cache",
      Connection: "keep-alive",
    },
  });
}

function emitSessionStatus(sessionID: string, status: "idle" | "busy" | "retry") {
  emitEvent("session.status", {
    sessionID,
    status: {
      type: status,
    },
  });
}

function buildUserMessageInfo(options: {
  sessionID: string;
  messageID: string;
  agent: string;
  modelID: string;
  prompt: string;
  system?: string;
  variant?: string;
}) {
  return {
    id: options.messageID,
    sessionID: options.sessionID,
    role: "user",
    time: {
      created: Date.now(),
    },
    summary: {
      title: options.prompt,
      diffs: [],
    },
    agent: options.agent,
    model: {
      providerID: defaultProvider,
      modelID: options.modelID,
    },
    system: options.system,
    variant: options.variant,
  };
}

function buildAssistantMessageInfo(options: {
  sessionID: string;
  messageID: string;
  parentID: string;
  agent: string;
  modelID: string;
}) {
  return {
    id: options.messageID,
    sessionID: options.sessionID,
    role: "assistant",
    time: {
      created: Date.now(),
    },
    parentID: options.parentID,
    modelID: options.modelID,
    providerID: defaultProvider,
    mode: "default",
    agent: options.agent,
    path: {
      cwd: workspaceDir,
      root: workspaceDir,
    },
    cost: 0,
    tokens: {
      input: 0,
      output: 0,
      reasoning: 0,
      cache: {
        read: 0,
        write: 0,
      },
    },
  };
}

async function handleSessionMessage(req: Request, sessionId: string) {
  const session = sessions.get(sessionId);
  if (!session) {
    return jsonResponse({ error: "session_not_found" }, 404);
  }

  if (session.activeRun) {
    return jsonResponse({ error: "session_busy" }, 409);
  }

  let body: {
    messageID?: string;
    model?: string;
    agent?: string;
    noReply?: boolean;
    system?: string;
    variant?: string;
    parts?: PartInput[];
  } | null = null;

  try {
    body = (await req.json()) as typeof body;
  } catch {
    body = null;
  }

  if (!body || !Array.isArray(body.parts)) {
    return jsonResponse({ error: "invalid_request" }, 400);
  }

  const promptParts = body.parts.filter((part) => part.type === "text" && typeof part.text === "string");
  const prompt = promptParts.map((part) => part.text).join("\n").trim();

  if (!prompt) {
    return jsonResponse({ error: "empty_prompt" }, 400);
  }

  const modelID = body.model ?? defaultModelId;
  const agent = body.agent ?? defaultAgent;
  const userMessageId = body.messageID ?? createId("message");
  const assistantMessageId = createId("message");

  emitEvent("message.updated", {
    info: buildUserMessageInfo({
      sessionID: session.record.id,
      messageID: userMessageId,
      agent,
      modelID,
      prompt,
      system: body.system,
      variant: body.variant,
    }),
  });

  const assistantMessageInfo = buildAssistantMessageInfo({
    sessionID: session.record.id,
    messageID: assistantMessageId,
    parentID: userMessageId,
    agent,
    modelID,
  });

  emitEvent("message.updated", {
    info: assistantMessageInfo,
  });

  session.record.time.updated = Date.now();
  emitSessionStatus(session.record.id, "busy");

  const abortController = new AbortController();
  session.activeRun = {
    abortController,
    assistantMessageId,
    startedAt: Date.now(),
  };

  void runClaudeQuery({
    session,
    prompt,
    modelID,
    agent,
    assistantMessageInfo,
    abortController,
  });

  return jsonResponse({
    info: assistantMessageInfo,
    parts: [],
  });
}

async function runClaudeQuery(options: {
  session: SessionState;
  prompt: string;
  modelID: string;
  agent: string;
  assistantMessageInfo: ReturnType<typeof buildAssistantMessageInfo>;
  abortController: AbortController;
}) {
  const { session, prompt, modelID, agent, assistantMessageInfo, abortController } = options;
  let assistantText = "";

  try {
    const stream = query({
      prompt,
      options: {
        cwd: workspaceDir,
        resume: session.resumeId ?? undefined,
        abortController,
        model: modelID,
      },
    });

    for await (const message of stream) {
      if (message.type === "assistant") {
        const blocks = message.message?.content ?? [];
        for (const block of blocks) {
          if (block.type === "text" && typeof block.text === "string") {
            assistantText += block.text;
          }
        }
      }

      if (message.type === "result") {
        if (message.session_id) {
          session.resumeId = message.session_id;
        }
      }
    }

    if (assistantText.trim()) {
      emitEvent("message.part.updated", {
        part: {
          id: createId("part"),
          sessionID: session.record.id,
          messageID: assistantMessageInfo.id,
          type: "text",
          text: assistantText,
          time: {
            start: session.activeRun?.startedAt ?? Date.now(),
            end: Date.now(),
          },
        },
        delta: assistantText,
      });
    }

    emitEvent("message.updated", {
      info: {
        ...assistantMessageInfo,
        time: {
          created: assistantMessageInfo.time.created,
          completed: Date.now(),
        },
      },
    });

    emitEvent("session.idle", { sessionID: session.record.id });
  } catch (error) {
    emitEvent("session.error", {
      sessionID: session.record.id,
      error: error instanceof Error ? error.message : String(error),
    });
  } finally {
    session.activeRun = null;
    emitSessionStatus(session.record.id, "idle");
  }
}

const server = Bun.serve({
  hostname: serverHost,
  port: requestedPort,
  async fetch(req) {
    const url = new URL(req.url);
    const pathname = url.pathname;

    if (pathname === "/event" && req.method === "GET") {
      return createSseResponse({ wrapWithDirectory: false });
    }

    if (pathname === "/global/event" && req.method === "GET") {
      return createSseResponse({ wrapWithDirectory: true });
    }

    if (pathname === "/session" && req.method === "GET") {
      const start = Number(url.searchParams.get("start")) || 0;
      const limit = Number(url.searchParams.get("limit")) || 50;

      const result = Array.from(sessions.values())
        .map((session) => session.record)
        .filter((session) => session.time.updated >= start)
        .slice(0, limit);

      return jsonResponse(result);
    }

    if (pathname === "/session" && req.method === "POST") {
      let body: { parentID?: string; title?: string; permission?: string } | null = null;
      try {
        body = (await req.json()) as typeof body;
      } catch {
        body = null;
      }

      const record = buildSessionRecord(body?.title, body?.parentID);
      sessions.set(record.id, {
        record,
        resumeId: null,
        activeRun: null,
      });

      emitEvent("session.created", { info: record });

      return jsonResponse(record);
    }

    if (pathname.startsWith("/session/") && req.method === "GET") {
      const sessionId = pathname.split("/")[2];
      const session = sessions.get(sessionId);
      if (!session) {
        return jsonResponse({ error: "session_not_found" }, 404);
      }
      return jsonResponse(session.record);
    }

    if (pathname.startsWith("/session/") && pathname.endsWith("/message") && req.method === "POST") {
      const parts = pathname.split("/");
      const sessionId = parts[2];
      return handleSessionMessage(req, sessionId);
    }

    if (pathname.startsWith("/session/") && pathname.endsWith("/abort") && req.method === "POST") {
      const parts = pathname.split("/");
      const sessionId = parts[2];
      const session = sessions.get(sessionId);
      if (!session) {
        return jsonResponse({ error: "session_not_found" }, 404);
      }

      if (session.activeRun) {
        session.activeRun.abortController.abort();
        session.activeRun = null;
      }

      emitSessionStatus(session.record.id, "idle");
      return jsonResponse({ ok: true });
    }

    return jsonResponse({ error: "not_found" }, 404);
  },
});

// eslint-disable-next-line no-console
console.log(`Listening on http://${serverHost}:${server.port}`);
