# Maestro Vision

**Status:** Active
**Last Updated:** 2026-01-22

---

## What Maestro Is

A macOS app for orchestrating AI coding agents across local and remote workspaces. Not just a terminal wrapper - a full development environment that understands projects, specs, and agent workflows.

---

## Core Experience

### 1. Workspaces & Projects

Workspaces are top-level containers that group related projects:

```
┌─────────────────────────────────────────┐
│  Workspaces                             │
│  ├── Personal                           │
│  │   ├── maestro/                       │
│  │   ├── side-project/                  │
│  │   └── experiments/                   │
│  └── Spritz                             │
│      ├── backend/                       │
│      └── frontend/                      │
└─────────────────────────────────────────┘
```

- Switch workspaces from sidebar
- Each workspace = a folder on disk containing project repos
- Projects are git repos within a workspace folder
- Open a project to start working

### 2. Agent Conversations

Open a project → start an agent thread. Full TUI-equivalent experience:

- **Chat interface**: Send prompts, see streaming responses
- **Tool execution**: Watch file edits, command runs, searches
- **Extended thinking**: Toggle and view reasoning
- **Model selection**: Choose model per session
- **Abort/resume**: Stop execution, continue later

Works with multiple harnesses:
- Claude Code (via Claude Agent SDK)
- Open Code (via its server protocol)
- Future: extensible to other agents

### 3. Git Integration

First-class git within each project:

- **Status panel**: Staged, unstaged, untracked files
- **Diff viewer**: Split or unified view with syntax highlighting
- **Commit flow**: Stage changes, compose messages (with gritty)
- **History**: Browse commits, view diffs per commit
- **Branch indicator**: Current branch in session info

### 4. Spec-Driven Development

Specs are first-class artifacts:

**Create a spec:**
- "New Spec" → Agent interviews you about the feature
- Uses a standardized template
- References existing specs automatically
- Outputs to `specs/<name>.md` + `specs/planning/<name>-plan.md`

**Work from specs:**
- Pick a spec to implement
- Agent has full context of the spec + plan
- Track progress against plan tasks

### 5. Agent Loop (Native)

Two modes of working:

**Interactive**: Chat back-and-forth, exploratory work, small tasks.

**Agent Loop**: Automated iteration for larger tasks.
- Select a spec + plan
- Configure: iterations, model, permissions
- Start loop → agent works autonomously
- Watch progress: iteration count, task completion, timing
- View live output per iteration
- Loop terminates on completion token or max iterations
- **Postmortem**: Analysis of what was done, quality check, improvement suggestions

The agent loop is native to the app, not a shell script. Full visibility into:
- Current iteration
- Elapsed time
- Tasks completed vs remaining
- Live streaming output
- Final summary + analysis

---

## Architecture

```
┌─────────────────────────────────────────────────────────────────────────┐
│                            Maestro App (Tauri)                          │
│  ┌───────────────────────────────────────────────────────────────────┐  │
│  │                         React Frontend                            │  │
│  │  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────────────┐   │  │
│  │  │Workspaces│  │  Agent   │  │   Git    │  │   Spec Manager   │   │  │
│  │  │ Sidebar  │  │  Thread  │  │  Panel   │  │   + Agent Loop   │   │  │
│  │  └──────────┘  └──────────┘  └──────────┘  └──────────────────┘   │  │
│  └───────────────────────────────────────────────────────────────────┘  │
│                                    │                                    │
│                                    │ Tauri IPC                          │
│  ┌───────────────────────────────────────────────────────────────────┐  │
│  │                          Rust Backend                             │  │
│  │  ┌────────────────┐  ┌────────────────┐  ┌────────────────────┐   │  │
│  │  │ Daemon Client  │  │ Local Storage  │  │  Workspace Manager │   │  │
│  │  │ (JSON-RPC)     │  │ (Persistence)  │  │  (Project Index)   │   │  │
│  │  └────────────────┘  └────────────────┘  └────────────────────┘   │  │
│  └───────────────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────────────┘
                                    │
                                    │ TCP JSON-RPC
                                    ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                          Maestro Daemon (VPS)                           │
│  ┌────────────────┐  ┌────────────────┐  ┌────────────────────────┐    │
│  │ Session Manager│  │  Terminal PTY  │  │    Git Operations      │    │
│  │ (Agent Pool)   │  │  (Streaming)   │  │    (Status, Diff)      │    │
│  └────────────────┘  └────────────────┘  └────────────────────────┘    │
│                              │                                          │
│                              │ spawns                                   │
│                              ▼                                          │
│  ┌────────────────────────────────────────────────────────────────┐    │
│  │                    Agent Harnesses                              │    │
│  │  ┌─────────────────┐  ┌─────────────────┐  ┌───────────────┐   │    │
│  │  │  Claude SDK     │  │    OpenCode     │  │    Future     │   │    │
│  │  │  Server (Bun)   │  │    (Native)     │  │   Harnesses   │   │    │
│  │  └─────────────────┘  └─────────────────┘  └───────────────┘   │    │
│  └────────────────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────────────────┘
```

