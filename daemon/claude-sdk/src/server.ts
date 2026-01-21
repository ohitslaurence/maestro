import { query } from "@anthropic-ai/claude-agent-sdk";
import { Database } from "bun:sqlite";
import { createHash } from "node:crypto";
import { mkdirSync } from "node:fs";
import { homedir } from "node:os";
import path from "node:path";

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
    assistantPartId: string;
    startedAt: number;
  } | null;
};

const workspaceDir = process.env.MAESTRO_WORKSPACE_DIR ?? process.cwd();
const serverHost = process.env.MAESTRO_HOST ?? "127.0.0.1";
const requestedPort = Number(process.env.MAESTRO_PORT ?? "0") || 0;
const defaultModelId = process.env.MAESTRO_CLAUDE_MODEL ?? "claude-sonnet-4-20250514";
const defaultAgent = process.env.MAESTRO_CLAUDE_AGENT ?? "claude-sdk";
const defaultProvider = "anthropic";
const dataRoot = process.env.MAESTRO_DATA_DIR ?? path.join(homedir(), ".maestro");
const claudeRoot = path.join(dataRoot, "claude");
const workspaceHash = createHash("sha256").update(workspaceDir).digest("hex");
const workspaceDataDir = path.join(claudeRoot, workspaceHash);
mkdirSync(workspaceDataDir, { recursive: true });
const dbPath = path.join(workspaceDataDir, "sessions.sqlite");
const db = new Database(dbPath);

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

const statements = initStorage();
loadSessions();

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

function initStorage() {
  db.exec(`
    CREATE TABLE IF NOT EXISTS sessions (
      id TEXT PRIMARY KEY,
      slug TEXT NOT NULL,
      project_id TEXT NOT NULL,
      directory TEXT NOT NULL,
      parent_id TEXT,
      title TEXT NOT NULL,
      version INTEGER NOT NULL,
      created_at INTEGER NOT NULL,
      updated_at INTEGER NOT NULL,
      resume_id TEXT
    );
    CREATE TABLE IF NOT EXISTS messages (
      id TEXT PRIMARY KEY,
      session_id TEXT NOT NULL,
      role TEXT NOT NULL,
      created_at INTEGER NOT NULL,
      completed_at INTEGER,
      parent_id TEXT,
      model_id TEXT,
      provider_id TEXT,
      agent TEXT,
      mode TEXT,
      system TEXT,
      variant TEXT,
      summary_title TEXT,
      summary_body TEXT,
      cost REAL,
      tokens_input INTEGER,
      tokens_output INTEGER,
      tokens_reasoning INTEGER,
      tokens_cache_read INTEGER,
      tokens_cache_write INTEGER,
      error_name TEXT,
      error_payload TEXT
    );
    CREATE TABLE IF NOT EXISTS parts (
      id TEXT PRIMARY KEY,
      session_id TEXT NOT NULL,
      message_id TEXT NOT NULL,
      type TEXT NOT NULL,
      text TEXT,
      content TEXT,
      tool TEXT,
      call_id TEXT,
      title TEXT,
      input_json TEXT,
      output TEXT,
      error TEXT,
      hash TEXT,
      files_json TEXT,
      time_start INTEGER,
      time_end INTEGER,
      metadata_json TEXT
    );
  `);

  return {
    insertSession: db.prepare(`
      INSERT INTO sessions (
        id, slug, project_id, directory, parent_id, title, version, created_at, updated_at, resume_id
      ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
      ON CONFLICT(id) DO UPDATE SET
        slug=excluded.slug,
        project_id=excluded.project_id,
        directory=excluded.directory,
        parent_id=excluded.parent_id,
        title=excluded.title,
        version=excluded.version,
        created_at=excluded.created_at,
        updated_at=excluded.updated_at,
        resume_id=excluded.resume_id
    `),
    touchSession: db.prepare("UPDATE sessions SET updated_at=? WHERE id=?"),
    updateSessionResume: db.prepare("UPDATE sessions SET resume_id=?, updated_at=? WHERE id=?"),
    selectSessions: db.prepare(
      "SELECT id, slug, project_id, directory, parent_id, title, version, created_at, updated_at, resume_id FROM sessions"
    ),
    selectSessionById: db.prepare(
      "SELECT id, slug, project_id, directory, parent_id, title, version, created_at, updated_at, resume_id FROM sessions WHERE id=?"
    ),
    insertMessage: db.prepare(`
      INSERT INTO messages (
        id, session_id, role, created_at, completed_at, parent_id, model_id, provider_id, agent, mode,
        system, variant, summary_title, summary_body, cost, tokens_input, tokens_output, tokens_reasoning,
        tokens_cache_read, tokens_cache_write, error_name, error_payload
      ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
      ON CONFLICT(id) DO UPDATE SET
        session_id=excluded.session_id,
        role=excluded.role,
        created_at=excluded.created_at,
        completed_at=excluded.completed_at,
        parent_id=excluded.parent_id,
        model_id=excluded.model_id,
        provider_id=excluded.provider_id,
        agent=excluded.agent,
        mode=excluded.mode,
        system=excluded.system,
        variant=excluded.variant,
        summary_title=excluded.summary_title,
        summary_body=excluded.summary_body,
        cost=excluded.cost,
        tokens_input=excluded.tokens_input,
        tokens_output=excluded.tokens_output,
        tokens_reasoning=excluded.tokens_reasoning,
        tokens_cache_read=excluded.tokens_cache_read,
        tokens_cache_write=excluded.tokens_cache_write,
        error_name=excluded.error_name,
        error_payload=excluded.error_payload
    `),
    updateMessageCompletion: db.prepare("UPDATE messages SET completed_at=? WHERE id=?"),
    updateMessageError: db.prepare(
      "UPDATE messages SET error_name=?, error_payload=?, completed_at=? WHERE id=?"
    ),
    insertPart: db.prepare(`
      INSERT INTO parts (
        id, session_id, message_id, type, text, content, tool, call_id, title, input_json, output,
        error, hash, files_json, time_start, time_end, metadata_json
      ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
      ON CONFLICT(id) DO UPDATE SET
        session_id=excluded.session_id,
        message_id=excluded.message_id,
        type=excluded.type,
        text=excluded.text,
        content=excluded.content,
        tool=excluded.tool,
        call_id=excluded.call_id,
        title=excluded.title,
        input_json=excluded.input_json,
        output=excluded.output,
        error=excluded.error,
        hash=excluded.hash,
        files_json=excluded.files_json,
        time_start=excluded.time_start,
        time_end=excluded.time_end,
        metadata_json=excluded.metadata_json
    `),
  };
}

