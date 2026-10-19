//! Clickable shape of the widget.
//!
//! The window is clipped to the shape (see `platform::set_region`); on top of that a thread reads
//! the mouse position 60 times a second and toggles click-through when it enters or leaves the shape.
//! The same thread sends the mouse position to the mascot, which follows it with its eyes.

use crate::AppState;
use serde::{Deserialize, Serialize};
use std::sync::atomic::Ordering;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

/// One piece of the shape: a rectangle with corners rounded by radius `r`
/// (a circle is a square with `r` equal to half its side). Logical pixels relative to the window.
#[derive(Deserialize, Clone, Default, Debug)]
pub struct Part {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub r: f64,
}

impl Part {
    fn contains(&self, x: f64, y: f64) -> bool {
        if x < self.x || y < self.y || x > self.x + self.w || y > self.y + self.h {
            return false;
        }
        let r = self.r.min(self.w / 2.0).min(self.h / 2.0);
        // distance from the inner rectangle shrunk by r
        let dx = (self.x + r - x).max(x - (self.x + self.w - r)).max(0.0);
        let dy = (self.y + r - y).max(y - (self.y + self.h - r)).max(0.0);
        dx.hypot(dy) <= r
    }
}

/// The visible shape of the widget: union of its pieces (circle, panel, label...).
/// Used for clicks and to clip the window, so the blur stays inside the shape.
#[derive(Deserialize, Clone, Default, Debug)]
pub struct Shape {
    pub parts: Vec<Part>,
}

impl Shape {
    pub fn contains(&self, x: f64, y: f64) -> bool {
        if self.parts.is_empty() {
            return true; // shape not known yet: better clickable than unreachable
        }
        self.parts.iter().any(|p| p.contains(x, y))
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
            std::thread::sleep(Duration::from_millis(16));
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
            // while the button is held (turning the dial) the widget keeps the mouse even
            // outside its shape, otherwise the drag would stop at the circle's edge
            let held = ignoring == Some(false) && crate::platform::left_button_down();
            let inside = held || state.shape.lock().unwrap().contains(lx, ly);
            if ignoring != Some(!inside) {
                let _ = w.set_ignore_cursor_events(!inside);
                ignoring = Some(!inside);
            }
            // 15 times a second is enough for the eyes
            if tick % 4 == 0 && (mx, my) != last {
                last = (mx, my);
                let idle = state.cfg.lock().unwrap().mode == "idle";
                if idle && !inside {
                    let _ = app.emit_to("widget", "cursor", Cursor { x: lx, y: ly });
                }
            }
        }
    });
}
