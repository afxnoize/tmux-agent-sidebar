use crate::event::{AgentEvent, EventAdapter};
use serde_json::Value;

use super::json_str;

fn parse_json_field(input: &Value, field: &str) -> Value {
    input
        .get(field)
        .and_then(|v| {
            if let Some(s) = v.as_str() {
                serde_json::from_str(s).ok()
            } else if v.is_object() {
                Some(v.clone())
            } else {
                None
            }
        })
        .unwrap_or(Value::Null)
}

pub struct ClaudeAdapter;

impl EventAdapter for ClaudeAdapter {
    fn parse(&self, event_name: &str, input: &Value) -> Option<AgentEvent> {
        match event_name {
            "session-start" => Some(AgentEvent::SessionStart {
                agent: "claude".into(),
                cwd: json_str(input, "cwd").into(),
                permission_mode: json_str(input, "permission_mode").into(),
                worktree: None,
                agent_id: None,
            }),
            "session-end" => Some(AgentEvent::SessionEnd),
            "user-prompt-submit" => Some(AgentEvent::UserPromptSubmit {
                agent: "claude".into(),
                cwd: json_str(input, "cwd").into(),
                permission_mode: json_str(input, "permission_mode").into(),
                prompt: json_str(input, "prompt").into(),
                worktree: None,
                agent_id: None,
            }),
            "notification" => {
                let wait_reason = json_str(input, "notification_type");
                let meta_only = wait_reason == "idle_prompt";
                Some(AgentEvent::Notification {
                    agent: "claude".into(),
                    cwd: json_str(input, "cwd").into(),
                    permission_mode: json_str(input, "permission_mode").into(),
                    wait_reason: wait_reason.into(),
                    meta_only,
                    worktree: None,
                    agent_id: None,
                })
            }
            "stop" => Some(AgentEvent::Stop {
                agent: "claude".into(),
                cwd: json_str(input, "cwd").into(),
                permission_mode: json_str(input, "permission_mode").into(),
                last_message: json_str(input, "last_assistant_message").into(),
                response: None,
                worktree: None,
                agent_id: None,
            }),
            "stop-failure" => {
                let error_type = json_str(input, "error");
                let error_details = json_str(input, "error_details");
                let error = if !error_type.is_empty() {
                    error_type
                } else {
                    error_details
                };
                Some(AgentEvent::StopFailure {
                    agent: "claude".into(),
                    cwd: json_str(input, "cwd").into(),
                    permission_mode: json_str(input, "permission_mode").into(),
                    error: error.into(),
                    worktree: None,
                    agent_id: None,
                })
            }
            "subagent-start" => {
                let agent_type = json_str(input, "agent_type");
                if agent_type.is_empty() {
                    return None;
                }
                Some(AgentEvent::SubagentStart {
                    agent_type: agent_type.into(),
                })
            }
            "subagent-stop" => {
                let agent_type = json_str(input, "agent_type");
                if agent_type.is_empty() {
                    return None;
                }
                Some(AgentEvent::SubagentStop {
                    agent_type: agent_type.into(),
                })
            }
            "activity-log" => {
                let tool_name = json_str(input, "tool_name");
                if tool_name.is_empty() {
                    return None;
                }
                Some(AgentEvent::ActivityLog {
                    tool_name: tool_name.into(),
                    tool_input: parse_json_field(input, "tool_input"),
                    tool_response: parse_json_field(input, "tool_response"),
                })
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn session_start() {
        let adapter = ClaudeAdapter;
        let input = json!({"cwd": "/home/user", "permission_mode": "default"});
        let event = adapter.parse("session-start", &input).unwrap();
        assert_eq!(
            event,
            AgentEvent::SessionStart {
                agent: "claude".into(),
                cwd: "/home/user".into(),
                permission_mode: "default".into(),
                worktree: None,
                agent_id: None,
            }
        );
    }

    #[test]
    fn session_end() {
        let adapter = ClaudeAdapter;
        assert_eq!(
            adapter.parse("session-end", &json!({})).unwrap(),
            AgentEvent::SessionEnd
        );
    }

    #[test]
    fn user_prompt_submit() {
        let adapter = ClaudeAdapter;
        let input = json!({"cwd": "/tmp", "permission_mode": "auto", "prompt": "fix bug"});
        let event = adapter.parse("user-prompt-submit", &input).unwrap();
        assert_eq!(
            event,
            AgentEvent::UserPromptSubmit {
                agent: "claude".into(),
                cwd: "/tmp".into(),
                permission_mode: "auto".into(),
                prompt: "fix bug".into(),
                worktree: None,
                agent_id: None,
            }
        );
    }

    #[test]
    fn notification() {
        let adapter = ClaudeAdapter;
        let input =
            json!({"cwd": "/tmp", "permission_mode": "default", "notification_type": "permission"});
        let event = adapter.parse("notification", &input).unwrap();
        assert_eq!(
            event,
            AgentEvent::Notification {
                agent: "claude".into(),
                cwd: "/tmp".into(),
                permission_mode: "default".into(),
                wait_reason: "permission".into(),
                meta_only: false,
                worktree: None,
                agent_id: None,
            }
        );
    }

    #[test]
    fn notification_idle_prompt_is_meta_only() {
        let adapter = ClaudeAdapter;
        let input = json!({"cwd": "/tmp", "permission_mode": "default", "notification_type": "idle_prompt"});
        let event = adapter.parse("notification", &input).unwrap();
        assert_eq!(
            event,
            AgentEvent::Notification {
                agent: "claude".into(),
                cwd: "/tmp".into(),
                permission_mode: "default".into(),
                wait_reason: "idle_prompt".into(),
                meta_only: true,
                worktree: None,
                agent_id: None,
            }
        );
    }

    #[test]
    fn stop() {
        let adapter = ClaudeAdapter;
        let input =
            json!({"cwd": "/tmp", "permission_mode": "default", "last_assistant_message": "done"});
        let event = adapter.parse("stop", &input).unwrap();
        assert_eq!(
            event,
            AgentEvent::Stop {
                agent: "claude".into(),
                cwd: "/tmp".into(),
                permission_mode: "default".into(),
                last_message: "done".into(),
                response: None,
                worktree: None,
                agent_id: None,
            }
        );
    }

    #[test]
    fn stop_failure_error_field() {
        let adapter = ClaudeAdapter;
        let input = json!({"cwd": "/tmp", "permission_mode": "default", "error": "rate_limit", "error_details": "too many"});
        let event = adapter.parse("stop-failure", &input).unwrap();
        assert_eq!(
            event,
            AgentEvent::StopFailure {
                agent: "claude".into(),
                cwd: "/tmp".into(),
                permission_mode: "default".into(),
                error: "rate_limit".into(),
                worktree: None,
                agent_id: None,
            }
        );
    }

    #[test]
    fn stop_failure_falls_back_to_error_details() {
        let adapter = ClaudeAdapter;
        let input = json!({"cwd": "/tmp", "permission_mode": "default", "error_details": "something went wrong"});
        let event = adapter.parse("stop-failure", &input).unwrap();
        assert_eq!(
            event,
            AgentEvent::StopFailure {
                agent: "claude".into(),
                cwd: "/tmp".into(),
                permission_mode: "default".into(),
                error: "something went wrong".into(),
                worktree: None,
                agent_id: None,
            }
        );
    }

    #[test]
    fn subagent_start() {
        let adapter = ClaudeAdapter;
        let input = json!({"agent_type": "Explore"});
        assert_eq!(
            adapter.parse("subagent-start", &input).unwrap(),
            AgentEvent::SubagentStart {
                agent_type: "Explore".into()
            }
        );
    }

    #[test]
    fn subagent_start_empty_type_ignored() {
        let adapter = ClaudeAdapter;
        assert!(adapter.parse("subagent-start", &json!({})).is_none());
    }

    #[test]
    fn subagent_stop() {
        let adapter = ClaudeAdapter;
        let input = json!({"agent_type": "Plan"});
        assert_eq!(
            adapter.parse("subagent-stop", &input).unwrap(),
            AgentEvent::SubagentStop {
                agent_type: "Plan".into()
            }
        );
    }

    #[test]
    fn activity_log() {
        let adapter = ClaudeAdapter;
        let input = json!({"tool_name": "Read", "tool_input": {"file_path": "/a/b.rs"}});
        let event = adapter.parse("activity-log", &input).unwrap();
        assert_eq!(
            event,
            AgentEvent::ActivityLog {
                tool_name: "Read".into(),
                tool_input: json!({"file_path": "/a/b.rs"}),
                tool_response: Value::Null,
            }
        );
    }

    #[test]
    fn activity_log_string_tool_input() {
        let adapter = ClaudeAdapter;
        let input = json!({"tool_name": "Edit", "tool_input": "{\"file_path\":\"/a/b.rs\"}"});
        let event = adapter.parse("activity-log", &input).unwrap();
        assert_eq!(
            event,
            AgentEvent::ActivityLog {
                tool_name: "Edit".into(),
                tool_input: json!({"file_path": "/a/b.rs"}),
                tool_response: Value::Null,
            }
        );
    }

    #[test]
    fn activity_log_empty_tool_name_ignored() {
        let adapter = ClaudeAdapter;
        assert!(adapter.parse("activity-log", &json!({})).is_none());
    }

    #[test]
    fn unknown_event_ignored() {
        let adapter = ClaudeAdapter;
        assert!(adapter.parse("unknown-event", &json!({})).is_none());
    }

    #[test]
    fn subagent_stop_empty_type_ignored() {
        let adapter = ClaudeAdapter;
        assert!(adapter.parse("subagent-stop", &json!({})).is_none());
    }

    #[test]
    fn notification_empty_reason() {
        let adapter = ClaudeAdapter;
        let input = json!({"cwd": "/tmp", "permission_mode": "default"});
        let event = adapter.parse("notification", &input).unwrap();
        assert_eq!(
            event,
            AgentEvent::Notification {
                agent: "claude".into(),
                cwd: "/tmp".into(),
                permission_mode: "default".into(),
                wait_reason: "".into(),
                meta_only: false,
                worktree: None,
                agent_id: None,
            }
        );
    }

    #[test]
    fn stop_failure_both_empty() {
        let adapter = ClaudeAdapter;
        let input = json!({"cwd": "/tmp", "permission_mode": "default"});
        let event = adapter.parse("stop-failure", &input).unwrap();
        assert_eq!(
            event,
            AgentEvent::StopFailure {
                agent: "claude".into(),
                cwd: "/tmp".into(),
                permission_mode: "default".into(),
                error: "".into(),
                worktree: None,
                agent_id: None,
            }
        );
    }

    #[test]
    fn stop_empty_last_message() {
        let adapter = ClaudeAdapter;
        let input = json!({"cwd": "/tmp", "permission_mode": "default"});
        let event = adapter.parse("stop", &input).unwrap();
        assert_eq!(
            event,
            AgentEvent::Stop {
                agent: "claude".into(),
                cwd: "/tmp".into(),
                permission_mode: "default".into(),
                last_message: "".into(),
                response: None,
                worktree: None,
                agent_id: None,
            }
        );
    }

    #[test]
    fn session_start_missing_fields_default_to_empty() {
        let adapter = ClaudeAdapter;
        let event = adapter.parse("session-start", &json!({})).unwrap();
        assert_eq!(
            event,
            AgentEvent::SessionStart {
                agent: "claude".into(),
                cwd: "".into(),
                permission_mode: "".into(),
                worktree: None,
                agent_id: None,
            }
        );
    }

    #[test]
    fn activity_log_with_tool_response() {
        let adapter = ClaudeAdapter;
        let input = json!({
            "tool_name": "TaskCreate",
            "tool_input": {"subject": "Fix bug"},
            "tool_response": {"task": {"id": "42"}}
        });
        let event = adapter.parse("activity-log", &input).unwrap();
        assert_eq!(
            event,
            AgentEvent::ActivityLog {
                tool_name: "TaskCreate".into(),
                tool_input: json!({"subject": "Fix bug"}),
                tool_response: json!({"task": {"id": "42"}}),
            }
        );
    }
}
