# doctor CLI Subcommand — Design

Date: 2026-04-11
Status: Approved (design phase)

## Goal

Add a `doctor` subcommand to `tmux-agent-sidebar` that prints, as JSON on stdout, the full list of hooks that Claude Code and Codex need to register in order for the sidebar to receive events, together with ready-to-paste config snippets.

The command is a pure generator: it derives everything from the in-code `HOOK_REGISTRATIONS` tables, does not read any user config files, and does not mutate anything on disk. Its output is designed to be consumed by either a human (copy/paste into `~/.claude/settings.json` / `~/.codex/hooks.json`) or an LLM helping the user set up hooks.

### Why now

The `HOOK_REGISTRATIONS` table in `src/adapter/mod.rs` is already documented as the single source of truth for "install wizards, README snippets, doctor commands, and docs". The `doctor` command is the first consumer of that table beyond the existing drift tests.

### Non-goals

- **No diagnosis.** The command does not read the user's actual `settings.json` / `hooks.json` and does not report which hooks are missing vs. present. That belongs to a future sidebar-side warning surface (see "Future direction") and is explicitly out of scope here.
- **No writing to disk.** No patching of user config files. No prompts. No interactive mode.
- **No new data source.** The command reads only from `ClaudeAdapter::HOOK_REGISTRATIONS` and `CodexAdapter::HOOK_REGISTRATIONS`. It does not duplicate hook knowledge.

### Future direction (motivation, not in scope)

Once this generator exists, the sidebar TUI process can reuse the same data to detect at startup that required hooks are not firing (e.g., by comparing the registration table against observed `/tmp/tmux-agent-*` files) and surface a warning alongside the existing version-update notice. That surface is built later, on top of this command.

## User Experience

Two invocation forms:

**Full mode** — no argument:

```
$ tmux-agent-sidebar doctor
{
  "version": "0.4.0",
  "hook_script": "/Users/alice/.tmux/plugins/tmux-agent-sidebar/hook.sh",
  "agents": {
    "claude": { ... },
    "codex":  { ... }
  }
}
```

**Single-agent mode** — one positional argument, an agent name:

```
$ tmux-agent-sidebar doctor claude
{
  "hooks": {
    "SessionStart": [
      { "matcher": "", "hooks": [{ "type": "command", "command": "bash /.../hook.sh claude session-start" }] }
    ],
    ...
  }
}
```

- The only accepted agent names are `claude` and `codex`. Any other value (or more than one argument) exits with code `2` and an error on stderr.
- Single-agent mode prints **only the snippet** — the raw JSON that can be pasted into `~/.claude/settings.json` or `~/.codex/hooks.json` unchanged (top-level `{ "hooks": { ... } }`). No wrapping object, no `version`, no `hook_script`, no normalized `hooks[]` array. This is the form an LLM can feed straight into `jq` or a file write.
- Both modes output pretty-printed JSON (two-space indent) on stdout. Exit code `0` on success. No stderr on success.

## Output Schema

Top level:

| Field | Type | Meaning |
|---|---|---|
| `version` | string | `tmux-agent-sidebar` binary version (same value as `--version`). |
| `hook_script` | string | Absolute path to `hook.sh` that should appear in the generated commands. |
| `agents` | object | Keyed by agent name (`"claude"`, `"codex"`). |

Each agent entry:

| Field | Type | Meaning |
|---|---|---|
| `config_path` | string | Suggested file to paste the snippet into (e.g. `"~/.claude/settings.json"`). Display-only hint; the tilde is *not* expanded. |
| `hooks` | array | Normalized hook registrations. One entry per `HookRegistration` in the adapter's table. |
| `snippet` | object | Ready-to-paste JSON fragment in the exact shape each agent's config file expects. |

Each `hooks[]` entry (normalized view, intended for machine consumers):

| Field | Type | Meaning |
|---|---|---|
| `trigger` | string | Upstream trigger name as the agent writes it in its config (e.g. `"SessionStart"`, `"PostToolUse"`). |
| `matcher` | string \| null | `null` when `HookRegistration.matcher` is `None`, otherwise the literal matcher string. |
| `event` | string | External event name the sidebar expects on the CLI (from `AgentEventKind::external_name()`, e.g. `"session-start"`, `"activity-log"`). |
| `command` | string | The full shell command to register: `bash <hook_script> <agent> <event>`. |

