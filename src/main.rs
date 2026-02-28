mod app;
mod data;
mod keys;
mod ui;

use anyhow::Result;
use app::{App, InputMode};
use clap::Parser;
use crossterm::event::{self, DisableMouseCapture, EnableMouseCapture, Event};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::ExecutableCommand;
use data::{detect_conflicts, fetch_caddy_sites, fetch_pm2_processes};
use keys::{map_event, AppCommand};
use ratatui::prelude::CrosstermBackend;
use ratatui::Terminal;
use std::io;
use std::process::Stdio;
use std::time::Duration;
use tokio::process::Command;

#[derive(Parser, Debug)]
#[command(name = "dev-tui")]
#[command(about = "Dev Server Control Center for PM2 + Caddy", long_about = None)]
struct Cli {
    /// Refresh interval in seconds
    #[arg(long, default_value_t = 2)]
    refresh: u64,

    /// Optional comma-separated manual ports to watch (e.g. 3000,5173,8080)
    #[arg(long)]
    manual_ports: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let refresh_interval = Duration::from_secs(cli.refresh.max(1));
    let manual_ports = parse_manual_ports(cli.manual_ports.as_deref().unwrap_or(""));

    enable_raw_mode()?;
    let mut stdout = io::stdout();
    stdout.execute(EnterAlternateScreen)?;
    stdout.execute(EnableMouseCapture)?;

    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(refresh_interval, manual_ports);
    let res = run_app(&mut terminal, &mut app).await;

    disable_raw_mode()?;
    io::stdout().execute(LeaveAlternateScreen)?;
    io::stdout().execute(DisableMouseCapture)?;

    res
}

async fn run_app(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    app: &mut App,
) -> Result<()> {
    refresh_data(app).await;

    loop {
        if app.should_refresh() {
            refresh_data(app).await;
        }

        terminal.draw(|f| ui::draw(f, app))?;

        if event::poll(Duration::from_millis(100))? {
            let ev = event::read()?;
            if let Event::Resize(_, _) = ev {
                continue;
            }

            match map_event(ev, app.input_mode) {
                AppCommand::Quit => break,
                AppCommand::Up => app.move_up(),
                AppCommand::Down => app.move_down(),
                AppCommand::Refresh => refresh_data(app).await,
                AppCommand::Enter => open_selected_in_browser(app).await,
                AppCommand::Restart => restart_selected(app).await,
                AppCommand::ToggleStartStop => toggle_selected(app).await,
                AppCommand::Logs => logs_selected(app).await,
                AppCommand::StartFilter => {
                    app.input_mode = InputMode::Filtering;
                }
                AppCommand::CancelFilter => {
                    app.input_mode = InputMode::Normal;
                }
                AppCommand::SubmitFilter => {
                    app.input_mode = InputMode::Normal;
                    app.clamp_selection();
                }
                AppCommand::Backspace => {
                    app.filter.pop();
                    app.clamp_selection();
                }
                AppCommand::Type(c) => {
                    app.filter.push(c);
                    app.clamp_selection();
                }
                AppCommand::MouseClick { x, y } => handle_mouse_click(app, x, y).await,
                AppCommand::None => {}
            }
        }
    }

    Ok(())
}

async fn refresh_data(app: &mut App) {
    let pm2 = fetch_pm2_processes().await.unwrap_or_default();
    let caddy = fetch_caddy_sites().await.unwrap_or_default();
    let conflicts = detect_conflicts(&pm2, &caddy, &app.manual_ports)
        .await
        .unwrap_or_default();

    app.pm2_processes = pm2;
    app.caddy_sites = caddy;
    app.conflicts = conflicts;
    app.clamp_selection();
    app.touch_refresh();

    app.set_status(format!(
        "refreshed: {} pm2, {} caddy, {} conflicts",
        app.pm2_processes.len(),
        app.caddy_sites.len(),
        app.conflicts.len()
    ));
}

