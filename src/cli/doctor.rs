//! `doctor` subcommand — prints required hooks and ready-to-paste config
//! snippets for Claude Code and Codex as JSON on stdout. Pure generator:
//! reads only the adapter `HOOK_REGISTRATIONS` tables, never the user's
//! config files.

use crate::adapter::HookRegistration;
use crate::adapter::claude::ClaudeAdapter;
use crate::adapter::codex::CodexAdapter;

#[allow(dead_code)]
const _CLAUDE_TABLE_REACHABLE: &[HookRegistration] = ClaudeAdapter::HOOK_REGISTRATIONS;
#[allow(dead_code)]
const _CODEX_TABLE_REACHABLE: &[HookRegistration] = CodexAdapter::HOOK_REGISTRATIONS;

/// Build the ready-to-paste `{ "hooks": { ... } }` JSON block for a single
/// agent. Returns `None` for unknown agent names.
///
/// Reads **only** from the adapter's `HOOK_REGISTRATIONS` table and
/// `AgentEventKind::external_name()` — no hook identity is duplicated here.
/// When `HookRegistration.matcher` is `None`, the snippet uses the empty
/// string `""` (matching Claude/Codex's "any tool" convention).
pub(crate) fn build_agent_snippet(agent: &str, hook_script: &str) -> Option<serde_json::Value> {
    let table: &[HookRegistration] = match agent {
        "claude" => ClaudeAdapter::HOOK_REGISTRATIONS,
        "codex" => CodexAdapter::HOOK_REGISTRATIONS,
        _ => return None,
    };

    let mut hooks = serde_json::Map::new();
    for reg in table {
        let matcher = reg.matcher.unwrap_or("");
        let command = format!(
            "bash {} {} {}",
            hook_script,
            agent,
            reg.kind.external_name()
        );
        let entry = serde_json::json!({
            "matcher": matcher,
            "hooks": [
                { "type": "command", "command": command }
            ],
        });
        let arr = hooks
            .entry(reg.trigger.to_string())
            .or_insert_with(|| serde_json::Value::Array(Vec::new()))
            .as_array_mut()
            .expect("trigger entry must be an array");
        arr.push(entry);
    }

    Some(serde_json::json!({ "hooks": serde_json::Value::Object(hooks) }))
}

/// Build the full doctor output: version, resolved hook script path,
/// and a per-agent object containing `config_path`, the normalized
/// `hooks[]` array, and the ready-to-paste `snippet`.
///
/// Pure function. `hook_script` is passed in so tests can pin it.
pub(crate) fn build_doctor_output(hook_script: &str) -> serde_json::Value {
    let claude = build_agent_entry(
        "claude",
        "~/.claude/settings.json",
        ClaudeAdapter::HOOK_REGISTRATIONS,
        hook_script,
    );
    let codex = build_agent_entry(
        "codex",
        "~/.codex/hooks.json",
        CodexAdapter::HOOK_REGISTRATIONS,
        hook_script,
    );

    serde_json::json!({
        "version": crate::VERSION,
        "hook_script": hook_script,
        "agents": {
            "claude": claude,
            "codex": codex,
        },
    })
}

fn build_agent_entry(
    agent: &str,
    config_path: &str,
    table: &[HookRegistration],
    hook_script: &str,
) -> serde_json::Value {
    let hooks: Vec<serde_json::Value> = table
        .iter()
        .map(|reg| {
            let command = format!(
                "bash {} {} {}",
                hook_script,
                agent,
                reg.kind.external_name()
            );
            serde_json::json!({
                "trigger": reg.trigger,
                "matcher": match reg.matcher {
                    Some(m) => serde_json::Value::String(m.to_string()),
                    None => serde_json::Value::Null,
                },
                "event": reg.kind.external_name(),
                "command": command,
            })
        })
        .collect();

    let snippet = build_agent_snippet(agent, hook_script)
        .expect("agent name hardcoded above, must match build_agent_snippet");

    serde_json::json!({
        "config_path": config_path,
        "hooks": hooks,
        "snippet": snippet,
    })
}

