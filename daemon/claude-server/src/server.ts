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

type PermissionReply = "once" | "always" | "reject";

type ModelInfo = {
  value: string;
  displayName: string;
  description: string;
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
  maxThinkingTokens?: number;
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
const defaultPermissionMode = process.env.MAESTRO_CLAUDE_PERMISSION_MODE;
const defaultSettingSources = ["user", "project", "local"] as const;
const defaultSystemPrompt = { type: "preset", preset: "claude_code" } as const;
const defaultTools = { type: "preset", preset: "claude_code" } as const;
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
const pendingPermissionRequests = new Map<
  string,
  {
    sessionID: string;
    permission: string;
    patterns: string[];
    metadata: Record<string, unknown>;
    createdAt: number;
    resolve: (reply: PermissionReply) => void;
    abort: () => void;
  }
>();
const persistentPermissionReplies = new Map<string, Set<string>>();

// Models cache (5-minute TTL per spec §4)
const MODELS_CACHE_TTL_MS = 5 * 60 * 1000;
const FALLBACK_MODELS: ModelInfo[] = [
  { value: "claude-sonnet-4-20250514", displayName: "Claude Sonnet 4", description: "Fast and capable" },
  { value: "claude-opus-4-20250514", displayName: "Claude Opus 4", description: "Most intelligent" },
  { value: "claude-haiku-3-5-20241022", displayName: "Claude Haiku 3.5", description: "Fastest" },
];
let modelsCache: { models: ModelInfo[]; fetchedAt: number } | null = null;

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

function permissionKey(permission: string, patterns: string[]) {
  const normalizedPatterns = [...patterns].map((pattern) => pattern.trim()).filter(Boolean).sort();
  return `${permission}:${normalizedPatterns.join(",")}`;
}

function getModels(): ModelInfo[] {
  const now = Date.now();
  if (modelsCache && now - modelsCache.fetchedAt < MODELS_CACHE_TTL_MS) {
    console.log("[models] cache hit");
    return modelsCache.models;
  }
  console.log("[models] cache miss, returning fallback");
  return FALLBACK_MODELS;
}

function updateModelsCache(models: ModelInfo[]) {
  modelsCache = { models, fetchedAt: Date.now() };
  console.log("[models] cache updated");
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
      resume_id TEXT,
      max_thinking_tokens INTEGER
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
        id, slug, project_id, directory, parent_id, title, version, created_at, updated_at, resume_id, max_thinking_tokens
      ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
      ON CONFLICT(id) DO UPDATE SET
        slug=excluded.slug,
        project_id=excluded.project_id,
        directory=excluded.directory,
        parent_id=excluded.parent_id,
        title=excluded.title,
        version=excluded.version,
        created_at=excluded.created_at,
        updated_at=excluded.updated_at,
        resume_id=excluded.resume_id,
        max_thinking_tokens=excluded.max_thinking_tokens
    `),
    touchSession: db.prepare("UPDATE sessions SET updated_at=? WHERE id=?"),
    updateSessionResume: db.prepare("UPDATE sessions SET resume_id=?, updated_at=? WHERE id=?"),
    selectSessions: db.prepare(
      "SELECT id, slug, project_id, directory, parent_id, title, version, created_at, updated_at, resume_id, max_thinking_tokens FROM sessions"
    ),
    selectSessionById: db.prepare(
      "SELECT id, slug, project_id, directory, parent_id, title, version, created_at, updated_at, resume_id, max_thinking_tokens FROM sessions WHERE id=?"
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
    updateMessageUsage: db.prepare(
      "UPDATE messages SET cost=?, tokens_input=?, tokens_output=?, tokens_reasoning=?, tokens_cache_read=?, tokens_cache_write=? WHERE id=?"
    ),
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
    max_thinking_tokens: number | null;
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
      maxThinkingTokens: row.max_thinking_tokens ?? undefined,
      activeRun: null,
    });
  }
}

function persistSession(record: SessionRecord, resumeId: string | null, maxThinkingTokens?: number) {
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
    maxThinkingTokens ?? null,
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

function getPersistentPermissionReplies(sessionID: string) {
  const existing = persistentPermissionReplies.get(sessionID);
  if (existing) {
    return existing;
  }
  const created = new Set<string>();
  persistentPermissionReplies.set(sessionID, created);
  return created;
}

function waitForPermissionReply(options: {
  requestID: string;
  sessionID: string;
  permission: string;
  patterns: string[];
  metadata: Record<string, unknown>;
  abortSignal: AbortSignal;
}) {
  if (options.abortSignal.aborted) {
    return Promise.resolve<PermissionReply>("reject");
  }

  return new Promise<PermissionReply>((resolve) => {
    const abort = () => {
      pendingPermissionRequests.delete(options.requestID);
      resolve("reject");
    };
    options.abortSignal.addEventListener("abort", abort, { once: true });
    pendingPermissionRequests.set(options.requestID, {
      sessionID: options.sessionID,
      permission: options.permission,
      patterns: options.patterns,
      metadata: options.metadata,
      createdAt: Date.now(),
      resolve: (reply) => {
        options.abortSignal.removeEventListener("abort", abort);
        resolve(reply);
      },
      abort,
    });
  });
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

function asNumber(value: unknown) {
  return typeof value === "number" && Number.isFinite(value) ? value : 0;
}

function extractUsageFromResult(message: unknown) {
  const typedMessage = message as {
    usage?: Record<string, unknown>;
    modelUsage?: Record<string, Record<string, unknown>>;
    total_cost_usd?: unknown;
    cost?: unknown;
  };

  let inputTokens = 0;
  let outputTokens = 0;
  let reasoningTokens = 0;
  let cacheRead = 0;
  let cacheWrite = 0;

  if (typedMessage?.usage && typeof typedMessage.usage === "object") {
    const usage = typedMessage.usage as Record<string, unknown>;
    inputTokens = asNumber(usage.input_tokens ?? usage.inputTokens);
    outputTokens = asNumber(usage.output_tokens ?? usage.outputTokens);
    reasoningTokens = asNumber(usage.reasoning_tokens ?? usage.reasoningTokens);
    cacheRead = asNumber(usage.cache_read_input_tokens ?? usage.cacheReadInputTokens);
    cacheWrite = asNumber(usage.cache_creation_input_tokens ?? usage.cacheWriteInputTokens);
  } else if (typedMessage?.modelUsage && typeof typedMessage.modelUsage === "object") {
    for (const entry of Object.values(typedMessage.modelUsage)) {
      if (!entry || typeof entry !== "object") {
        continue;
      }
      inputTokens += asNumber(entry.input_tokens ?? entry.inputTokens);
      outputTokens += asNumber(entry.output_tokens ?? entry.outputTokens);
      reasoningTokens += asNumber(entry.reasoning_tokens ?? entry.reasoningTokens);
      cacheRead += asNumber(entry.cache_read_input_tokens ?? entry.cacheReadInputTokens);
      cacheWrite += asNumber(entry.cache_creation_input_tokens ?? entry.cacheWriteInputTokens);
    }
  }

  const cost = asNumber(typedMessage?.total_cost_usd ?? typedMessage?.cost);

  return {
    cost,
    tokens: {
      input: inputTokens,
      output: outputTokens,
      reasoning: reasoningTokens,
      cache: {
        read: cacheRead,
        write: cacheWrite,
      },
    },
  };
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

function persistReasoningPart(options: {
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
    "reasoning",
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

function serializePartValue(value: unknown) {
  if (value === undefined || value === null) {
    return null;
  }

  if (typeof value === "string") {
    return value;
  }

  try {
    return JSON.stringify(value);
  } catch {
    return String(value);
  }
}

function persistToolPart(options: {
  id: string;
  sessionID: string;
  messageID: string;
  tool: string;
  callId: string | null;
  input: unknown;
  output?: unknown;
  error?: unknown;
  timeStart: number;
  timeEnd?: number;
}) {
  statements.insertPart.run(
    options.id,
    options.sessionID,
    options.messageID,
    "tool",
    null,
    null,
    options.tool,
    options.callId,
    null,
    serializePartValue(options.input),
    serializePartValue(options.output),
    serializePartValue(options.error),
    null,
    null,
    options.timeStart,
    options.timeEnd ?? null,
    null,
  );
}

function persistSimplePart(options: {
  id: string;
  sessionID: string;
  messageID: string;
  type: string;
  title?: string | null;
  metadata?: unknown;
  timeStart: number;
  timeEnd?: number;
}) {
  statements.insertPart.run(
    options.id,
    options.sessionID,
    options.messageID,
    options.type,
    null,
    null,
    null,
    null,
    options.title ?? null,
    null,
    null,
    null,
    null,
    null,
    options.timeStart,
    options.timeEnd ?? null,
    serializePartValue(options.metadata),
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

function extractReasoningFromBlocks(blocks: unknown) {
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

    const typedBlock = block as { type?: unknown; thinking?: unknown; text?: unknown };
    if (typedBlock.type === "thinking" && typeof typedBlock.thinking === "string") {
      fullText += typedBlock.thinking;
    }

    if (typedBlock.type === "thinking_delta" && typeof typedBlock.thinking === "string") {
      deltaText += typedBlock.thinking;
      hasDelta = true;
    }

    if (typedBlock.type === "thinking_delta" && typeof typedBlock.text === "string") {
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

function emitSessionStatus(
  sessionID: string,
  status: "idle" | "busy" | "retry",
  options?: { modelId?: string; maxThinkingTokens?: number }
) {
  emitEvent("session.status", {
    sessionID,
    status: {
      type: status,
    },
    ...(options?.modelId ? { modelId: options.modelId } : {}),
    ...(options?.maxThinkingTokens !== undefined ? { maxThinkingTokens: options.maxThinkingTokens } : {}),
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
    maxThinkingTokens?: number;
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

  // Per-message thinking override (§4): message override > session default > undefined
  const messageMaxThinkingTokens = typeof body.maxThinkingTokens === "number" && body.maxThinkingTokens > 0
    ? body.maxThinkingTokens
    : undefined;
  const effectiveMaxThinkingTokens = messageMaxThinkingTokens ?? session.maxThinkingTokens;
  if (messageMaxThinkingTokens !== undefined) {
    console.log(`[message] thinking override: ${messageMaxThinkingTokens}`);
  }

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
  emitSessionStatus(session.record.id, "busy", {
    modelId: modelID,
    maxThinkingTokens: effectiveMaxThinkingTokens,
  });

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
    maxThinkingTokens: effectiveMaxThinkingTokens,
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
  maxThinkingTokens?: number;
}) {
  const { session, prompt, modelID, agent, assistantMessageInfo, abortController, maxThinkingTokens } = options;
  let assistantText = "";
  let reasoningText = "";
  const partId = session.activeRun?.assistantPartId ?? createId("part");
  const timeStart = session.activeRun?.startedAt ?? Date.now();
  const toolParts = new Map<string, {
    id: string;
    tool: string;
    input: unknown;
    timeStart: number;
  }>();
  const toolStartEmitted = new Set<string>();
  const subagentParts = new Map<string, { id: string; name: string; timeStart: number }>();

  const mapPermissionForTool = (toolName: string, input: unknown) => {
    const normalizedName = toolName.trim();
    const metadata = input && typeof input === "object" ? input : { value: input };
    const filePath = (input as { file_path?: unknown; path?: unknown })?.file_path
      ?? (input as { file_path?: unknown; path?: unknown })?.path;
    const patterns = typeof filePath === "string" ? [filePath] : [];

    if (normalizedName === "Write" || normalizedName === "Edit") {
      return { permission: "edit", patterns, metadata };
    }

    if (normalizedName === "Read" || normalizedName === "Glob" || normalizedName === "Grep") {
      return { permission: "read", patterns, metadata };
    }

    if (normalizedName === "Bash") {
      return { permission: "bash", patterns: [], metadata };
    }

    return { permission: normalizedName.toLowerCase(), patterns, metadata };
  };

  const emitPartUpdated = (part: Record<string, unknown>, delta?: string) => {
    emitEvent("message.part.updated", {
      part,
      ...(delta ? { delta } : {}),
    });
  };

  const emitToolPart = (options: {
    id: string;
    tool: string;
    callId: string | null;
    input: unknown;
    output?: unknown;
    error?: unknown;
    timeStart: number;
    timeEnd?: number;
  }) => {
    if (!options.timeEnd && toolStartEmitted.has(options.id)) {
      return;
    }

    if (!options.timeEnd) {
      toolStartEmitted.add(options.id);
    }

    persistToolPart({
      id: options.id,
      sessionID: session.record.id,
      messageID: assistantMessageInfo.id,
      tool: options.tool,
      callId: options.callId,
      input: options.input,
      output: options.output,
      error: options.error,
      timeStart: options.timeStart,
      timeEnd: options.timeEnd,
    });

    const time: Record<string, number> = { start: options.timeStart };
    if (options.timeEnd) {
      time.end = options.timeEnd;
    }

    emitPartUpdated({
      id: options.id,
      sessionID: session.record.id,
      messageID: assistantMessageInfo.id,
      type: "tool",
      tool: options.tool,
      callID: options.callId ?? undefined,
      input: options.input,
      output: options.output,
      error: options.error,
      time,
    });
  };

  try {
    const stream = query({
      prompt,
      options: {
        cwd: workspaceDir,
        resume: session.resumeId ?? undefined,
        abortController,
        model: modelID,
        maxThinkingTokens,
        includePartialMessages: true,
        settingSources: [...defaultSettingSources],
        systemPrompt: defaultSystemPrompt,
        tools: defaultTools,
        ...(defaultPermissionMode ? { permissionMode: defaultPermissionMode } : {}),
        canUseTool: async (toolName: string, input: unknown) => {
          if (toolName === "AskUserQuestion") {
            return {
              behavior: "deny",
              message: "Interactive questions are not supported in this session",
            };
          }

          const { permission, patterns, metadata } = mapPermissionForTool(toolName, input);
          const replyKey = permissionKey(permission, patterns);
          const sessionPermissions = getPersistentPermissionReplies(session.record.id);
          if (sessionPermissions.has(replyKey)) {
            return { behavior: "allow", updatedInput: input };
          }

          const requestId = createId("permission");
          emitEvent("permission.asked", {
            id: requestId,
            sessionID: session.record.id,
            permission,
            patterns,
            metadata,
            always: patterns,
          });

          const reply = await waitForPermissionReply({
            requestID: requestId,
            sessionID: session.record.id,
            permission,
            patterns,
            metadata: metadata as Record<string, unknown>,
            abortSignal: abortController.signal,
          });

          pendingPermissionRequests.delete(requestId);
          if (reply === "always") {
            sessionPermissions.add(replyKey);
          }

          emitEvent("permission.replied", {
            sessionID: session.record.id,
            requestID: requestId,
            reply,
          });

          if (reply === "reject") {
            return { behavior: "deny", message: "User denied permission" };
          }

          return { behavior: "allow", updatedInput: input };
        },
        hooks: {
          PreToolUse: [
            {
              hooks: [async (input: any, toolUseId: string | undefined) => {
                const toolName =
                  typeof input?.tool_name === "string"
                    ? input.tool_name
                    : typeof input?.toolName === "string"
                      ? input.toolName
                      : "tool";
                const toolInput = input?.tool_input ?? input?.toolInput ?? input?.input ?? null;
                const callId = toolUseId ?? createId("tool");
                const timeStarted = Date.now();
                const existing = toolParts.get(callId);
                if (existing) {
                  return {};
                }

                const partId = createId("part");

                toolParts.set(callId, {
                  id: partId,
                  tool: toolName,
                  input: toolInput,
                  timeStart: timeStarted,
                });

                emitToolPart({
                  id: partId,
                  tool: toolName,
                  callId,
                  input: toolInput,
                  timeStart: timeStarted,
                });

                return {};
              }],
            },
          ],
          PostToolUse: [
            {
              hooks: [async (input: any, toolUseId: string | undefined) => {
                const toolName =
                  typeof input?.tool_name === "string"
                    ? input.tool_name
                    : typeof input?.toolName === "string"
                      ? input.toolName
                      : "tool";
                const toolInput = input?.tool_input ?? input?.toolInput ?? input?.input ?? null;
                const toolOutput = input?.tool_response ?? input?.toolOutput ?? input?.output ?? null;
                const callId = toolUseId ?? createId("tool");
                const timeFinished = Date.now();
                const existing = toolParts.get(callId);
                const partId = existing?.id ?? createId("part");
                const timeStart = existing?.timeStart ?? timeFinished;

                toolParts.set(callId, {
                  id: partId,
                  tool: toolName,
                  input: existing?.input ?? toolInput,
                  timeStart,
                });

                emitToolPart({
                  id: partId,
                  tool: toolName,
                  callId,
                  input: existing?.input ?? toolInput,
                  output: toolOutput,
                  timeStart,
                  timeEnd: timeFinished,
                });

                return {};
              }],
            },
          ],
          PostToolUseFailure: [
            {
              hooks: [async (input: any, toolUseId: string | undefined) => {
                const toolName =
                  typeof input?.tool_name === "string"
                    ? input.tool_name
                    : typeof input?.toolName === "string"
                      ? input.toolName
                      : "tool";
                const toolInput = input?.tool_input ?? input?.toolInput ?? input?.input ?? null;
                const error = input?.error ?? input?.message ?? input ?? "Tool failed";
                const callId = toolUseId ?? createId("tool");
                const timeFinished = Date.now();
                const existing = toolParts.get(callId);
                const partId = existing?.id ?? createId("part");
                const timeStart = existing?.timeStart ?? timeFinished;

                toolParts.set(callId, {
                  id: partId,
                  tool: toolName,
                  input: existing?.input ?? toolInput,
                  timeStart,
                });

                emitToolPart({
                  id: partId,
                  tool: toolName,
                  callId,
                  input: existing?.input ?? toolInput,
                  error,
                  timeStart,
                  timeEnd: timeFinished,
                });

                return {};
              }],
            },
          ],
          SessionStart: [
            {
              hooks: [async () => {
                emitSessionStatus(session.record.id, "busy");
                return {};
              }],
            },
          ],
          SubagentStart: [
            {
              hooks: [async (input: any) => {
                const name = input?.agent ?? input?.name ?? "subagent";
                const agentId = input?.id ?? createId("subagent");
                const partId = createId("part");
                const timeStarted = Date.now();
                subagentParts.set(agentId, { id: partId, name, timeStart: timeStarted });
                persistSimplePart({
                  id: partId,
                  sessionID: session.record.id,
                  messageID: assistantMessageInfo.id,
                  type: "agent",
                  title: name,
                  metadata: { agentId },
                  timeStart: timeStarted,
                });
                emitPartUpdated({
                  id: partId,
                  sessionID: session.record.id,
                  messageID: assistantMessageInfo.id,
                  type: "agent",
                  name,
                  time: { start: timeStarted },
                });
                return {};
              }],
            },
          ],
          SubagentStop: [
            {
              hooks: [async (input: any, id: string | undefined) => {
                const agentId = id ?? input?.id ?? null;
                const timeFinished = Date.now();
                const existing = agentId ? subagentParts.get(agentId) : undefined;
                const partId = existing?.id ?? createId("part");
                const name = existing?.name ?? "subagent";
                if (agentId) {
                  subagentParts.delete(agentId);
                }
                persistSimplePart({
                  id: partId,
                  sessionID: session.record.id,
                  messageID: assistantMessageInfo.id,
                  type: "agent",
                  title: name,
                  metadata: { agentId },
                  timeStart: existing?.timeStart ?? timeFinished,
                  timeEnd: timeFinished,
                });
                emitPartUpdated({
                  id: partId,
                  sessionID: session.record.id,
                  messageID: assistantMessageInfo.id,
                  type: "agent",
                  name,
                  time: { start: existing?.timeStart ?? timeFinished, end: timeFinished },
                });
                return {};
              }],
            },
          ],
          PreCompact: [
            {
              hooks: [async () => {
                const partId = createId("part");
                const now = Date.now();
                persistSimplePart({
                  id: partId,
                  sessionID: session.record.id,
                  messageID: assistantMessageInfo.id,
                  type: "compaction",
                  title: "Compacting context",
                  timeStart: now,
                  timeEnd: now,
                });
                emitPartUpdated({
                  id: partId,
                  sessionID: session.record.id,
                  messageID: assistantMessageInfo.id,
                  type: "compaction",
                  title: "Compacting context",
                  time: { start: now, end: now },
                });
                return {};
              }],
            },
          ],
        },
      },
    });

    // Fetch supported models to populate cache (non-blocking)
    stream.supportedModels().then((models) => {
      updateModelsCache(models);
    }).catch((err) => {
      console.log("[models] failed to fetch from SDK:", err?.message ?? err);
    });

    for await (const message of stream) {
      if (message.type === "assistant") {
        const blocks = message.message?.content;
        const { fullText, deltaText, hasDelta } = extractTextFromBlocks(blocks);
        const reasoning = extractReasoningFromBlocks(blocks);
        let nextText = assistantText;
        let delta = "";
        let nextReasoning = reasoningText;
        let reasoningDelta = "";

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

        if (reasoning.hasDelta && reasoning.deltaText) {
          reasoningDelta = reasoning.deltaText;
          nextReasoning = reasoningText + reasoning.deltaText;
        } else if (reasoning.fullText) {
          if (reasoningText && reasoning.fullText.startsWith(reasoningText)) {
            reasoningDelta = reasoning.fullText.slice(reasoningText.length);
            nextReasoning = reasoning.fullText;
          } else if (reasoningText) {
            reasoningDelta = reasoning.fullText;
            nextReasoning = reasoningText + reasoning.fullText;
          } else {
            reasoningDelta = reasoning.fullText;
            nextReasoning = reasoning.fullText;
          }
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

        if (reasoningDelta) {
          reasoningText = nextReasoning;
          const timeEnd = Date.now();
          const reasoningPartId = `${partId}-reasoning`;
          persistReasoningPart({
            id: reasoningPartId,
            sessionID: session.record.id,
            messageID: assistantMessageInfo.id,
            text: reasoningText,
            timeStart,
            timeEnd,
          });
          emitPartUpdated(
            {
              id: reasoningPartId,
              sessionID: session.record.id,
              messageID: assistantMessageInfo.id,
              type: "reasoning",
              text: reasoningText,
              time: {
                start: timeStart,
                end: timeEnd,
              },
            },
            reasoningDelta,
          );
        }

        if (Array.isArray(blocks)) {
          for (const block of blocks) {
            if (!block || typeof block !== "object") {
              continue;
            }
            const typedBlock = block as {
              type?: unknown;
              id?: unknown;
              name?: unknown;
              input?: unknown;
              tool_use_id?: unknown;
              content?: unknown;
            };
          }
        }
      }

      if (message.type === "result") {
        if (message.session_id) {
          session.resumeId = message.session_id;
        }

        const usage = extractUsageFromResult(message);
        statements.updateMessageUsage.run(
          usage.cost,
          usage.tokens.input,
          usage.tokens.output,
          usage.tokens.reasoning,
          usage.tokens.cache.read,
          usage.tokens.cache.write,
          assistantMessageInfo.id,
        );
        assistantMessageInfo.cost = usage.cost;
        assistantMessageInfo.tokens = usage.tokens;
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

    if (pathname === "/models" && req.method === "GET") {
      return jsonResponse(getModels());
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
      let body: { parentID?: string; title?: string; permission?: string; maxThinkingTokens?: number } | null = null;
      try {
        body = (await req.json()) as typeof body;
      } catch {
        body = null;
      }

      const maxThinkingTokens = typeof body?.maxThinkingTokens === "number" && body.maxThinkingTokens > 0
        ? body.maxThinkingTokens
        : undefined;

      const record = buildSessionRecord(body?.title, body?.parentID);
      sessions.set(record.id, {
        record,
        resumeId: null,
        maxThinkingTokens,
        activeRun: null,
      });

      console.log(`[session] modelId=default maxThinkingTokens=${maxThinkingTokens ?? "undefined"}`);
      persistSession(record, null, maxThinkingTokens);

      emitEvent("session.created", {
        info: record,
        maxThinkingTokens,
      });

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

    // GET /permission/pending - List pending permission requests (§4, §5)
    if (pathname === "/permission/pending" && req.method === "GET") {
      const sessionId = url.searchParams.get("sessionId");
      const requests: Array<{
        id: string;
        sessionID: string;
        permission: string;
        patterns: string[];
        metadata: Record<string, unknown>;
        createdAt: number;
      }> = [];

      for (const [requestId, pending] of pendingPermissionRequests) {
        if (sessionId && pending.sessionID !== sessionId) {
          continue;
        }
        requests.push({
          id: requestId,
          sessionID: pending.sessionID,
          permission: pending.permission,
          patterns: pending.patterns,
          metadata: pending.metadata,
          createdAt: pending.createdAt,
        });
      }

      return jsonResponse({ requests });
    }

    // POST /permission/:requestId/reply - Reply to a pending permission (§4)
    if (pathname.startsWith("/permission/") && pathname.endsWith("/reply") && req.method === "POST") {
      const parts = pathname.split("/");
      const requestID = parts[2];
      if (!requestID) {
        return jsonResponse({ error: "invalid_request" }, 400);
      }

      let body: { reply?: PermissionReply; message?: string } | null = null;
      try {
        body = (await req.json()) as typeof body;
      } catch {
        body = null;
      }

      const reply = body?.reply;
      if (reply !== "once" && reply !== "always" && reply !== "reject") {
        return jsonResponse({ error: "invalid_request" }, 400);
      }

      const pending = pendingPermissionRequests.get(requestID);
      if (!pending) {
        return jsonResponse({ error: "permission_not_found" }, 404);
      }

      pendingPermissionRequests.delete(requestID);
      pending.resolve(reply);
      return jsonResponse({ success: true });
    }

    return jsonResponse({ error: "not_found" }, 404);
  },
});

// eslint-disable-next-line no-console
console.log(`Listening on http://${serverHost}:${server.port}`);