function loadSessions() {
  const rows = statements.selectSessions.all() as Array<{
    id: string;
    slug: string;
    project_id: string;
    directory: string;
    parent_id: string | null;
    title: string;
    version: number;
    created_at: number;
    updated_at: number;
    resume_id: string | null;
  }>;

  for (const row of rows) {
    const record: SessionRecord = {
      id: row.id,
      slug: row.slug,
      projectID: row.project_id,
      directory: row.directory,
      parentID: row.parent_id ?? undefined,
      title: row.title,
      version: row.version,
      time: {
        created: row.created_at,
        updated: row.updated_at,
      },
    };

    sessions.set(record.id, {
      record,
      resumeId: row.resume_id,
      activeRun: null,
    });
  }
}

function persistSession(record: SessionRecord, resumeId: string | null) {
  statements.insertSession.run(
    record.id,
    record.slug,
    record.projectID,
    record.directory,
    record.parentID ?? null,
    record.title,
    record.version,
    record.time.created,
    record.time.updated,
    resumeId,
  );
}

function touchSession(session: SessionState) {
  session.record.time.updated = Date.now();
  statements.touchSession.run(session.record.time.updated, session.record.id);
}

function updateSessionResume(session: SessionState) {
  const updatedAt = Date.now();
  session.record.time.updated = updatedAt;
  statements.updateSessionResume.run(session.resumeId, updatedAt, session.record.id);
}

function serializeJson(value: unknown) {
  if (value === undefined) {
    return null;
  }
  return JSON.stringify(value);
}

function persistUserMessage(info: ReturnType<typeof buildUserMessageInfo>) {
  statements.insertMessage.run(
    info.id,
    info.sessionID,
    info.role,
    info.time.created,
    null,
    null,
    info.model.modelID,
    info.model.providerID,
    info.agent,
    null,
    info.system ?? null,
    info.variant ?? null,
    info.summary?.title ?? null,
    info.summary?.body ?? null,
    0,
    0,
    0,
    0,
    0,
    0,
    null,
    null,
  );
}

function persistAssistantMessage(info: ReturnType<typeof buildAssistantMessageInfo>) {
  statements.insertMessage.run(
    info.id,
    info.sessionID,
    info.role,
    info.time.created,
    null,
    info.parentID,
    info.modelID,
    info.providerID,
    info.agent,
    info.mode,
    null,
    null,
    null,
    null,
    info.cost,
    info.tokens.input,
    info.tokens.output,
    info.tokens.reasoning,
    info.tokens.cache.read,
    info.tokens.cache.write,
    null,
    null,
  );
}

function persistTextPart(options: {
  id: string;
  sessionID: string;
  messageID: string;
  text: string;
  timeStart: number;
  timeEnd: number;
}) {
  statements.insertPart.run(
    options.id,
    options.sessionID,
    options.messageID,
    "text",
    options.text,
    null,
    null,
    null,
    null,
    null,
    null,
    null,
    null,
    null,
    options.timeStart,
    options.timeEnd,
    null,
  );
}

