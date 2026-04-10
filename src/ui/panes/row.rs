use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

use crate::tmux::PaneStatus;
use crate::ui::colors::ColorTheme;
use crate::ui::icons::StatusIcons;
use crate::ui::text::{
    display_width, elapsed_label, pad_to, truncate_to_width, wait_reason_label, wrap_text,
    wrap_text_char,
};

/// Shared context passed to each row-rendering helper.
struct RowCtx<'a> {
    border_style: Style,
    inner_width: usize,
    theme: &'a ColorTheme,
    apply_bg: &'a dyn Fn(Style) -> Style,
    active: bool,
}

fn bordered_line(
    border_style: Style,
    apply_bg: &dyn Fn(Style) -> Style,
    inner_width: usize,
    content_spans: Vec<Span<'static>>,
    content_width: usize,
) -> Line<'static> {
    let padding = pad_to(content_width, inner_width);
    let mut spans = vec![
        Span::styled("│", border_style),
        Span::styled(" ", apply_bg(Style::default())),
    ];
    spans.extend(content_spans);
    spans.push(Span::styled(padding, apply_bg(Style::default())));
    spans.push(Span::styled("│", border_style));
    Line::from(spans)
}

fn bordered_split_line(
    border_style: Style,
    apply_bg: &dyn Fn(Style) -> Style,
    inner_width: usize,
    left_spans: Vec<Span<'static>>,
    left_width: usize,
    right_spans: Vec<Span<'static>>,
    right_width: usize,
) -> Line<'static> {
    let padding = inner_width.saturating_sub(left_width + right_width);
    let mut spans = vec![
        Span::styled("│", border_style),
        Span::styled(" ", apply_bg(Style::default())),
    ];
    spans.extend(left_spans);
    spans.push(Span::styled(
        " ".repeat(padding),
        apply_bg(Style::default()),
    ));
    spans.extend(right_spans);
    spans.push(Span::styled("│", border_style));
    Line::from(spans)
}

fn status_row(
    pane: &crate::tmux::PaneInfo,
    ctx: &RowCtx,
    icons: &StatusIcons,
    spinner_frame: usize,
    now: u64,
) -> Line<'static> {
    use crate::tmux::PermissionMode;
    let theme = ctx.theme;
    let apply_bg = ctx.apply_bg;

    let (icon, pulse_color) = running_icon_for(&pane.status, spinner_frame, icons);
    let icon_color =
        pulse_color.unwrap_or_else(|| theme.status_color(&pane.status, pane.attention));
    let title = pane.agent.label();
    let badge = pane.permission_mode.badge();
    let elapsed = elapsed_label(pane.started_at, now);

    let title_fg = theme.agent_color(&pane.agent);
    let is_active_status = matches!(pane.status, PaneStatus::Running | PaneStatus::Waiting);
    let elapsed_fg = if is_active_status {
        theme.text_active
    } else {
        theme.text_muted
    };
    let active_mod = if ctx.active {
        Modifier::BOLD
    } else {
        Modifier::empty()
    };

    let badge_extra = if badge.is_empty() { 0 } else { 1 };
    let left_dw =
        display_width(icon) + 1 + display_width(title) + badge_extra + display_width(badge);
    let available_for_elapsed = ctx.inner_width.saturating_sub(left_dw);
    let elapsed = truncate_to_width(&elapsed, available_for_elapsed);
    let elapsed_dw = display_width(&elapsed);
    let padding = pad_to(left_dw + elapsed_dw, ctx.inner_width);

    let mut spans: Vec<Span<'static>> = vec![
        Span::styled("│", ctx.border_style),
        Span::styled(" ", apply_bg(Style::default())),
        Span::styled(icon.to_string(), apply_bg(Style::default().fg(icon_color))),
        Span::styled(
            format!(" {}", title),
            apply_bg(Style::default().fg(title_fg).add_modifier(active_mod)),
        ),
    ];
    if !badge.is_empty() {
        let badge_color = match pane.permission_mode {
            PermissionMode::BypassPermissions => theme.badge_danger,
            PermissionMode::Auto => theme.badge_auto,
            PermissionMode::Plan => theme.badge_plan,
            PermissionMode::AcceptEdits => theme.badge_auto,
            PermissionMode::Default => theme.text_muted,
        };
        spans.push(Span::styled(
            format!(" {}", badge),
            apply_bg(Style::default().fg(badge_color)),
        ));
    }
    spans.push(Span::styled(padding, apply_bg(Style::default())));
    spans.push(Span::styled(
        elapsed,
        apply_bg(Style::default().fg(elapsed_fg)),
    ));
    spans.push(Span::styled("│", ctx.border_style));
    Line::from(spans)
}

