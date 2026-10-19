//! Widget settings, saved to `config.json` in the app's data folder.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct Config {
    /// Mode shown: idle, clock, calendar, music, timer, weather, disks.
    pub mode: String,
    /// Window level: desktop, top, normal.
    pub level: String,
    /// Moving enabled (the widget always starts locked).
    pub move_enabled: bool,
    pub snap: bool,
    /// Grid step in logical pixels.
    pub grid: u32,
    pub visible: bool,
    /// Window position in physical pixels.
    pub x: Option<i32>,
    pub y: Option<i32>,
    /// Which side the panel opens on: right, left or auto (based on where the widget sits).
    pub panel_dir: String,
    /// Current side: true if the panel opens to the left (the circle sits on the right of the window).
    pub left: bool,
    /// Where the circle's center sits (physical pixels), when no quick position is chosen.
    pub home: Option<(i32, i32)>,
    /// Quick position: tl, tc, tr, ml, c, mr, bl, bc, br (recomputed from the screen).
    pub preset: Option<String>,
    /// The first-run tutorial was already shown (saved here so updates keep it).
    pub tutorial_done: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            mode: "idle".into(),
            level: "desktop".into(),
            move_enabled: false,
            snap: true,
            grid: 24,
            visible: true,
            x: None,
            y: None,
            panel_dir: "auto".into(),
            left: false,
            home: None,
            preset: None,
            tutorial_done: false,
        }
    }
}

pub fn dir(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_config_dir().ok()
}

fn file(app: &AppHandle) -> Option<PathBuf> {
    dir(app).map(|d| d.join("config.json"))
}

pub fn load(app: &AppHandle) -> Config {
    let mut c: Config = file(app)
        .and_then(|f| std::fs::read_to_string(f).ok())
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();
    c.move_enabled = false;
    if !["right", "left", "auto"].contains(&c.panel_dir.as_str()) {
        c.panel_dir = "auto".into();
    }
    if c.preset.as_deref().is_some_and(|p| !crate::snap::PRESETS.contains(&p)) {
        c.preset = None;
    }
    if c.grid < 8 || c.grid > 96 {
        c.grid = 24;
    }
    c
}

pub fn save(app: &AppHandle, c: &Config) {
    let Some(f) = file(app) else { return };
    if let Some(d) = f.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    if let Ok(s) = serde_json::to_string_pretty(c) {
        let _ = std::fs::write(f, s);
    }
}