Each `snippet` is the shape each agent's config file already uses (matching the current README):

```json
{
  "hooks": {
    "SessionStart": [
      {
        "matcher": "",
        "hooks": [
          { "type": "command", "command": "bash /.../hook.sh claude session-start" }
        ]
      }
    ],
    "PostToolUse": [ ... ]
  }
}
```

- When `HookRegistration.matcher` is `None`, `"matcher"` in the snippet is the empty string `""` (matching README convention, which is how Claude Code / Codex express "any tool").
- When `HookRegistration.matcher` is `Some("startup|resume")`, `"matcher"` is that literal value. (Codex `SessionStart` is the only current case.)
- Registrations that share a `trigger` are grouped under the same key in the snippet, preserving the order they appear in the adapter's table.

The snippet is nested JSON *inside* the top-level JSON output, not a string. Consumers who want it as a string can serialize the sub-tree themselves.

## Architecture

### New module

`src/cli/doctor.rs` — owns the command. Wired into `src/cli/mod.rs::run()` as:

```rust
"doctor" => doctor::cmd_doctor(rest),
```

No other changes to `mod.rs` beyond adding the `mod doctor;` declaration and the match arm.

### Pure core, thin CLI shell

The logic is split so it is testable without shelling out:

```rust
// Pure. Full output (both agents, normalized + snippet + metadata).
pub(crate) fn build_doctor_output(hook_script: &str) -> serde_json::Value;

// Pure. Single-agent snippet — the raw { "hooks": { ... } } block.
// Returns None for unknown agent names.
pub(crate) fn build_agent_snippet(agent: &str, hook_script: &str) -> Option<serde_json::Value>;

// Thin wrapper: parses args, resolves path, prints, returns exit code.
pub(crate) fn cmd_doctor(args: &[String]) -> i32;
```

Both builder functions have no I/O and no environment lookups. `build_doctor_output` calls `build_agent_snippet` internally for each agent so that the two views of the same data cannot diverge.

`cmd_doctor` dispatches on argument count:

- `[]` → call `build_doctor_output`, print pretty-printed, return `0`.
- `[agent]` → call `build_agent_snippet(agent, ...)`; if `Some`, print pretty-printed and return `0`; if `None`, write `"error: unknown agent '<name>' (expected 'claude' or 'codex')"` to stderr and return `2`.
- `[_, _, ...]` → write a usage line to stderr and return `2`.

### Data source

`build_doctor_output` walks `ClaudeAdapter::HOOK_REGISTRATIONS` and `CodexAdapter::HOOK_REGISTRATIONS` and builds both the normalized `hooks` array and the `snippet` object in one pass. It uses `AgentEventKind::external_name()` to derive the event name embedded in the command.

No hook knowledge is duplicated. If a new registration is added to an adapter's table, `doctor` picks it up automatically.

### Hook script path resolution

Implemented as a small helper `resolve_hook_script() -> String` in `doctor.rs`:

1. `std::env::current_exe()` — get the absolute path of the running binary.
2. Walk parent directories looking for a sibling `hook.sh`. The two layouts the project already supports (see `hook.sh`):
   - `<plugin_dir>/bin/tmux-agent-sidebar` → `<plugin_dir>/hook.sh`
   - `<plugin_dir>/target/release/tmux-agent-sidebar` → `<plugin_dir>/hook.sh`
   Generalized: starting from the binary's directory, walk up at most 3 levels; at each level, check whether `<level>/hook.sh` exists. Return the first hit.
3. Fallback: `~/.tmux/plugins/tmux-agent-sidebar/hook.sh` as a literal string (the tilde is not expanded; this matches the README and stays LLM-friendly).

Fallback kicks in when `current_exe()` fails, when no `hook.sh` is found in the walk, or when the binary was installed somewhere unrelated (e.g. `/usr/local/bin`). The function never panics and never returns `Result`.

### CLI help / version