fn branch_ports_row(
    git_info: &crate::group::PaneGitInfo,
    ports: Option<&[u16]>,
    ctx: &RowCtx,
) -> Option<Line<'static>> {
    let branch = crate::ui::text::branch_label(git_info);
    let port_text = ports.and_then(|ports| {
        if ports.is_empty() {
            return None;
        }
        let mut port_list = String::new();
        for (i, port) in ports.iter().enumerate() {
            if i > 0 {
                port_list.push_str(", ");
            }
            port_list.push_str(&port.to_string());
        }
        Some(format!(":{}", port_list))
    });

    if branch.is_empty() && port_text.is_none() {
        return None;
    }

    let theme = ctx.theme;
    let apply_bg = ctx.apply_bg;
    let branch_color = theme.branch;

    let left_prefix = "  ";
    let right_prefix = "  ";
    let right_text = port_text.unwrap_or_default();
    let right_width = if right_text.is_empty() {
        0
    } else {
        display_width(right_prefix) + display_width(&right_text)
    };
    let left_room = ctx.inner_width.saturating_sub(right_width);
    let max_branch_width = left_room.saturating_sub(display_width(left_prefix));
    let truncated_branch = truncate_to_width(&branch, max_branch_width);
    let left_text = format!("{}{}", left_prefix, truncated_branch);
    let left_width = display_width(&left_text);

    let mut left_spans: Vec<Span<'static>> = vec![Span::styled(
        left_text,
        apply_bg(Style::default().fg(branch_color)),
    )];
    if branch.is_empty() {
        left_spans.clear();
    }
    let right_spans: Vec<Span<'static>> = if right_text.is_empty() {
        vec![]
    } else {
        vec![Span::styled(
            format!("{}{}", right_prefix, right_text),
            apply_bg(Style::default().fg(theme.port)),
        )]
    };
    let left_width = if branch.is_empty() { 0 } else { left_width };

    Some(bordered_split_line(
        ctx.border_style,
        apply_bg,
        ctx.inner_width,
        left_spans,
        left_width,
        right_spans,
        right_width,
    ))
}

fn task_progress_row(
    task_progress: Option<&crate::activity::TaskProgress>,
    ctx: &RowCtx,
) -> Option<Line<'static>> {
    use crate::activity::TaskStatus;
    let progress = task_progress?;
    if progress.is_empty() {
        return None;
    }

    let mut icons = String::new();
    for (_, status) in &progress.tasks {
        let ch = match status {
            TaskStatus::Completed => "✔",
            TaskStatus::InProgress => "◼",
            TaskStatus::Pending => "◻",
        };
        icons.push_str(ch);
    }
    let summary = format!(
        "  {} {}/{}",
        icons,
        progress.completed_count(),
        progress.total()
    );
    let summary_dw = display_width(&summary);
    let task_color = ctx.theme.task_progress;
    Some(bordered_line(
        ctx.border_style,
        ctx.apply_bg,
        ctx.inner_width,
        vec![Span::styled(
            summary,
            (ctx.apply_bg)(Style::default().fg(task_color)),
        )],
        summary_dw,
    ))
}

