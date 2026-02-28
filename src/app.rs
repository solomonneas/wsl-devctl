use crate::data::{CaddySite, Pm2Process, PortConflict};
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Default)]
pub struct RowHit {
    pub y: u16,
    pub process_index: usize,
    pub restart_x: (u16, u16),
    pub stop_x: (u16, u16),
    pub logs_x: (u16, u16),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    Filtering,
}

pub struct App {
    pub refresh_interval: Duration,
    pub last_refresh: Instant,
    pub filter: String,
    pub input_mode: InputMode,
    pub selected: usize,
    pub pm2_processes: Vec<Pm2Process>,
    pub caddy_sites: Vec<CaddySite>,
    pub conflicts: Vec<PortConflict>,
    pub status_message: String,
    pub manual_ports: Vec<u16>,
    pub row_hits: Vec<RowHit>,
}

impl App {
    pub fn new(refresh_interval: Duration, manual_ports: Vec<u16>) -> Self {
        Self {
            refresh_interval,
            last_refresh: Instant::now() - refresh_interval,
            filter: String::new(),
            input_mode: InputMode::Normal,
            selected: 0,
            pm2_processes: Vec::new(),
            caddy_sites: Vec::new(),
            conflicts: Vec::new(),
            status_message: "Loading…".to_string(),
            manual_ports,
            row_hits: Vec::new(),
        }
    }

    pub fn should_refresh(&self) -> bool {
        self.last_refresh.elapsed() >= self.refresh_interval
    }

    pub fn touch_refresh(&mut self) {
        self.last_refresh = Instant::now();
    }

    pub fn filtered_indices(&self) -> Vec<usize> {
        if self.filter.trim().is_empty() {
            return (0..self.pm2_processes.len()).collect();
        }

        let matcher = SkimMatcherV2::default();
        let q = self.filter.trim();

        let mut scored = self
            .pm2_processes
            .iter()
            .enumerate()
            .filter_map(|(idx, p)| {
                let hay = format!("{} {}", p.name, p.status);
                matcher.fuzzy_match(&hay, q).map(|score| (idx, score))
            })
            .collect::<Vec<_>>();

        scored.sort_by(|a, b| b.1.cmp(&a.1));
        scored.into_iter().map(|(idx, _)| idx).collect()
    }

    pub fn selected_process_index(&self) -> Option<usize> {
        let filtered = self.filtered_indices();
        filtered.get(self.selected).copied()
    }

    pub fn move_up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn move_down(&mut self) {
        let len = self.filtered_indices().len();
        if len > 0 && self.selected + 1 < len {
            self.selected += 1;
        }
    }

    pub fn clamp_selection(&mut self) {
        let len = self.filtered_indices().len();
        if len == 0 {
            self.selected = 0;
        } else if self.selected >= len {
            self.selected = len - 1;
        }
    }

    pub fn set_status(&mut self, msg: impl Into<String>) {
        self.status_message = msg.into();
    }
}