---

## Data Model

### Workspace
```typescript
interface Workspace {
  id: string;
  name: string;              // "Personal", "Spritz"
  rootPath: string;          // "/Users/laurence/dev/personal"
  projects: Project[];
}
```

### Project
```typescript
interface Project {
  id: string;
  name: string;              // "maestro"
  path: string;              // Full path to repo
  workspaceId: string;
  gitBranch?: string;
  lastOpenedAt?: number;
}
```

### Thread (Conversation)
```typescript
interface Thread {
  id: string;
  projectId: string;
  harness: 'claude_code' | 'open_code';
  title: string;
  createdAt: number;
  updatedAt: number;
  status: 'idle' | 'busy' | 'error';
  messages: Message[];
}
```

### Spec
```typescript
interface Spec {
  id: string;
  projectId: string;
  name: string;
  specPath: string;          // specs/feature-x.md
  planPath: string;          // specs/planning/feature-x-plan.md
  status: 'draft' | 'ready' | 'in_progress' | 'complete';
  tasks: SpecTask[];
}

interface SpecTask {
  id: string;
  description: string;
  status: 'pending' | 'in_progress' | 'completed';
}
```

### AgentLoopRun
```typescript
interface AgentLoopRun {
  id: string;
  specId: string;
  projectId: string;
  config: {
    model: string;
    maxIterations: number;
    permissionMode: string;
  };
  status: 'running' | 'completed' | 'failed' | 'stopped';
  iterations: IterationResult[];
  startedAt: number;
  endedAt?: number;
  postmortem?: PostmortemReport;
}
```

---

## Phased Delivery

### Phase A: Foundation (Complete)
- [x] Tauri app scaffolded
- [x] Remote daemon with JSON-RPC
- [x] Terminal PTY streaming
- [x] Session discovery
- [x] Basic git operations
- [x] macOS glass styling

### Phase B: Agent Harnesses (In Progress)
- [x] Claude SDK server spec
- [x] Claude SDK UI spec
- [ ] Claude SDK server implementation
- [ ] Claude SDK UI integration
- [ ] OpenCode harness improvements
- [ ] Model selection UI

### Phase C: Git Polish
- [ ] Git panel in main UI
- [ ] Split/unified diff viewer
- [ ] Commit flow with staging
- [ ] History browser
- [ ] Branch indicator in sidebar

### Phase D: Workspaces & Projects
- [ ] Workspace data model
- [ ] Workspace switcher UI
- [ ] Project list per workspace
- [ ] Project persistence (recent, favorites)

### Phase E: Spec System
- [ ] Spec discovery (scan specs/ folder)
- [ ] Spec viewer
- [ ] "New Spec" wizard with agent interview
- [ ] Spec template system
- [ ] Plan progress tracking

### Phase F: Native Agent Loop
- [ ] Agent loop engine (Rust or managed Bun process)
- [ ] Iteration monitoring UI
- [ ] Live output streaming
- [ ] Completion detection
- [ ] Postmortem generation
- [ ] Run history

### Phase G: Advanced Orchestration
- [ ] Multi-project coordination
- [ ] Worktree management
- [ ] Sub-agent spawning
- [ ] Meta-orchestrator
- [ ] Ralph Wiggins loop (fresh agent per task to avoid context degradation)

---

## Success Criteria

When complete, this workflow is seamless:

1. Open Maestro → see Personal workspace
2. Click on `maestro` project
3. View git status, see uncommitted changes
4. "New Spec" → describe a feature → agent interviews me → spec created
5. Open the spec → see tasks in the plan
6. "Start Agent Loop" → select model, iterations → loop runs
7. Watch progress: iterations completing, tasks checked off
8. Loop finishes → postmortem shows what was done
9. Review diffs → stage and commit with gritty
10. Switch to Spritz workspace → different context, fresh start

---

## Open Questions

1. **Workspace persistence**: Local JSON? SQLite? How to sync across machines?
2. **Spec templates**: Hardcoded or configurable per workspace?
3. **Agent loop permissions**: How to handle tool approvals during automated runs?
4. **Multi-daemon**: Support multiple VPS daemons for different workspaces?
5. **Mobile companion**: Still a goal? Scope for v1?
