//! Impostazioni del widget, salvate in `config.json` nella cartella dati dell'app.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct Config {
    /// Modalità mostrata: idle, clock, calendar, music, timer, weather, disks.
    pub mode: String,
    /// Livello della finestra: desktop, top, normal.
    pub level: String,
    /// Spostamento attivo (all'avvio il widget riparte sempre fermo).
    pub move_enabled: bool,
    pub snap: bool,
    /// Passo della griglia in pixel logici.
    pub grid: u32,
    pub visible: bool,
    /// Posizione della finestra in pixel fisici.
    pub x: Option<i32>,
    pub y: Option<i32>,
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