fn subagent_rows(subagents: &[String], ctx: &RowCtx) -> Vec<Line<'static>> {
    let mut out = Vec::new();
    if subagents.is_empty() {
        return out;
    }
    let theme = ctx.theme;
    let apply_bg = ctx.apply_bg;
    let subagent_color = theme.subagent;
    let tree_color = theme.text_muted;
    let last_idx = subagents.len() - 1;
    for (i, sa) in subagents.iter().enumerate() {
        let connector = if i == last_idx { "└ " } else { "├ " };
        let numbered = if sa.contains('#') {
            sa.clone()
        } else {
            format!("{} #{}", sa, i + 1)
        };
        let prefix = format!("  {}", connector);
        let prefix_dw = display_width(&prefix);
        let max_sa_w = ctx.inner_width.saturating_sub(prefix_dw);
        let truncated_sa = truncate_to_width(&numbered, max_sa_w);
        let text_dw = prefix_dw + display_width(&truncated_sa);
        out.push(bordered_line(
            ctx.border_style,
            apply_bg,
            ctx.inner_width,
            vec![
                Span::styled(prefix, apply_bg(Style::default().fg(tree_color))),
                Span::styled(truncated_sa, apply_bg(Style::default().fg(subagent_color))),
            ],
            text_dw,
        ));
    }
    out
}

fn wait_reason_row(wait_reason: &str, status: &PaneStatus, ctx: &RowCtx) -> Option<Line<'static>> {
    if wait_reason.is_empty() {
        return None;
    }
    let reason = wait_reason_label(wait_reason);
    let text = format!("  {}", reason);
    let text_dw = display_width(&text);
    let reason_color = if matches!(status, PaneStatus::Error) {
        ctx.theme.status_error
    } else {
        ctx.theme.wait_reason
    };
    Some(bordered_line(
        ctx.border_style,
        ctx.apply_bg,
        ctx.inner_width,
        vec![Span::styled(
            text,
            (ctx.apply_bg)(Style::default().fg(reason_color)),
        )],
        text_dw,
    ))
}

fn prompt_rows(pane: &crate::tmux::PaneInfo, ctx: &RowCtx) -> Vec<Line<'static>> {
    let mut out = Vec::new();
    let apply_bg = ctx.apply_bg;
    let theme = ctx.theme;

    let is_response = pane.prompt_is_response;
    let prompt_color = if ctx.active {
        theme.text_active
    } else {
        theme.text_muted
    };
    let display_prompt = pane.prompt.clone();
    let wrap_width = ctx
        .inner_width
        .saturating_sub(if is_response { 4 } else { 2 });
    let wrapped = if is_response {
        wrap_text_char(&display_prompt, wrap_width, 3)
    } else {
        wrap_text(&display_prompt, wrap_width, 3)
    };
    for (li, wl) in wrapped.iter().enumerate() {
        if is_response && li == 0 {
            let arrow_color = theme.response_arrow;
            let text_dw = 4 + display_width(wl); // "  ▶ " + text
            out.push(bordered_line(
                ctx.border_style,
                apply_bg,
                ctx.inner_width,
                vec![
                    Span::styled(
                        "  ▶ ",
                        apply_bg(
                            Style::default()
                                .fg(arrow_color)
                                .add_modifier(Modifier::BOLD),
                        ),
                    ),
                    Span::styled(wl.clone(), apply_bg(Style::default().fg(prompt_color))),
                ],
                text_dw,
            ));
        } else {
            let indent = if is_response { "    " } else { "  " };
            let text = format!("{}{}", indent, wl);
            let text_dw = display_width(&text);
            out.push(bordered_line(
                ctx.border_style,
                apply_bg,
                ctx.inner_width,
                vec![Span::styled(
                    text,
                    apply_bg(Style::default().fg(prompt_color)),
                )],
                text_dw,
            ));
        }
    }
    out
}

fn idle_hint_row(ctx: &RowCtx) -> Line<'static> {
    let text = "  Waiting for prompt…";
    let text_dw = display_width(text);
    let idle_color = if ctx.active {
        ctx.theme.text_active
    } else {
        ctx.theme.text_muted
    };
    bordered_line(
        ctx.border_style,
        ctx.apply_bg,
        ctx.inner_width,
        vec![Span::styled(
            text.to_string(),
            (ctx.apply_bg)(Style::default().fg(idle_color)),
        )],
        text_dw,
    )
}