/// Resolve the absolute path of `hook.sh` to embed in the generated
/// commands. Strategy:
///
/// 1. `std::env::current_exe()` → get the running binary path.
/// 2. Walk up at most 3 directories from its parent, checking for a
///    sibling `hook.sh` at each level. Matches the two layouts the
///    project already supports:
///      - `<plugin>/bin/tmux-agent-sidebar` → `<plugin>/hook.sh`
///      - `<plugin>/target/release/tmux-agent-sidebar` → `<plugin>/hook.sh`
/// 3. Fallback: the literal string `~/.tmux/plugins/tmux-agent-sidebar/hook.sh`
///    (tilde intentionally not expanded, matches README).
///
/// Never panics.
fn resolve_hook_script() -> String {
    const FALLBACK: &str = "~/.tmux/plugins/tmux-agent-sidebar/hook.sh";

    let Ok(exe) = std::env::current_exe() else {
        return FALLBACK.to_string();
    };
    let Some(mut dir) = exe.parent().map(|p| p.to_path_buf()) else {
        return FALLBACK.to_string();
    };
    for _ in 0..=3 {
        let candidate = dir.join("hook.sh");
        if candidate.is_file() {
            return candidate.to_string_lossy().into_owned();
        }
        match dir.parent() {
            Some(parent) => dir = parent.to_path_buf(),
            None => break,
        }
    }
    FALLBACK.to_string()
}

/// Pure dispatch core. Returns the exit code and the JSON to print
/// (or `None` if nothing should be printed, e.g. on error). Splitting
/// this out keeps `cmd_doctor` a thin I/O wrapper.
fn run_doctor(args: &[String], hook_script: &str) -> (i32, Option<serde_json::Value>) {
    match args.len() {
        0 => (0, Some(build_doctor_output(hook_script))),
        1 => match build_agent_snippet(&args[0], hook_script) {
            Some(snippet) => (0, Some(snippet)),
            None => {
                eprintln!(
                    "error: unknown agent '{}' (expected 'claude' or 'codex')",
                    args[0]
                );
                (2, None)
            }
        },
        _ => {
            eprintln!("usage: tmux-agent-sidebar doctor [claude|codex]");
            (2, None)
        }
    }
}