async fn handle_mouse_click(app: &mut App, x: u16, y: u16) {
    if let Some(hit) = app.row_hits.iter().find(|r| r.y == y).cloned() {
        if let Some(sel_pos) = app
            .filtered_indices()
            .iter()
            .position(|idx| *idx == hit.process_index)
        {
            app.selected = sel_pos;
        }

        if x >= hit.restart_x.0 && x <= hit.restart_x.1 {
            restart_selected(app).await;
            return;
        }

        if x >= hit.stop_x.0 && x <= hit.stop_x.1 {
            toggle_selected(app).await;
            return;
        }

        if x >= hit.logs_x.0 && x <= hit.logs_x.1 {
            logs_selected(app).await;
        }
    }
}

async fn open_selected_in_browser(app: &mut App) {
    let Some(idx) = app.selected_process_index() else {
        app.set_status("no process selected");
        return;
    };

    let Some(port) = app.pm2_processes[idx].port else {
        app.set_status("selected process has no PORT");
        return;
    };

    let url = format!("http://localhost:{port}");
    let cmds = [
        vec!["wslview", url.as_str()],
        vec!["xdg-open", url.as_str()],
        vec!["cmd.exe", "/C", "start", "", url.as_str()],
    ];

    for cmd in cmds {
        let mut c = Command::new(cmd[0]);
        c.args(&cmd[1..]);
        if c
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .is_ok()
        {
            app.set_status(format!("opened {url}"));
            return;
        }
    }

    app.set_status(format!("failed to open {url}"));
}

async fn restart_selected(app: &mut App) {
    let Some(idx) = app.selected_process_index() else {
        app.set_status("no process selected");
        return;
    };

    let name = app.pm2_processes[idx].name.clone();
    let output = Command::new("pm2").arg("restart").arg(&name).output().await;
    match output {
        Ok(o) if o.status.success() => app.set_status(format!("restarted {name}")),
        Ok(o) => app.set_status(format!(
            "restart failed for {name}: {}",
            String::from_utf8_lossy(&o.stderr).trim()
        )),
        Err(e) => app.set_status(format!("restart failed for {name}: {e}")),
    }
    refresh_data(app).await;
}

async fn toggle_selected(app: &mut App) {
    let Some(idx) = app.selected_process_index() else {
        app.set_status("no process selected");
        return;
    };

    let proc = app.pm2_processes[idx].clone();
    let (verb, action) = if proc.status == "online" {
        ("stopped", "stop")
    } else {
        ("started", "start")
    };

    let output = Command::new("pm2").arg(action).arg(&proc.name).output().await;
    match output {
        Ok(o) if o.status.success() => app.set_status(format!("{verb} {}", proc.name)),
        Ok(o) => app.set_status(format!(
            "{} failed for {}: {}",
            action,
            proc.name,
            String::from_utf8_lossy(&o.stderr).trim()
        )),
        Err(e) => app.set_status(format!("{} failed for {}: {e}", action, proc.name)),
    }
    refresh_data(app).await;
}

async fn logs_selected(app: &mut App) {
    let Some(idx) = app.selected_process_index() else {
        app.set_status("no process selected");
        return;
    };

    let name = app.pm2_processes[idx].name.clone();
    let output = Command::new("pm2")
        .arg("logs")
        .arg(&name)
        .arg("--lines")
        .arg("20")
        .arg("--nostream")
        .output()
        .await;

    match output {
        Ok(o) if o.status.success() => {
            let text = String::from_utf8_lossy(&o.stdout);
            let first = text.lines().next().unwrap_or("no log lines");
            app.set_status(format!("logs {}: {}", name, first));
        }
        Ok(o) => app.set_status(format!(
            "logs failed for {name}: {}",
            String::from_utf8_lossy(&o.stderr).trim()
        )),
        Err(e) => app.set_status(format!("logs failed for {name}: {e}")),
    }
}

fn parse_manual_ports(input: &str) -> Vec<u16> {
    input
        .split(',')
        .filter_map(|s| s.trim().parse::<u16>().ok())
        .collect()
}