#[allow(clippy::too_many_arguments)]
pub(super) fn render_pane_lines_with_ports(
    pane: &crate::tmux::PaneInfo,
    git_info: &crate::group::PaneGitInfo,
    ports: Option<&[u16]>,
    _command: Option<&str>,
    task_progress: Option<&crate::activity::TaskProgress>,
    selected: bool,
    active: bool,
    border_color: Color,
    width: usize,
    icons: &StatusIcons,
    theme: &ColorTheme,
    spinner_frame: usize,
    now: u64,
) -> Vec<Line<'static>> {
    let border_style = Style::default().fg(border_color);
    let inner_width = width.saturating_sub(3);
    let bg = if selected {
        Some(theme.selection_bg)
    } else {
        None
    };
    let apply_bg = move |s: Style| match bg {
        Some(c) => s.bg(c),
        None => s,
    };
    let ctx = RowCtx {
        border_style,
        inner_width,
        theme,
        apply_bg: &apply_bg,
        active,
    };

    let mut out: Vec<Line<'static>> = Vec::new();
    out.push(status_row(pane, &ctx, icons, spinner_frame, now));
    if let Some(line) = branch_ports_row(git_info, ports, &ctx) {
        out.push(line);
    }
    if let Some(line) = task_progress_row(task_progress, &ctx) {
        out.push(line);
    }
    out.extend(subagent_rows(&pane.subagents, &ctx));
    if let Some(line) = wait_reason_row(&pane.wait_reason, &pane.status, &ctx) {
        out.push(line);
    }
    if !pane.prompt.is_empty() {
        out.extend(prompt_rows(pane, &ctx));
    } else if matches!(pane.status, PaneStatus::Idle) {
        out.push(idle_hint_row(&ctx));
    }
    out
}

