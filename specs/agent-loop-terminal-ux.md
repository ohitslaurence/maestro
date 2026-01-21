# Agent Loop Terminal UX

**Status:** Draft
**Version:** 1.0
**Last Updated:** 2026-01-21

---

## 1. Overview
### Purpose
Provide a clear, rich terminal experience for `scripts/agent-loop.sh` so operators can see progress,
timing, and results in real time, with durable logs for post-run inspection.

### Goals
- Use `gum` for a polished live UI (headers, spinners, status lines, summary tables).
- Show iteration number, elapsed runtime, and last iteration duration while the loop runs.
- Persist full output to log files with stable naming and timestamps.
- Summarize run metrics and completion status at exit.
- Avoid garbled output by keeping `claude` stdin closed and handling signals cleanly.
- Provide interactive spec selection with searchable filtering when a spec path is not supplied.
- Sort selectable specs by most recent `Last Updated` when available.
- Exit the loop immediately when the completion token is detected, even if the agent output
  includes extra text.
- Present a final summary screen and a clear "close" affordance after completion.
- Emit an analyzable run report and prompt snapshot for post-run analysis.

### Non-Goals
- Full-screen TUI navigation or interactive controls.
- Changing the `claude` prompt or loop semantics.
- Remote log shipping or telemetry outside the local machine.

---

## 2. Architecture
### Components
- **Loop Runner**: existing orchestration in `scripts/agent-loop.sh` that controls iterations.
- **Gum UI Layer**: reusable helpers for styled headers, status lines, spinners, and summaries.
- **Spec Picker**: gum-driven selector that lists specs and resolves plan paths.
- **Logging + Metrics**: run-level and per-iteration log files, in-memory stats aggregation.
- **Signal Handler**: traps `INT`, `TERM`, and `EXIT` to print summary and restore terminal state.

### Dependencies
- `gum` CLI (Charm gum).
- Bash 3+ compatible shell features already used in the script.
- Core utilities: `date`, `mktemp`, `printf`, `tee`, `wc`, `sed`.

### Module/Folder Layout
- `scripts/agent-loop.sh` (updated entry point)
- `scripts/lib/agent-loop-ui.sh` (new helper library)
- `scripts/lib/spec-picker.sh` (new helper for spec discovery)
- `logs/agent-loop/` (run logs, created on demand)

---

## 3. Data Model
### Core Types
**RunConfig**
| Field | Type | Required | Notes |
| --- | --- | --- | --- |
| `spec_path` | string | yes | Path to spec file |
| `plan_path` | string | yes | Path to plan file |
| `iterations` | number | yes | Maximum loop iterations |
| `log_dir` | string | yes | Base log directory |
| `run_id` | string | yes | `YYYYmmdd-HHMMSS` timestamp |
| `gum_enabled` | boolean | yes | Disabled when `--no-gum` or non-TTY |

**SpecEntry**
| Field | Type | Required | Notes |
| --- | --- | --- | --- |
| `spec_path` | string | yes | Path under `specs/` |
| `plan_path` | string | yes | Path under `specs/planning/` |
| `title` | string | yes | First heading in the spec |
| `status` | string | no | Optional parsed status field |
| `last_updated` | string | no | `YYYY-MM-DD` parsed from metadata |

**IterationStats**
| Field | Type | Required | Notes |
| --- | --- | --- | --- |
| `iteration` | number | yes | 1-based index |
| `start_ms` | number | yes | Unix epoch millis |
| `end_ms` | number | yes | Unix epoch millis |
| `duration_ms` | number | yes | `end_ms - start_ms` |
| `exit_code` | number | yes | Exit status of `claude` |
| `complete_detected` | boolean | yes | Output matched `<promise>COMPLETE</promise>` |
| `log_path` | string | yes | Per-iteration log file |
| `output_bytes` | number | no | Raw output size in bytes |
| `output_lines` | number | no | Raw output line count |
| `tail_path` | string | no | Last N lines for quick inspection |

**RunSummary**
| Field | Type | Required | Notes |
| --- | --- | --- | --- |
| `start_ms` | number | yes | Run start time |
| `end_ms` | number | yes | Run end time |
| `total_duration_ms` | number | yes | Overall runtime |
| `iterations_run` | number | yes | Completed iterations |
| `completed_iteration` | number | no | Iteration where COMPLETE occurred |
| `avg_duration_ms` | number | yes | Average iteration time |
| `last_exit_code` | number | yes | Exit code from last iteration |
| `completion_mode` | string | yes | `strict` when exact match, `lenient` when token appears |

