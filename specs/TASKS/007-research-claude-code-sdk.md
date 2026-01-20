# Task: Research Claude Code SDK

## Objective

Research Claude Code's programmatic interface to understand how Maestro can spawn, attach to, and control Claude Code agents.

## Questions to Answer

1. **Server mode**: Does Claude Code have an `app-server` mode like Codex?
2. **Programmatic API**: Is there a thread/message API we can call?
3. **Event streaming**: How do we receive real-time output from agents?
4. **Session attachment**: Can we attach to running CLI sessions?
5. **Background agents**: How does the recently-added background agent support work?

## Resources to Research

### Claude Agent SDK
- Package: `@anthropic-ai/claude-agent-sdk` (TypeScript & Python)
- Docs: https://docs.anthropic.com/en/docs/claude-code (or similar)
- GitHub: Search for official Anthropic repos

### Claude Code CLI
- How does `claude` CLI work internally?
- Configuration files and home directory structure
- Any server/daemon modes?

## Output

Create `specs/CLAUDE_CODE_RESEARCH.md` with:

### 1. SDK Overview
- Available packages and installation
- Supported languages
- Core concepts

### 2. Agent Lifecycle
- How to spawn an agent programmatically
- How to send messages/prompts
- How to receive responses
- How to stop/cancel

### 3. Event Model
- What events are emitted?
- How to subscribe to output streams?
- Tool use events, approval events, etc.

### 4. Integration Options for Maestro
- Option A: Use SDK directly
- Option B: Wrap CLI in PTY (like CodexMonitor does with Codex)
- Option C: Other approaches

### 5. Recommendations
- Recommended integration approach
- Gaps/limitations to be aware of
- Code snippets for key operations

## Method

1. Index Claude Code SDK repo/docs via Nia (if available)
2. Search for existing implementations/examples
3. Review any official documentation
4. Test locally if possible

## Constraints

- Focus on what's actually available today
- Note any features that are experimental/unstable
- Compare capabilities to what CodexMonitor does with Codex