pub(crate) fn cmd_doctor(args: &[String]) -> i32 {
    let hook_script = resolve_hook_script();
    let (code, json) = run_doctor(args, &hook_script);
    if let Some(v) = json {
        match serde_json::to_string_pretty(&v) {
            Ok(s) => println!("{}", s),
            Err(e) => {
                eprintln!("error: failed to serialize doctor output: {}", e);
                return 1;
            }
        }
    }
    code
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    const FAKE_HOOK: &str = "/fake/hook.sh";

    #[test]
    fn snippet_unknown_agent_returns_none() {
        assert!(build_agent_snippet("not-an-agent", FAKE_HOOK).is_none());
    }

    #[test]
    fn snippet_claude_has_hooks_key() {
        let v = build_agent_snippet("claude", FAKE_HOOK).unwrap();
        assert!(v.get("hooks").is_some(), "missing top-level hooks key");
        assert!(v.get("hooks").unwrap().is_object());
    }

    #[test]
    fn snippet_claude_covers_every_registration() {
        let v = build_agent_snippet("claude", FAKE_HOOK).unwrap();
        let hooks = v.get("hooks").unwrap().as_object().unwrap();
        let mut expected_triggers: Vec<&str> = ClaudeAdapter::HOOK_REGISTRATIONS
            .iter()
            .map(|r| r.trigger)
            .collect();
        expected_triggers.sort();
        expected_triggers.dedup();
        let mut actual_triggers: Vec<&str> = hooks.keys().map(String::as_str).collect();
        actual_triggers.sort();
        assert_eq!(actual_triggers, expected_triggers);
    }

    #[test]
    fn snippet_claude_session_start_has_correct_shape() {
        let v = build_agent_snippet("claude", FAKE_HOOK).unwrap();
        let entries = v
            .pointer("/hooks/SessionStart")
            .and_then(Value::as_array)
            .expect("SessionStart should be an array");
        assert_eq!(entries.len(), 1);
        let entry = &entries[0];
        assert_eq!(entry.get("matcher"), Some(&json!("")));
        let inner = entry
            .get("hooks")
            .and_then(Value::as_array)
            .expect("inner hooks array");
        assert_eq!(inner.len(), 1);
        assert_eq!(inner[0].get("type"), Some(&json!("command")));
        assert_eq!(
            inner[0].get("command"),
            Some(&json!("bash /fake/hook.sh claude session-start"))
        );
    }

    #[test]
    fn snippet_claude_post_tool_use_maps_to_activity_log() {
        let v = build_agent_snippet("claude", FAKE_HOOK).unwrap();
        let cmd = v
            .pointer("/hooks/PostToolUse/0/hooks/0/command")
            .and_then(Value::as_str)
            .unwrap();
        assert_eq!(cmd, "bash /fake/hook.sh claude activity-log");
    }

    #[test]
    fn snippet_codex_session_start_has_custom_matcher() {
        let v = build_agent_snippet("codex", FAKE_HOOK).unwrap();
        let entry = v
            .pointer("/hooks/SessionStart/0")
            .expect("codex SessionStart entry");
        assert_eq!(entry.get("matcher"), Some(&json!("startup|resume")));
        assert_eq!(
            entry
                .pointer("/hooks/0/command")
                .and_then(Value::as_str)
                .unwrap(),
            "bash /fake/hook.sh codex session-start"
        );
    }

    #[test]
    fn snippet_codex_non_session_start_has_empty_matcher() {
        let v = build_agent_snippet("codex", FAKE_HOOK).unwrap();
        for reg in CodexAdapter::HOOK_REGISTRATIONS {
            if reg.trigger == "SessionStart" {
                continue;
            }
            let entry = v
                .pointer(&format!("/hooks/{}/0", reg.trigger))
                .unwrap_or_else(|| panic!("missing codex trigger {}", reg.trigger));
            assert_eq!(
                entry.get("matcher"),
                Some(&json!("")),
                "{} should have empty matcher",
                reg.trigger
            );
        }
    }

    #[test]
    fn full_output_has_expected_top_level_keys() {
        let v = build_doctor_output(FAKE_HOOK);
        assert_eq!(
            v.get("version").and_then(Value::as_str),
            Some(crate::VERSION)
        );
        assert_eq!(
            v.get("hook_script").and_then(Value::as_str),
            Some(FAKE_HOOK)
        );
        let agents = v.get("agents").and_then(Value::as_object).unwrap();
        let mut keys: Vec<&str> = agents.keys().map(String::as_str).collect();
        keys.sort();
        assert_eq!(keys, vec!["claude", "codex"]);
    }

    #[test]
    fn full_output_snippet_matches_single_agent_snippet() {
        let full = build_doctor_output(FAKE_HOOK);
        for agent in ["claude", "codex"] {
            let from_full = full
                .pointer(&format!("/agents/{}/snippet", agent))
                .unwrap_or_else(|| panic!("missing snippet for {}", agent));
            let from_single = build_agent_snippet(agent, FAKE_HOOK).unwrap();
            assert_eq!(from_full, &from_single, "drift for {}", agent);
        }
    }

    #[test]
    fn full_output_normalized_hooks_count_matches_table() {
        let full = build_doctor_output(FAKE_HOOK);
        for (agent, table_len) in [
            ("claude", ClaudeAdapter::HOOK_REGISTRATIONS.len()),
            ("codex", CodexAdapter::HOOK_REGISTRATIONS.len()),
        ] {
            let hooks = full
                .pointer(&format!("/agents/{}/hooks", agent))
                .and_then(Value::as_array)
                .unwrap_or_else(|| panic!("missing hooks array for {}", agent));
            assert_eq!(
                hooks.len(),
                table_len,
                "normalized hooks[] length must match HOOK_REGISTRATIONS for {}",
                agent
            );
        }
    }

    #[test]
    fn full_output_normalized_entry_shape() {
        let full = build_doctor_output(FAKE_HOOK);
        let first = full.pointer("/agents/claude/hooks/0").unwrap();
        assert_eq!(first.get("trigger"), Some(&json!("SessionStart")));
        assert_eq!(first.get("matcher"), Some(&Value::Null));
        assert_eq!(first.get("event"), Some(&json!("session-start")));
        assert_eq!(
            first.get("command"),
            Some(&json!("bash /fake/hook.sh claude session-start"))
        );

        let codex_ss = full.pointer("/agents/codex/hooks/0").unwrap();
        assert_eq!(codex_ss.get("trigger"), Some(&json!("SessionStart")));
        assert_eq!(codex_ss.get("matcher"), Some(&json!("startup|resume")));
    }

    #[test]
    fn full_output_config_paths() {
        let full = build_doctor_output(FAKE_HOOK);
        assert_eq!(
            full.pointer("/agents/claude/config_path")
                .and_then(Value::as_str),
            Some("~/.claude/settings.json")
        );
        assert_eq!(
            full.pointer("/agents/codex/config_path")
                .and_then(Value::as_str),
            Some("~/.codex/hooks.json")
        );
    }

    #[test]
    fn run_doctor_no_args_returns_full_output() {
        let (code, json) = run_doctor(&[], FAKE_HOOK);
        assert_eq!(code, 0);
        assert!(json.unwrap().get("agents").is_some());
    }

    #[test]
    fn run_doctor_claude_returns_only_snippet() {
        let (code, json) = run_doctor(&["claude".to_string()], FAKE_HOOK);
        assert_eq!(code, 0);
        let v = json.unwrap();
        assert!(v.get("hooks").is_some());
        assert!(v.get("version").is_none());
        assert!(v.get("hook_script").is_none());
        assert!(v.get("agents").is_none());
    }

    #[test]
    fn run_doctor_codex_returns_only_snippet() {
        let (code, json) = run_doctor(&["codex".to_string()], FAKE_HOOK);
        assert_eq!(code, 0);
        let v = json.unwrap();
        assert!(v.get("hooks").is_some());
        assert!(v.get("version").is_none());
    }

    #[test]
    fn run_doctor_unknown_agent_returns_err_exit_2() {
        let (code, json) = run_doctor(&["gemini".to_string()], FAKE_HOOK);
        assert_eq!(code, 2);
        assert!(json.is_none());
    }

    #[test]
    fn run_doctor_too_many_args_returns_err_exit_2() {
        let (code, json) = run_doctor(&["claude".to_string(), "extra".to_string()], FAKE_HOOK);
        assert_eq!(code, 2);
        assert!(json.is_none());
    }

    #[test]
    fn full_output_snapshot() {
        let v = build_doctor_output(FAKE_HOOK);
        let actual = serde_json::to_string_pretty(&v).unwrap();
        // Version-independent snapshot: substitute the placeholder at test
        // time so a version bump in Cargo.toml does not break this test.
        // When adapter tables legitimately change, temporarily add a
        // `println!` to inspect the new output and update the literal below.
        let expected = EXPECTED_FULL_OUTPUT.replace("__VERSION__", crate::VERSION);
        assert_eq!(
            actual, expected,
            "doctor full output changed; update EXPECTED_FULL_OUTPUT in the \
             same commit that changes HOOK_REGISTRATIONS"
        );
    }

    const EXPECTED_FULL_OUTPUT: &str = r#"{
  "agents": {
    "claude": {
      "config_path": "~/.claude/settings.json",
      "hooks": [
        {
          "command": "bash /fake/hook.sh claude session-start",
          "event": "session-start",
          "matcher": null,
          "trigger": "SessionStart"
        },
        {
          "command": "bash /fake/hook.sh claude session-end",
          "event": "session-end",
          "matcher": null,
          "trigger": "SessionEnd"
        },
        {
          "command": "bash /fake/hook.sh claude user-prompt-submit",
          "event": "user-prompt-submit",
          "matcher": null,
          "trigger": "UserPromptSubmit"
        },
        {
          "command": "bash /fake/hook.sh claude notification",
          "event": "notification",
          "matcher": null,
          "trigger": "Notification"
        },
        {
          "command": "bash /fake/hook.sh claude stop",
          "event": "stop",
          "matcher": null,
          "trigger": "Stop"
        },
        {
          "command": "bash /fake/hook.sh claude stop-failure",
          "event": "stop-failure",
          "matcher": null,
          "trigger": "StopFailure"
        },
        {
          "command": "bash /fake/hook.sh claude permission-denied",
          "event": "permission-denied",
          "matcher": null,
          "trigger": "PermissionDenied"
        },
        {
          "command": "bash /fake/hook.sh claude cwd-changed",
          "event": "cwd-changed",
          "matcher": null,
          "trigger": "CwdChanged"
        },
        {
          "command": "bash /fake/hook.sh claude subagent-start",
          "event": "subagent-start",
          "matcher": null,
          "trigger": "SubagentStart"
        },
        {
          "command": "bash /fake/hook.sh claude subagent-stop",
          "event": "subagent-stop",
          "matcher": null,
          "trigger": "SubagentStop"
        },
        {
          "command": "bash /fake/hook.sh claude activity-log",
          "event": "activity-log",
          "matcher": null,
          "trigger": "PostToolUse"
        },
        {
          "command": "bash /fake/hook.sh claude task-created",
          "event": "task-created",
          "matcher": null,
          "trigger": "TaskCreated"
        },
        {
          "command": "bash /fake/hook.sh claude task-completed",
          "event": "task-completed",
          "matcher": null,
          "trigger": "TaskCompleted"
        },
        {
          "command": "bash /fake/hook.sh claude teammate-idle",
          "event": "teammate-idle",
          "matcher": null,
          "trigger": "TeammateIdle"
        },
        {
          "command": "bash /fake/hook.sh claude worktree-create",
          "event": "worktree-create",
          "matcher": null,
          "trigger": "WorktreeCreate"
        },
        {
          "command": "bash /fake/hook.sh claude worktree-remove",
          "event": "worktree-remove",
          "matcher": null,
          "trigger": "WorktreeRemove"
        }
      ],
      "snippet": {
        "hooks": {
          "CwdChanged": [
            {
              "hooks": [
                {
                  "command": "bash /fake/hook.sh claude cwd-changed",
                  "type": "command"
                }
              ],
              "matcher": ""
            }
          ],
          "Notification": [
            {
              "hooks": [
                {
                  "command": "bash /fake/hook.sh claude notification",
                  "type": "command"
                }
              ],
              "matcher": ""
            }
          ],
          "PermissionDenied": [
            {
              "hooks": [
                {
                  "command": "bash /fake/hook.sh claude permission-denied",
                  "type": "command"
                }
              ],
              "matcher": ""
            }
          ],
          "PostToolUse": [
            {
              "hooks": [
                {
                  "command": "bash /fake/hook.sh claude activity-log",
                  "type": "command"
                }
              ],
              "matcher": ""
            }
          ],
          "SessionEnd": [
            {
              "hooks": [
                {
                  "command": "bash /fake/hook.sh claude session-end",
                  "type": "command"
                }
              ],
              "matcher": ""
            }
          ],
          "SessionStart": [
            {
              "hooks": [
                {
                  "command": "bash /fake/hook.sh claude session-start",
                  "type": "command"
                }
              ],
              "matcher": ""
            }
          ],
          "Stop": [
            {
              "hooks": [
                {
                  "command": "bash /fake/hook.sh claude stop",
                  "type": "command"
                }
              ],
              "matcher": ""
            }
          ],
          "StopFailure": [
            {
              "hooks": [
                {
                  "command": "bash /fake/hook.sh claude stop-failure",
                  "type": "command"
                }
              ],
              "matcher": ""
            }
          ],
          "SubagentStart": [
            {
              "hooks": [
                {
                  "command": "bash /fake/hook.sh claude subagent-start",
                  "type": "command"
                }
              ],
              "matcher": ""
            }
          ],
          "SubagentStop": [
            {
              "hooks": [
                {
                  "command": "bash /fake/hook.sh claude subagent-stop",
                  "type": "command"
                }
              ],
              "matcher": ""
            }
          ],
          "TaskCompleted": [
            {
              "hooks": [
                {
                  "command": "bash /fake/hook.sh claude task-completed",
                  "type": "command"
                }
              ],
              "matcher": ""
            }
          ],
          "TaskCreated": [
            {
              "hooks": [
                {
                  "command": "bash /fake/hook.sh claude task-created",
                  "type": "command"
                }
              ],
              "matcher": ""
            }
          ],
          "TeammateIdle": [
            {
              "hooks": [
                {
                  "command": "bash /fake/hook.sh claude teammate-idle",
                  "type": "command"
                }
              ],
              "matcher": ""
            }
          ],
          "UserPromptSubmit": [
            {
              "hooks": [
                {
                  "command": "bash /fake/hook.sh claude user-prompt-submit",
                  "type": "command"
                }
              ],
              "matcher": ""
            }
          ],
          "WorktreeCreate": [
            {
              "hooks": [
                {
                  "command": "bash /fake/hook.sh claude worktree-create",
                  "type": "command"
                }
              ],
              "matcher": ""
            }
          ],
          "WorktreeRemove": [
            {
              "hooks": [
                {
                  "command": "bash /fake/hook.sh claude worktree-remove",
                  "type": "command"
                }
              ],
              "matcher": ""
            }
          ]
        }
      }
    },
    "codex": {
      "config_path": "~/.codex/hooks.json",
      "hooks": [
        {
          "command": "bash /fake/hook.sh codex session-start",
          "event": "session-start",
          "matcher": "startup|resume",
          "trigger": "SessionStart"
        },
        {
          "command": "bash /fake/hook.sh codex session-end",
          "event": "session-end",
          "matcher": null,
          "trigger": "SessionEnd"
        },
        {
          "command": "bash /fake/hook.sh codex user-prompt-submit",
          "event": "user-prompt-submit",
          "matcher": null,
          "trigger": "UserPromptSubmit"
        },
        {
          "command": "bash /fake/hook.sh codex stop",
          "event": "stop",
          "matcher": null,
          "trigger": "Stop"
        },
        {
          "command": "bash /fake/hook.sh codex activity-log",
          "event": "activity-log",
          "matcher": null,
          "trigger": "PostToolUse"
        }
      ],
      "snippet": {
        "hooks": {
          "PostToolUse": [
            {
              "hooks": [
                {
                  "command": "bash /fake/hook.sh codex activity-log",
                  "type": "command"
                }
              ],
              "matcher": ""
            }
          ],
          "SessionEnd": [
            {
              "hooks": [
                {
                  "command": "bash /fake/hook.sh codex session-end",
                  "type": "command"
                }
              ],
              "matcher": ""
            }
          ],
          "SessionStart": [
            {
              "hooks": [
                {
                  "command": "bash /fake/hook.sh codex session-start",
                  "type": "command"
                }
              ],
              "matcher": "startup|resume"
            }
          ],
          "Stop": [
            {
              "hooks": [
                {
                  "command": "bash /fake/hook.sh codex stop",
                  "type": "command"
                }
              ],
              "matcher": ""
            }
          ],
          "UserPromptSubmit": [
            {
              "hooks": [
                {
                  "command": "bash /fake/hook.sh codex user-prompt-submit",
                  "type": "command"
                }
              ],
              "matcher": ""
            }
          ]
        }
      }
    }
  },
  "hook_script": "/fake/hook.sh",
  "version": "__VERSION__"
}"#;

    #[test]
    fn full_output_normalized_command_matches_snippet_command() {
        let full = build_doctor_output(FAKE_HOOK);
        for agent in ["claude", "codex"] {
            let hooks = full
                .pointer(&format!("/agents/{}/hooks", agent))
                .and_then(Value::as_array)
                .unwrap();
            for entry in hooks {
                let trigger = entry.get("trigger").and_then(Value::as_str).unwrap();
                let command = entry.get("command").and_then(Value::as_str).unwrap();
                let group = full
                    .pointer(&format!("/agents/{}/snippet/hooks/{}", agent, trigger))
                    .and_then(Value::as_array)
                    .unwrap_or_else(|| panic!("snippet missing trigger {} for {}", trigger, agent));
                let found = group.iter().any(|slot: &Value| {
                    slot.pointer("/hooks/0/command")
                        .and_then(Value::as_str)
                        .map(|c| c == command)
                        .unwrap_or(false)
                });
                assert!(
                    found,
                    "command {:?} missing from snippet of {}::{}",
                    command, agent, trigger
                );
            }
        }
    }
}