function extractTextFromBlocks(blocks: unknown) {
  let fullText = "";
  let deltaText = "";
  let hasDelta = false;

  if (!Array.isArray(blocks)) {
    return { fullText, deltaText, hasDelta };
  }

  for (const block of blocks) {
    if (!block || typeof block !== "object") {
      continue;
    }

    const typedBlock = block as { type?: unknown; text?: unknown };
    if (typedBlock.type === "text" && typeof typedBlock.text === "string") {
      fullText += typedBlock.text;
    }

    if (typedBlock.type === "text_delta" && typeof typedBlock.text === "string") {
      deltaText += typedBlock.text;
      hasDelta = true;
    }
  }

  return { fullText, deltaText, hasDelta };
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

  const userMessageInfo = buildUserMessageInfo({
    sessionID: session.record.id,
    messageID: userMessageId,
    agent,
    modelID,
    prompt,
    system: body.system,
    variant: body.variant,
  });
  persistUserMessage(userMessageInfo);
  emitEvent("message.updated", { info: userMessageInfo });

  const assistantMessageInfo = buildAssistantMessageInfo({
    sessionID: session.record.id,
    messageID: assistantMessageId,
    parentID: userMessageId,
    agent,
    modelID,
  });

  persistAssistantMessage(assistantMessageInfo);
  emitEvent("message.updated", { info: assistantMessageInfo });

  touchSession(session);
  emitSessionStatus(session.record.id, "busy");

  const abortController = new AbortController();
  session.activeRun = {
    abortController,
    assistantMessageId,
    assistantPartId: createId("part"),
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
  const partId = session.activeRun?.assistantPartId ?? createId("part");
  const timeStart = session.activeRun?.startedAt ?? Date.now();

  try {
    const stream = query({
      prompt,
      options: {
        cwd: workspaceDir,
        resume: session.resumeId ?? undefined,
        abortController,
        model: modelID,
        includePartialMessages: true,
      },
    });

    for await (const message of stream) {
      if (message.type === "assistant") {
        const blocks = message.message?.content;
        const { fullText, deltaText, hasDelta } = extractTextFromBlocks(blocks);
        let nextText = assistantText;
        let delta = "";

        if (hasDelta && deltaText) {
          delta = deltaText;
          nextText = assistantText + deltaText;
        } else if (fullText) {
          if (assistantText && fullText.startsWith(assistantText)) {
            delta = fullText.slice(assistantText.length);
            nextText = fullText;
          } else if (assistantText) {
            delta = fullText;
            nextText = assistantText + fullText;
          } else {
            delta = fullText;
            nextText = fullText;
          }
        } else if (typeof (message as { delta?: { text?: unknown } }).delta?.text === "string") {
          delta = (message as { delta?: { text?: string } }).delta?.text ?? "";
          nextText = assistantText + delta;
        }

        if (delta) {
          assistantText = nextText;
          const timeEnd = Date.now();
          persistTextPart({
            id: partId,
            sessionID: session.record.id,
            messageID: assistantMessageInfo.id,
            text: assistantText,
            timeStart,
            timeEnd,
          });
          emitEvent("message.part.updated", {
            part: {
              id: partId,
              sessionID: session.record.id,
              messageID: assistantMessageInfo.id,
              type: "text",
              text: assistantText,
              time: {
                start: timeStart,
                end: timeEnd,
              },
            },
            delta,
          });
        }
      }

      if (message.type === "result") {
        if (message.session_id) {
          session.resumeId = message.session_id;
        }
      }
    }

    const completedAt = Date.now();
    statements.updateMessageCompletion.run(completedAt, assistantMessageInfo.id);
    emitEvent("message.updated", {
      info: {
        ...assistantMessageInfo,
        time: {
          created: assistantMessageInfo.time.created,
          completed: completedAt,
        },
      },
    });

    emitEvent("session.idle", { sessionID: session.record.id });
  } catch (error) {
    const completedAt = Date.now();
    statements.updateMessageError.run(
      "UnknownError",
      serializeJson({ message: error instanceof Error ? error.message : String(error) }),
      completedAt,
      assistantMessageInfo.id,
    );
    emitEvent("session.error", {
      sessionID: session.record.id,
      error: error instanceof Error ? error.message : String(error),
    });
  } finally {
    session.activeRun = null;
    emitSessionStatus(session.record.id, "idle");
    updateSessionResume(session);
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
        .sort((a, b) => b.time.updated - a.time.updated)
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

      persistSession(record, null);

      emitEvent("session.created", { info: record });

      return jsonResponse(record);
    }

    if (pathname.startsWith("/session/") && req.method === "GET") {
      const sessionId = pathname.split("/")[2];
      const session = sessions.get(sessionId);
      if (!session) {
        const row = statements.selectSessionById.get(sessionId) as
          | {
              id: string;
              slug: string;
              project_id: string;
              directory: string;
              parent_id: string | null;
              title: string;
              version: number;
              created_at: number;
              updated_at: number;
              resume_id: string | null;
            }
          | undefined;
        if (!row) {
          return jsonResponse({ error: "session_not_found" }, 404);
        }
        return jsonResponse({
          id: row.id,
          slug: row.slug,
          projectID: row.project_id,
          directory: row.directory,
          parentID: row.parent_id ?? undefined,
          title: row.title,
          version: row.version,
          time: {
            created: row.created_at,
            updated: row.updated_at,
          },
        });
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
        const completedAt = Date.now();
        statements.updateMessageError.run(
          "MessageAbortedError",
          serializeJson({ message: "aborted" }),
          completedAt,
          session.activeRun.assistantMessageId,
        );
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
