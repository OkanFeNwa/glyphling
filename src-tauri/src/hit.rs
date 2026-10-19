//! Forma cliccabile del widget.
//!
//! La finestra è un rettangolo trasparente: fuori dal cerchio e dal pannello i clic
//! devono arrivare al desktop. Un thread legge la posizione del mouse 30 volte al
//! secondo e attiva o disattiva il passaggio dei clic quando entra o esce dalla forma.
//! Lo stesso thread manda la posizione del mouse alla mascotte, che la segue con gli occhi.

use crate::AppState;
use serde::{Deserialize, Serialize};
use std::sync::atomic::Ordering;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

#[derive(Deserialize, Clone, Default, Debug)]
pub struct Panel {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// Coordinate in pixel logici rispetto alla finestra del widget.
#[derive(Deserialize, Clone, Default, Debug)]
pub struct Shape {
    pub cx: f64,
    pub cy: f64,
    pub r: f64,
    pub panel: Option<Panel>,
}

impl Shape {
    pub fn contains(&self, x: f64, y: f64) -> bool {
        if self.r <= 0.0 {
            return true; // forma non ancora nota: meglio cliccabile che irraggiungibile
        }
        if (x - self.cx).hypot(y - self.cy) <= self.r {
            return true;
        }
        match &self.panel {
            Some(p) => x >= p.x && x <= p.x + p.w && y >= p.y && y <= p.y + p.h,
            None => false,
        }
    }
}

#[derive(Serialize, Clone)]
struct Cursor {
    x: f64,
    y: f64,
}

pub fn spawn(app: AppHandle) {
    std::thread::spawn(move || {
        let mut ignoring: Option<bool> = None;
        let mut last = (i32::MIN, i32::MIN);
        let mut tick = 0u32;
        loop {
            std::thread::sleep(Duration::from_millis(33));
            tick = tick.wrapping_add(1);
            let Some(w) = app.get_webview_window("widget") else { continue };
            let state = app.state::<AppState>();
            if state.dragging.load(Ordering::Relaxed) || !w.is_visible().unwrap_or(false) {
                continue;
            }
            let Some((mx, my)) = crate::platform::cursor_pos() else { continue };
            let (Ok(pos), Ok(sf)) = (w.outer_position(), w.scale_factor()) else { continue };
            let lx = (mx - pos.x) as f64 / sf;
            let ly = (my - pos.y) as f64 / sf;
            let inside = state.shape.lock().unwrap().contains(lx, ly);
            if ignoring != Some(!inside) {
                let _ = w.set_ignore_cursor_events(!inside);
                ignoring = Some(!inside);
            }
            // 15 volte al secondo bastano per gli occhi
            if tick % 2 == 0 && (mx, my) != last {
                last = (mx, my);
                let idle = state.cfg.lock().unwrap().mode == "idle";
                if idle && !inside {
                    let _ = app.emit_to("widget", "cursor", Cursor { x: lx, y: ly });
                }
            }
        }
    });
}