fn running_icon_for<'a>(
    status: &PaneStatus,
    spinner_frame: usize,
    icons: &'a StatusIcons,
) -> (&'a str, Option<Color>) {
    use crate::SPINNER_PULSE;

    match status {
        PaneStatus::Running => {
            let color_idx = SPINNER_PULSE[spinner_frame % SPINNER_PULSE.len()];
            (icons.status_icon(status), Some(Color::Indexed(color_idx)))
        }
        _ => (icons.status_icon(status), None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::group::PaneGitInfo;
    use crate::tmux::{AgentType, PaneInfo, PermissionMode};
    use crate::ui::icons::StatusIcons;

    fn pane(permission_mode: PermissionMode, status: PaneStatus, prompt: &str) -> PaneInfo {
        pane_with_response(permission_mode, status, prompt, false)
    }

    fn pane_with_response(
        permission_mode: PermissionMode,
        status: PaneStatus,
        prompt: &str,
        is_response: bool,
    ) -> PaneInfo {
        PaneInfo {
            pane_id: "%1".into(),
            pane_active: false,
            status,
            attention: false,
            agent: AgentType::Codex,
            path: "/tmp/project".into(),
            current_command: String::new(),
            prompt: prompt.into(),
            prompt_is_response: is_response,
            started_at: None,
            wait_reason: String::new(),
            permission_mode,
            subagents: vec![],
            pane_pid: None,
            worktree_name: String::new(),
            worktree_branch: String::new(),
        }
    }

    fn line_text(line: &Line<'_>) -> String {
        line.spans
            .iter()
            .map(|span| span.content.as_ref())
            .collect()
    }

    /// Convenience: build a RowCtx bound to a no-op background for unit tests.
    fn test_ctx<'a>(
        theme: &'a ColorTheme,
        inner_width: usize,
        active: bool,
        apply_bg: &'a dyn Fn(Style) -> Style,
    ) -> RowCtx<'a> {
        RowCtx {
            border_style: Style::default().fg(theme.border_active),
            inner_width,
            theme,
            apply_bg,
            active,
        }
    }

    #[test]
    fn render_pane_lines_shows_permission_badge() {
        let theme = ColorTheme::default();
        let pane = pane(PermissionMode::Auto, PaneStatus::Running, "");
        let lines = render_pane_lines_with_ports(
            &pane,
            &PaneGitInfo::default(),
            None,
            None,
            None,
            false,
            false,
            theme.border_active,
            40,
            &StatusIcons::default(),
            &theme,
            0,
            0,
        );

        let status = line_text(&lines[0]);
        assert!(status.contains(" codex auto"));
    }

    #[test]
    fn render_pane_lines_shows_branch_and_ports_on_same_row() {
        let theme = ColorTheme::default();
        let pane = pane(PermissionMode::Default, PaneStatus::Running, "");
        let ports = vec![3000, 5173];
        let lines = render_pane_lines_with_ports(
            &pane,
            &PaneGitInfo {
                repo_root: Some("/tmp/project".into()),
                branch: Some("feature/sidebar".into()),
                is_worktree: false,
                worktree_name: None,
            },
            Some(&ports),
            None,
            None,
            false,
            false,
            theme.border_active,
            40,
            &StatusIcons::default(),
            &theme,
            0,
            0,
        );

        assert!(lines.len() >= 2);
        let branch_port_line = line_text(&lines[1]);
        assert!(branch_port_line.contains("feature/sidebar"));
        assert!(branch_port_line.contains(":3000, 5173"));
        assert!(branch_port_line.find("feature/sidebar") < branch_port_line.find(":3000, 5173"));
    }

    #[test]
    fn render_pane_lines_shows_command_row() {
        let theme = ColorTheme::default();
        let mut pane = pane(PermissionMode::Default, PaneStatus::Running, "");
        pane.current_command = "npm run dev -- --port 3000".into();
        let lines = render_pane_lines_with_ports(
            &pane,
            &PaneGitInfo::default(),
            None,
            Some("npm run dev -- --port 3000"),
            None,
            false,
            false,
            theme.border_active,
            40,
            &StatusIcons::default(),
            &theme,
            0,
            0,
        );

        assert_eq!(lines.len(), 1);
        assert!(lines.iter().all(|line| !line_text(line).contains("cmd:")));
    }

    #[test]
    fn render_pane_lines_truncates_long_branch_when_ports_present() {
        let theme = ColorTheme::default();
        let pane = pane(PermissionMode::Default, PaneStatus::Running, "");
        let ports = vec![3000];
        let lines = render_pane_lines_with_ports(
            &pane,
            &PaneGitInfo {
                repo_root: Some("/tmp/project".into()),
                branch: Some("feature/sidebar/really-long-branch-name-that-should-truncate".into()),
                is_worktree: false,
                worktree_name: None,
            },
            Some(&ports),
            None,
            None,
            false,
            false,
            theme.border_active,
            40,
            &StatusIcons::default(),
            &theme,
            0,
            0,
        );

        assert!(lines.len() >= 2);
        let branch_port_line = line_text(&lines[1]);
        assert!(
            branch_port_line.contains('…'),
            "long branch should be truncated"
        );
        assert!(branch_port_line.contains(":3000"));
        assert!(
            branch_port_line.find('…') < branch_port_line.find(":3000"),
            "branch truncation should remain left of the port text"
        );
    }

    #[test]
    fn render_pane_lines_uses_injected_now_for_elapsed() {
        let theme = ColorTheme::default();
        let mut pane = pane(PermissionMode::Default, PaneStatus::Running, "");
        pane.started_at = Some(1_000_000 - 125);
        let lines = render_pane_lines_with_ports(
            &pane,
            &PaneGitInfo::default(),
            None,
            None,
            None,
            false,
            false,
            theme.border_active,
            40,
            &StatusIcons::default(),
            &theme,
            0,
            1_000_000,
        );

        let status = line_text(&lines[0]);
        assert!(status.contains("2m5s"));
    }

    #[test]
    fn running_icon_for_all_statuses() {
        let icons = StatusIcons::default();
        assert_eq!(running_icon_for(&PaneStatus::Idle, 0, &icons), ("○", None));
        assert_eq!(
            running_icon_for(&PaneStatus::Waiting, 0, &icons),
            ("◐", None)
        );
        assert_eq!(running_icon_for(&PaneStatus::Error, 0, &icons), ("✕", None));
        assert_eq!(
            running_icon_for(&PaneStatus::Unknown, 0, &icons),
            ("·", None)
        );

        let (icon, color) = running_icon_for(&PaneStatus::Running, 0, &icons);
        assert_eq!(icon, "●");
        assert_eq!(color, Some(Color::Indexed(82)));
    }

    #[test]
    fn render_pane_lines_shows_idle_prompt_hint() {
        let theme = ColorTheme::default();
        let pane = pane(PermissionMode::Default, PaneStatus::Idle, "");
        let lines = render_pane_lines_with_ports(
            &pane,
            &PaneGitInfo::default(),
            None,
            None,
            None,
            false,
            false,
            theme.border_active,
            40,
            &StatusIcons::default(),
            &theme,
            0,
            0,
        );

        assert_eq!(lines.len(), 2);
        let hint = line_text(&lines[1]);
        assert!(hint.contains("Waiting for prompt"));
    }

    #[test]
    fn render_pane_lines_wraps_prompt_when_present() {
        let theme = ColorTheme::default();
        let pane = pane(
            PermissionMode::BypassPermissions,
            PaneStatus::Idle,
            "hello world from codex",
        );
        let lines = render_pane_lines_with_ports(
            &pane,
            &PaneGitInfo::default(),
            None,
            None,
            None,
            false,
            false,
            theme.border_active,
            18,
            &StatusIcons::default(),
            &theme,
            0,
            0,
        );

        assert!(lines.len() >= 2);
        let status = line_text(&lines[0]);
        assert!(status.contains(" codex !"));
        assert!(!line_text(&lines[1]).contains("Waiting for prompt"));
    }

    #[test]
    fn render_pane_lines_shows_single_subagent() {
        let theme = ColorTheme::default();
        let mut p = pane(PermissionMode::Default, PaneStatus::Running, "test");
        p.subagents = vec!["Explore".into()];
        let lines = render_pane_lines_with_ports(
            &p,
            &PaneGitInfo::default(),
            None,
            None,
            None,
            false,
            false,
            theme.border_active,
            40,
            &StatusIcons::default(),
            &theme,
            0,
            0,
        );

        // status + subagent + prompt = 3 lines minimum
        assert!(lines.len() >= 3);
        let sub_line = line_text(&lines[1]);
        assert!(sub_line.contains("└ "));
        assert!(sub_line.contains("Explore #1"));
    }

    #[test]
    fn render_pane_lines_shows_multiple_subagents_tree() {
        let theme = ColorTheme::default();
        let mut p = pane(PermissionMode::Default, PaneStatus::Running, "test");
        p.subagents = vec!["Explore #1".into(), "Plan".into(), "Explore #2".into()];
        let lines = render_pane_lines_with_ports(
            &p,
            &PaneGitInfo::default(),
            None,
            None,
            None,
            false,
            false,
            theme.border_active,
            40,
            &StatusIcons::default(),
            &theme,
            0,
            0,
        );

        // status + 3 subagents + prompt = 5 lines minimum
        assert!(lines.len() >= 5);
        assert!(line_text(&lines[1]).contains("├ "));
        assert!(line_text(&lines[1]).contains("Explore #1"));
        assert!(line_text(&lines[2]).contains("├ "));
        assert!(line_text(&lines[2]).contains("Plan #2"));
        assert!(line_text(&lines[3]).contains("└ "));
        assert!(line_text(&lines[3]).contains("Explore #2"));
    }

    #[test]
    fn render_pane_lines_subagents_before_wait_reason() {
        let theme = ColorTheme::default();
        let mut p = pane(PermissionMode::Default, PaneStatus::Waiting, "");
        p.subagents = vec!["Explore".into()];
        p.wait_reason = "permission_prompt".into();
        let lines = render_pane_lines_with_ports(
            &p,
            &PaneGitInfo::default(),
            None,
            None,
            None,
            false,
            false,
            theme.border_active,
            40,
            &StatusIcons::default(),
            &theme,
            0,
            0,
        );

        // status + subagent + wait_reason + idle hint = 4
        assert!(lines.len() >= 3);
        let sub_line = line_text(&lines[1]);
        assert!(sub_line.contains("Explore #1"));
        let reason_line = line_text(&lines[2]);
        assert!(reason_line.contains("permission required"));
    }

    #[test]
    fn render_pane_lines_response_shows_arrow() {
        let theme = ColorTheme::default();
        let p = pane_with_response(
            PermissionMode::Default,
            PaneStatus::Idle,
            "Task completed successfully",
            true,
        );
        let lines = render_pane_lines_with_ports(
            &p,
            &PaneGitInfo::default(),
            None,
            None,
            None,
            false,
            false,
            theme.border_active,
            40,
            &StatusIcons::default(),
            &theme,
            0,
            0,
        );

        assert!(lines.len() >= 2);
        let response_line = line_text(&lines[1]);
        assert!(response_line.contains("▶"));
        assert!(response_line.contains("Task completed successfully"));
    }

    #[test]
    fn render_pane_lines_response_uses_char_wrap() {
        let theme = ColorTheme::default();
        // Long response that would word-wrap at spaces but should char-wrap instead
        let p = pane_with_response(
            PermissionMode::Default,
            PaneStatus::Idle,
            "abcdef ghijk lmnop qrstu vwxyz",
            true,
        );
        // Width 20: inner_width=17, prefix=4, so wrap at 13 chars
        let lines = render_pane_lines_with_ports(
            &p,
            &PaneGitInfo::default(),
            None,
            None,
            None,
            false,
            false,
            theme.border_active,
            20,
            &StatusIcons::default(),
            &theme,
            0,
            0,
        );

        assert!(lines.len() >= 2);
        // First line has ▶ + start of text
        let first = line_text(&lines[1]);
        assert!(first.contains("▶"));
        // Second line should NOT have trimmed spaces (char-wrap, not word-wrap)
        // With word-wrap "abcdef ghijk " would break at "ghijk", char-wrap fills fully
        let second = line_text(&lines[2]);
        assert!(!second.starts_with("│  ghijk"));
    }

    #[test]
    fn render_pane_lines_normal_prompt_not_detected_as_response() {
        let theme = ColorTheme::default();
        let p = pane(PermissionMode::Default, PaneStatus::Running, "fix the bug");
        let lines = render_pane_lines_with_ports(
            &p,
            &PaneGitInfo::default(),
            None,
            None,
            None,
            false,
            false,
            theme.border_active,
            40,
            &StatusIcons::default(),
            &theme,
            0,
            0,
        );

        assert!(lines.len() >= 2);
        let prompt_line = line_text(&lines[1]);
        assert!(!prompt_line.contains("▶"));
        assert!(prompt_line.contains("fix the bug"));
    }

    #[test]
    fn render_pane_lines_shows_task_progress() {
        use crate::activity::{TaskProgress, TaskStatus};
        let theme = ColorTheme::default();
        let p = pane(PermissionMode::Default, PaneStatus::Running, "");
        let progress = TaskProgress {
            tasks: vec![
                ("Task A".into(), TaskStatus::Completed),
                ("Task B".into(), TaskStatus::InProgress),
                ("Task C".into(), TaskStatus::Pending),
            ],
        };
        let lines = render_pane_lines_with_ports(
            &p,
            &PaneGitInfo::default(),
            None,
            None,
            Some(&progress),
            false,
            false,
            theme.border_active,
            40,
            &StatusIcons::default(),
            &theme,
            0,
            0,
        );

        // status + task progress + idle hint = 3 lines
        assert!(lines.len() >= 2);
        let task_line = line_text(&lines[1]);
        assert!(task_line.contains("✔◼◻"));
        assert!(task_line.contains("1/3"));
    }

    #[test]
    fn render_pane_lines_no_task_line_when_empty() {
        use crate::activity::TaskProgress;
        let theme = ColorTheme::default();
        let p = pane(PermissionMode::Default, PaneStatus::Idle, "");
        let progress = TaskProgress { tasks: vec![] };
        let lines = render_pane_lines_with_ports(
            &p,
            &PaneGitInfo::default(),
            None,
            None,
            Some(&progress),
            false,
            false,
            theme.border_active,
            40,
            &StatusIcons::default(),
            &theme,
            0,
            0,
        );

        // Should not have task line, just status + idle hint
        assert_eq!(lines.len(), 2);
        let hint = line_text(&lines[1]);
        assert!(hint.contains("Waiting for prompt"));
    }

    // ─── Row-level unit tests (added during decomposition) ──────

    #[test]
    fn branch_ports_row_returns_none_when_branch_and_ports_empty() {
        let theme = ColorTheme::default();
        let noop: &dyn Fn(Style) -> Style = &|s: Style| s;
        let ctx = test_ctx(&theme, 40, false, noop);
        let result = branch_ports_row(&PaneGitInfo::default(), None, &ctx);
        assert!(result.is_none());
    }

    #[test]
    fn branch_ports_row_returns_some_when_only_ports() {
        let theme = ColorTheme::default();
        let noop: &dyn Fn(Style) -> Style = &|s: Style| s;
        let ctx = test_ctx(&theme, 40, false, noop);
        let ports = vec![3000];
        let result = branch_ports_row(&PaneGitInfo::default(), Some(&ports), &ctx);
        let line = result.expect("should render port line");
        let text = line_text(&line);
        assert!(text.contains(":3000"));
    }

    #[test]
    fn subagent_rows_empty_returns_empty_vec() {
        let theme = ColorTheme::default();
        let noop: &dyn Fn(Style) -> Style = &|s: Style| s;
        let ctx = test_ctx(&theme, 40, false, noop);
        let rows = subagent_rows(&[], &ctx);
        assert!(rows.is_empty());
    }

    #[test]
    fn wait_reason_row_empty_returns_none() {
        let theme = ColorTheme::default();
        let noop: &dyn Fn(Style) -> Style = &|s: Style| s;
        let ctx = test_ctx(&theme, 40, false, noop);
        let result = wait_reason_row("", &PaneStatus::Waiting, &ctx);
        assert!(result.is_none());
    }

    #[test]
    fn wait_reason_row_uses_error_color_when_status_is_error() {
        let theme = ColorTheme::default();
        let noop: &dyn Fn(Style) -> Style = &|s: Style| s;
        let ctx = test_ctx(&theme, 40, false, noop);
        let line = wait_reason_row("permission_prompt", &PaneStatus::Error, &ctx)
            .expect("should render reason line");
        // The colored content span is the second (index=2): border, space, text
        let text_span = line
            .spans
            .iter()
            .find(|s| s.content.contains("permission"))
            .expect("reason text should be present");
        assert_eq!(text_span.style.fg, Some(theme.status_error));
    }

    #[test]
    fn task_progress_row_none_when_progress_empty() {
        use crate::activity::TaskProgress;
        let theme = ColorTheme::default();
        let noop: &dyn Fn(Style) -> Style = &|s: Style| s;
        let ctx = test_ctx(&theme, 40, false, noop);
        let progress = TaskProgress { tasks: vec![] };
        assert!(task_progress_row(Some(&progress), &ctx).is_none());
        assert!(task_progress_row(None, &ctx).is_none());
    }

    #[test]
    fn idle_hint_row_contains_waiting_text() {
        let theme = ColorTheme::default();
        let noop: &dyn Fn(Style) -> Style = &|s: Style| s;
        let ctx = test_ctx(&theme, 40, false, noop);
        let line = idle_hint_row(&ctx);
        assert!(line_text(&line).contains("Waiting for prompt"));
    }
}