`doctor` is an undocumented subcommand in this release (same as the existing `hook`, `toggle`, etc. — the project does not use a full CLI parser). It is announced in the README in a follow-up PR, not in this change.

## Testing Strategy

Tests live in `src/cli/doctor.rs` under `#[cfg(test)] mod tests` and exercise the pure core only. No filesystem, no subprocesses.

### What gets tested

1. **Schema shape.** `build_doctor_output("/fake/hook.sh")` returns an object with top-level keys `version`, `hook_script`, `agents`. `agents` contains exactly `claude` and `codex`.

2. **Hook script propagation.** `hook_script` at the top level equals the input. Every `command` string in every `hooks[]` entry starts with `"bash /fake/hook.sh "`.

3. **Completeness vs. adapter tables.** For each adapter, `output.agents.<name>.hooks.len()` equals `<Adapter>::HOOK_REGISTRATIONS.len()`. For each registration, there is a matching normalized entry with the right `trigger`, `matcher`, `event`, and `command`. This catches "forgot to include one" regressions.

4. **Snippet grouping.** For Claude, a chosen shared-trigger case (if any exist in the table) has all its registrations under the same key in `snippet.hooks.<Trigger>`, in table order. For Codex, `SessionStart` has `matcher: "startup|resume"` in the snippet (not `""`), and every other trigger has `matcher: ""`.

5. **Snippet command strings match normalized entries.** For every registration, the `command` in the snippet equals the `command` in the normalized `hooks[]` entry for the same registration. Prevents the two views from drifting.

6. **External event name correctness.** Spot-check that `SessionStart` maps to `event: "session-start"` and `PostToolUse` maps to `event: "activity-log"` (the known rename). The full mapping is already covered by the adapter's drift test; `doctor` tests just verify the lookup is wired.

7. **Snapshot test.** `build_doctor_output("/fake/hook.sh")` is serialized with `serde_json::to_string_pretty` and compared against an inline expected-string literal. This locks the full JSON shape and is the fastest way to spot accidental schema changes in review. When the adapter tables legitimately change, this test is updated in the same PR.

8. **Single-agent snippet.** `build_agent_snippet("claude", "/fake/hook.sh")` returns `Some` and equals `build_doctor_output(...).agents.claude.snippet` (the two views come from the same data and must never diverge). Same check for Codex. `build_agent_snippet("unknown", "/fake/hook.sh")` returns `None`.

### What is explicitly not tested here

- `resolve_hook_script()` is not unit-tested. It depends on `current_exe()` and on the filesystem layout, which cannot be pinned in `cargo test` without making the test environment-sensitive. The fallback string is verified indirectly by the existence of the helper; the walking logic is simple enough that a snapshot-style test is not worth the flakiness.
- No integration test that shells out to the binary and parses stdout. The pure-core split makes the CLI wrapper trivial enough that a direct test adds no signal.

### Existing tests that stay as safety nets

- `assert_table_drift_free` in `src/adapter/mod.rs` — guarantees the adapter tables match the `parse()` arms. `doctor` inherits this: if the table is right, `doctor` is right.
- No changes to existing tests are required.

## Risks and Mitigations

| Risk | Mitigation |
|---|---|
| Adapter table gains a new registration, `doctor` output silently changes. | Snapshot test fails, forcing explicit acknowledgement in the PR. |
| Snippet shape drifts from what Claude/Codex actually accept. | Snippet shape is the same one already shipped in README and tested in practice. A shape change is an intentional event, not an accident. |
| `current_exe()` returns a path with no nearby `hook.sh`. | Fallback to the literal `~/.tmux/plugins/tmux-agent-sidebar/hook.sh` string, same as README. User can still copy the snippet; only the absolute path differs. |
| Codex matcher conventions change. | Matcher string is read from `HookRegistration.matcher`, not hardcoded in `doctor`. One place to update. |

## Files Touched

- `src/cli/mod.rs` — declare `mod doctor;`, add `"doctor"` match arm.
- `src/cli/doctor.rs` — new file: `cmd_doctor`, `build_doctor_output`, `resolve_hook_script`, tests.

No changes to adapter code, event layer, tmux layer, or UI.