### Storage Schema (if any)
- Run log: `logs/agent-loop/run-<run_id>.log`
- Per-iteration logs: `logs/agent-loop/run-<run_id>-iter-<NN>.log`
- Optional summary JSON: `logs/agent-loop/run-<run_id>-summary.json`
- Run report (TSV): `logs/agent-loop/run-<run_id>-report.tsv`
- Prompt snapshot: `logs/agent-loop/run-<run_id>-prompt.txt`
- Per-iteration tail: `logs/agent-loop/run-<run_id>-iter-<NN>.tail.txt`
- Analysis prompt: `logs/agent-loop/run-<run_id>-analysis-prompt.txt`

---

## 4. Interfaces
### Public APIs
Script usage accepts positional arguments or interactive selection, with optional flags:

```
./scripts/agent-loop.sh [spec-path] [plan-path] \
  [--iterations <n>] [--log-dir <path>] [--no-gum] [--summary-json] [--no-wait]
```

Defaults:
- `--iterations`: `50`
- `--log-dir`: `logs/agent-loop`

Completion behavior:
- Strict mode: if output equals `<promise>COMPLETE</promise>` after trimming, mark `completion_mode=strict`.
- Lenient mode: if the token appears anywhere else in the output, mark `completion_mode=lenient`,
  emit a warning, but stop the loop.
- Close affordance: use `gum confirm --default=true "Close"` unless `--no-wait` is set.

Selection behavior:
- If `spec-path` is omitted and gum is available, launch the spec picker.
- If gum is unavailable or `--no-gum` is set, require `spec-path` and print usage.

### Internal APIs
- `ui_header(title)`
- `ui_status(line)`
- `ui_spinner(title, command...)`
- `ui_log(level, message)`
- `ui_table(title, rows...)`
- `spec_picker(list_path) -> spec_path, plan_path`

### Events (names + payloads)
Log markers written to the run log:
- `RUN_START`, `RUN_END`
- `ITERATION_START`, `ITERATION_END`
- `COMPLETE_DETECTED`
- `ERROR`

---

## 5. Workflows
### Main Flow
```
Parse args -> optional spec picker -> validate paths -> resolve log dir -> init UI
  -> write prompt snapshot + report header
  -> for each iteration:
       show header + status
       run claude (spinner)
       write output to iteration log
       detect COMPLETE (strict or lenient)
       update stats + status line
  -> print summary (table)
  -> show completion screen and wait for user input unless --no-wait
```

### Spec Picker Flow
```
Discover specs -> parse title/status -> gum filter -> resolve plan path -> return selection
```

Discovery rules:
- Scan `specs/*.md` excluding `specs/README.md` and `specs/research/`.
- Parse the first heading as `title` and optional `Status` field for display.
- Compute plan path as `specs/planning/<spec>-plan.md` by default.
- Sort by `Last Updated` descending when present; fall back to file mtime, then name.

### Edge Cases
- **gum missing**: if `--no-gum` or non-TTY, fall back to plain output; otherwise exit with
  install instructions.
- **spec missing**: if no spec selected, exit with a clear message and list known specs.
- **plan missing**: if the selected spec has no plan, exit with a path hint.
- **completion protocol violation**: if token appears with extra output, log the full output and
  surface a warning in the summary.
- **non-zero exit**: log error, emit summary, and exit non-zero.
- **empty output**: log warning and continue.
- **signal interrupt**: trap `INT`/`TERM`, print summary, and exit 130/143.

### Retry/Backoff
No retries; the loop remains deterministic. Errors require a manual restart.

---

## 6. Error Handling
### Error Types
- `missing_spec`
- `missing_plan`
- `gum_missing`
- `log_dir_unwritable`
- `claude_failed`
- `output_parse_error`

### Recovery Strategy
- Fail fast with a clear log message and a summary line.
- When `claude` fails, retain the iteration log and surface its path.

---

## 7. Observability
### Logs
- Run log includes timestamps and markers for each iteration.
- Per-iteration logs capture raw `claude` output verbatim.
- Run report captures structured events suitable for analysis.
- Per-iteration tail files capture the final output chunk for quick review.
- Analysis prompt generator (`scripts/agent-loop-analyze.sh`) emits a standard review prompt.

### Metrics
- Total runtime, per-iteration duration, average duration, completed iteration.

### Traces
None; this is a local shell workflow.

---

## 8. Security and Privacy
### AuthZ/AuthN
Not applicable; local script only.

### Data Handling
Logs may include prompts and agent output. Store locally and avoid sharing if sensitive.

---

## 9. Migration or Rollout
### Compatibility Notes
- Positional arguments remain unchanged.
- New flags are optional and additive.

### Rollout Plan
1. Add gum UI helpers + logging.
2. Add summary output and signal handling.
3. Document gum dependency in README or script usage output.

---

## 10. Open Questions
- Should gum be a hard dependency or installed automatically?
- Do we want log retention or pruning helpers?
- Should the summary JSON include prompt metadata for future dashboards?
