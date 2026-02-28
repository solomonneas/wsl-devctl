use crate::app::InputMode;
use crossterm::event::{Event, KeyCode, MouseEventKind};

#[derive(Debug, Clone)]
pub enum AppCommand {
    None,
    Quit,
    Up,
    Down,
    Refresh,
    Enter,
    Restart,
    ToggleStartStop,
    Logs,
    StartFilter,
    CancelFilter,
    SubmitFilter,
    Backspace,
    Type(char),
    MouseClick { x: u16, y: u16 },
}

pub fn map_event(event: Event, mode: InputMode) -> AppCommand {
    match event {
        Event::Key(k) => match mode {
            InputMode::Filtering => match k.code {
                KeyCode::Esc => AppCommand::CancelFilter,
                KeyCode::Enter => AppCommand::SubmitFilter,
                KeyCode::Backspace => AppCommand::Backspace,
                KeyCode::Char(c) => AppCommand::Type(c),
                _ => AppCommand::None,
            },
            InputMode::Normal => match k.code {
                KeyCode::Char('q') | KeyCode::Esc => AppCommand::Quit,
                KeyCode::Down | KeyCode::Char('j') => AppCommand::Down,
                KeyCode::Up | KeyCode::Char('k') => AppCommand::Up,
                KeyCode::Char('r') => AppCommand::Restart,
                KeyCode::Char('s') => AppCommand::ToggleStartStop,
                KeyCode::Char('l') => AppCommand::Logs,
                KeyCode::Char('f') | KeyCode::Char('/') => AppCommand::StartFilter,
                KeyCode::Enter => AppCommand::Enter,
                KeyCode::Char('R') => AppCommand::Refresh,
                _ => AppCommand::None,
            },
        },
        Event::Mouse(m) => {
            if matches!(m.kind, MouseEventKind::Down(_)) {
                AppCommand::MouseClick {
                    x: m.column,
                    y: m.row,
                }
            } else {
                AppCommand::None
            }
        }
        _ => AppCommand::None,
    }
}
