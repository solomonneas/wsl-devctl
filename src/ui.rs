use crate::app::{App, InputMode, RowHit};
use crate::data::format_uptime;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

pub fn draw(frame: &mut Frame<'_>, app: &mut App) {
    app.row_hits.clear();

    let area = frame.area();
    let vertical = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(8),
            Constraint::Length(3),
        ])
        .split(area);

    let filter_title = match app.input_mode {
        InputMode::Filtering => "Filter (typing…)".to_string(),
        InputMode::Normal => "Filter (/ or f)".to_string(),
    };
    let filter = Paragraph::new(app.filter.clone())
        .block(Block::default().title(filter_title).borders(Borders::ALL));
    frame.render_widget(filter, vertical[0]);

    let mut lines: Vec<Line<'static>> = Vec::new();
    let filtered = app.filtered_indices();

    let running: Vec<usize> = filtered
        .iter()
        .copied()
        .filter(|i| app.pm2_processes[*i].status == "online")
        .collect();
    let stopped: Vec<usize> = filtered
        .iter()
        .copied()
        .filter(|i| app.pm2_processes[*i].status != "online")
        .collect();

    lines.push(Line::from(vec![Span::styled(
        format!("🔥 RUNNING ({})", running.len()),
        Style::default().fg(Color::Green),
    )]));

    let mut y = vertical[1].y + 1;
    render_pm2_group(
        app,
        &mut lines,
        &running,
        &filtered,
        &mut y,
        vertical[1].width,
    );

    lines.push(Line::default());
    y += 1;
    lines.push(Line::from(vec![Span::styled(
        format!("😴 STOPPED ({})", stopped.len()),
        Style::default().fg(Color::Yellow),
    )]));
    y += 1;

    render_pm2_group(
        app,
        &mut lines,
        &stopped,
        &filtered,
        &mut y,
        vertical[1].width,
    );

    let processes = Paragraph::new(lines)
        .block(Block::default().title("PM2 Processes").borders(Borders::ALL))
        .wrap(Wrap { trim: false });
    frame.render_widget(processes, vertical[1]);

    let mut lower_lines: Vec<Line<'static>> = vec![Line::from(vec![Span::styled(
        "🌐 Caddy Static Roots",
        Style::default().fg(Color::Cyan),
    )])];
    for c in app.caddy_sites.iter().take(4) {
        let port = c.port.map(|p| format!(":{p}")).unwrap_or_else(|| "-".to_string());
        lower_lines.push(Line::from(format!("• {} {} -> {}", c.label, port, c.root)));
    }

    lower_lines.push(Line::default());
    lower_lines.push(Line::from(vec![Span::styled(
        "⚠️ PORT CONFLICTS",
        Style::default().fg(Color::LightRed),
    )]));
    if app.conflicts.is_empty() {
        lower_lines.push(Line::from("• none"));
    } else {
        for c in app.conflicts.iter().take(4) {
            lower_lines.push(Line::from(format!(
                "• :{} -> {}{}",
                c.port,
                c.owners.join(" vs "),
                if c.is_open { " [open]" } else { " [closed]" }
            )));
        }
    }

    let lower = Paragraph::new(lower_lines)
        .block(Block::default().borders(Borders::ALL).title("Caddy + Conflicts"))
        .wrap(Wrap { trim: true });
    frame.render_widget(lower, vertical[2]);

    let help = Paragraph::new(format!(
        "j/k:nav ↑/↓ | enter:browser | r:restart | s:stop/start | l:logs | q:quit || {}",
        app.status_message
    ))
    .block(
        Block::default()
            .title("Dev Server Control Center")
            .borders(Borders::ALL),
    );
    frame.render_widget(help, vertical[3]);
}

fn render_pm2_group(
    app: &mut App,
    lines: &mut Vec<Line<'static>>,
    indices: &[usize],
    filtered_order: &[usize],
    y: &mut u16,
    width: u16,
) {
    for idx in indices {
        let proc = &app.pm2_processes[*idx];
        let selected = filtered_order
            .get(app.selected)
            .map(|s| *s == *idx)
            .unwrap_or(false);

        let dot = if proc.status == "online" { "●" } else { "○" };
        let port = proc
            .port
            .map(|p| format!(":{p}"))
            .unwrap_or_else(|| "-".to_string());
        let uptime = if proc.status == "online" {
            format_uptime(proc.uptime_secs)
        } else {
            "-".to_string()
        };
        let action2 = if proc.status == "online" {
            "[s:stop]"
        } else {
            "[s:start]"
        };

        let row = format!(
            "{} {:<18} {:<6} {} {:<4} {:>4}MB [r:restart]{}[l:logs]",
            if selected { ">" } else { " " },
            proc.name,
            port,
            dot,
            uptime,
            proc.memory_mb,
            action2
        );

        let style = if selected {
            Style::default().bg(Color::DarkGray)
        } else {
            Style::default()
        };

        lines.push(Line::from(vec![Span::styled(row, style)]));

        let right = width.saturating_sub(2);
        app.row_hits.push(RowHit {
            y: *y,
            process_index: *idx,
            restart_x: (right.saturating_sub(28), right.saturating_sub(18)),
            stop_x: (right.saturating_sub(17), right.saturating_sub(9)),
            logs_x: (right.saturating_sub(8), right.saturating_sub(1)),
        });
        *y += 1;
    }
}
