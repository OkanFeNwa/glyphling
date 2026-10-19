//! Widget settings, saved to `config.json` in the app's data folder.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

/// An app in the music list: `key` is what it is matched by (compact exe name), `name` what is shown.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct MusicApp {
    pub key: String,
    pub name: String,
}

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
    /// The dial label sits under the circle (the circle is in the top half of the screen).
    pub below: bool,
    /// Where the circle's center sits (physical pixels), when no quick position is chosen.
    pub home: Option<(i32, i32)>,
    /// Quick position: tl, tc, tr, ml, c, mr, bl, bc, br (recomputed from the screen).
    pub preset: Option<String>,
    /// The first-run tutorial was already shown (saved here so updates keep it).
    pub tutorial_done: bool,
    /// Music source filter: all, allow (only the listed apps) or block (all but the listed).
    /// The CPU temperature sensor was already offered once.
    pub sensor_offered: bool,
    /// Look for new versions by itself.
    pub auto_update: bool,
    /// Last version Settings was opened for (so it is announced once).
    pub update_notified: String,
    /// Process the volume slider acts on (e.g. "spotify"); empty = the music app being followed.
    pub volume_target: String,
    /// Apps the music mode listens to and controls; empty = any app.
    pub music_list: Vec<MusicApp>,
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
            below: false,
            home: None,
            preset: None,
            tutorial_done: false,
            sensor_offered: false,
            auto_update: true,
            update_notified: String::new(),
            volume_target: String::new(),
            music_list: vec![],
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
