//! Widget position.
//!
//! The circle is the only reference point (the anchor, in physical pixels): it is kept
//! fully on screen with a 24 px margin, outside the taskbar. When the panel opens it
//! picks a side; if that side has no room the circle slides away from the edge only
//! while the panel is open. Quick positions (corners, edge middles, center) stick to
//! the edges and are recomputed from the work area. Dragging snaps the circle's center
//! to the grid, so the dots shown while dragging mark where it lands.

use crate::{config, AppState, D, OPEN_W, PAD_X, PAD_Y, WIN_W};
use tauri::Monitor;
use serde::Serialize;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, WebviewWindow};

/// Margin from the work area edges, in logical pixels.
const EDGE: f64 = 24.0;
/// The mode label sits below the circle near the top, so the top margin can be small.
const EDGE_T: f64 = 8.0;
/// Radius of the circle's hit area (circle plus bezel).
const R: f64 = D / 2.0 + 11.0;
/// How far the panel reaches past the circle's edge, and the width of circle + panel.
const EXT: f64 = OPEN_W - D;
const TOTAL: f64 = OPEN_W;
const OFF_Y: f64 = PAD_Y + D / 2.0;
pub const PRESETS: [&str; 9] = ["tl", "tc", "tr", "ml", "c", "mr", "bl", "bc", "br"];

/// Horizontal distance of the circle's center from the window's left edge, in logical pixels.
/// With the panel on the left the circle sits on the right of the window.
pub fn cx_off(left: bool) -> f64 {
    if left { WIN_W - PAD_X - D / 2.0 } else { PAD_X + D / 2.0 }
}

#[derive(Serialize, Clone)]
struct Grid {
    on: bool,
    step: u32,
    ox: f64,
    oy: f64,
    color: Option<String>,
}

pub fn start_drag(app: &AppHandle, w: &WebviewWindow, color: Option<String>) -> tauri::Result<()> {
    let state = app.state::<AppState>();
    if state.dragging.swap(true, Ordering::SeqCst) {
        return Ok(());
    }
    let cfg = state.cfg.lock().unwrap().clone();
    if cfg.snap {
        show_grid(app, w, cfg.grid, color);
    }
    w.start_dragging()?;

    let app = app.clone();
    std::thread::spawn(move || {
        // the native handler moves the window while the button is held down
        std::thread::sleep(Duration::from_millis(60));
        let t0 = Instant::now();
        while crate::platform::left_button_down() && t0.elapsed() < Duration::from_secs(120) {
            std::thread::sleep(Duration::from_millis(15));
        }
        let free = !cfg.snap || crate::platform::alt_down();
        if let Some(w) = app.get_webview_window("widget") {
            if let Some(a) = dropped_anchor(&w, &cfg, free) {
                crate::update(&app, |c| {
                    c.preset = None;
                    c.home = Some(a);
                });
            }
        }
        hide_grid(&app);
        app.state::<AppState>().dragging.store(false, Ordering::SeqCst);
        layout(&app, 8);
    });
    Ok(())
}

/// Work area of a monitor: left, top, right, bottom (physical) and scale factor.
#[derive(Clone, Copy)]
struct Area {
    l: f64,
    t: f64,
    r: f64,
    b: f64,
    sf: f64,
}

impl Area {
    fn of(m: &Monitor) -> Self {
        let wa = m.work_area();
        let (l, t) = (wa.position.x as f64, wa.position.y as f64);
        Area { l, t, r: l + wa.size.width as f64, b: t + wa.size.height as f64, sf: m.scale_factor() }
    }
}

/// The monitor holding point `p` (physical), else the window's, else the primary one.
fn area_at(w: &WebviewWindow, p: Option<(i32, i32)>) -> Option<Area> {
    let mons = w.available_monitors().unwrap_or_default();
    p.and_then(|(x, y)| {
        mons.iter().find(|m| {
            let (mp, ms) = (m.position(), m.size());
            x >= mp.x && y >= mp.y && x < mp.x + ms.width as i32 && y < mp.y + ms.height as i32
        })
    })
    .cloned()
    .or_else(|| w.current_monitor().ok().flatten())
    .or_else(|| w.primary_monitor().ok().flatten())
    .map(|m| Area::of(&m))
}

/// Circle center for a quick position.
fn preset_anchor(id: &str, a: Area) -> (f64, f64) {
    let (e, r) = (EDGE * a.sf, R * a.sf);
    let (col, row) = if id == "c" { ('c', 'm') } else { (id.chars().nth(1).unwrap_or('c'), id.chars().next().unwrap_or('m')) };
    let x = match col { 'l' => a.l + e + r, 'r' => a.r - e - r, _ => (a.l + a.r) / 2.0 };
    let y = match row { 't' => a.t + EDGE_T * a.sf + r, 'b' => a.b - e - r, _ => (a.t + a.b) / 2.0 };
    (x, y)
}

/// The circle's whole hit area stays on screen.
fn clamp_anchor((x, y): (f64, f64), a: Area) -> (f64, f64) {
    let m = (EDGE + R) * a.sf;
    let mt = (EDGE_T + R) * a.sf;
    (x.clamp(a.l + m, (a.r - m).max(a.l + m)), y.clamp(a.t + mt, (a.b - m).max(a.t + mt)))
}

/// Where the circle lives, from the settings: quick position, saved place, or the
/// window's current place for settings written by older versions.
fn anchor(w: &WebviewWindow, c: &config::Config) -> Option<((f64, f64), Area)> {
    let sf = w.scale_factor().ok()?;
    let from_window = || {
        let p = match (c.x, c.y) {
            (Some(x), Some(y)) => PhysicalPosition::new(x, y),
            _ => w.outer_position().ok()?,
        };
        Some(((p.x as f64 + cx_off(c.left) * sf) as i32, (p.y as f64 + OFF_Y * sf) as i32))
    };
    let home = c.home.or_else(from_window);
    let area = area_at(w, home)?;
    let a = match (&c.preset, home) {
        (Some(id), _) => preset_anchor(id, area),
        (None, Some((x, y))) => (x as f64, y as f64),
        (None, None) => preset_anchor("br", area),
    };
    Some((clamp_anchor(a, area), area))
}

/// Side and slide of the open panel for a circle centered at `x`.
fn open_layout(x: f64, c: &config::Config, a: Area) -> (bool, f64) {
    let (e, half, ext, total) = (EDGE * a.sf, D / 2.0 * a.sf, EXT * a.sf, TOTAL * a.sf);
    let (cl, cr) = (x - half, x + half);
    let (room_l, room_r) = (cl - a.l - e, a.r - e - cr);
    let left = match c.panel_dir.as_str() {
        "left" => true,
        "right" => false,
        // toward the middle of the screen, unless only the other side has room
        _ => {
            if x > (a.l + a.r) / 2.0 { room_l >= ext || room_r < ext } else { !(room_r >= ext || room_l < ext) }
        }
    };
    let centered = c.preset.as_deref().is_some_and(|p| p == "c" || p.ends_with('c'));
    let dx = if centered {
        // middle quick positions: circle + panel together in the middle
        let want = (a.l + a.r) / 2.0 - total / 2.0;
        if left { want + total - half - x } else { want + half - x }
    } else if left && room_l < ext {
        ext - room_l
    } else if !left && room_r < ext {
        room_r - ext
    } else {
        0.0
    };
    (left, dx)
}

/// Places the window for the current state: circle on its anchor, or slid aside while
/// the panel is open and needs room. `steps` > 0 glides there.
pub fn layout(app: &AppHandle, steps: i32) {
    if crate::in_tutorial(app) {
        return;
    }
    let Some(w) = app.get_webview_window("widget") else { return };
    let cfg = app.state::<AppState>().cfg.lock().unwrap().clone();
    let Some(((ax, ay), area)) = anchor(&w, &cfg) else { return };
    let sf = area.sf;
    let (left, dx) = if cfg.mode != "idle" { open_layout(ax, &cfg, area) } else { (cfg.left, 0.0) };
    let to = PhysicalPosition::new((ax + dx - cx_off(left) * sf).round() as i32, (ay - OFF_Y * sf).round() as i32);
    let side_changed = left != cfg.left;
    // the label goes to the side of the circle that has room: under it when the circle is high
    let below = ay < (area.t + area.b) / 2.0;
    crate::update(app, |c| {
        c.left = left;
        c.below = below;
        c.x = Some(to.x);
        c.y = Some(to.y);
        if c.preset.is_none() {
            c.home = Some((ax.round() as i32, ay.round() as i32));
        }
    });
    if side_changed || steps <= 0 {
        // the page moves the circle to the other side of the window in the same frame
        let _ = w.set_position(to);
    } else {
        std::thread::spawn(move || glide(&w, to, steps));
    }
}

/// Circle center where the user dropped the widget, snapped to the grid unless free.
fn dropped_anchor(w: &WebviewWindow, cfg: &config::Config, free: bool) -> Option<(i32, i32)> {
    let pos = w.outer_position().ok()?;
    let sf = w.scale_factor().ok()?;
    let mut x = pos.x as f64 + cx_off(cfg.left) * sf;
    let mut y = pos.y as f64 + OFF_Y * sf;
    let area = area_at(w, Some((x as i32, y as i32)))?;
    if !free {
        let g = cfg.grid as f64 * sf;
        x = area.l + ((x - area.l) / g).round() * g;
        y = area.t + ((y - area.t) / g).round() * g;
    }
    let (x, y) = clamp_anchor((x, y), area);
    Some((x.round() as i32, y.round() as i32))
}

/// Glides the window to `to` in `steps` frames of 15 ms with an ease-out curve.
fn glide(w: &WebviewWindow, to: PhysicalPosition<i32>, steps: i32) {
    let Ok(from) = w.outer_position() else { return };
    if from == to {
        return;
    }
    // Time-based so a slow frame skips ahead instead of stretching the move.
    let total = Duration::from_millis(steps.max(1) as u64 * 15);
    let start = std::time::Instant::now();
    loop {
        let t = (start.elapsed().as_secs_f64() / total.as_secs_f64()).min(1.0);
        let k = 1.0 - (1.0 - t).powi(3);
        let x = from.x as f64 + (to.x - from.x) as f64 * k;
        let y = from.y as f64 + (to.y - from.y) as f64 * k;
        let _ = w.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));
        if t >= 1.0 {
            break;
        }
        std::thread::sleep(Duration::from_millis(8));
    }
}

fn show_grid(app: &AppHandle, w: &WebviewWindow, step: u32, color: Option<String>) {
    let Some(g) = app.get_webview_window("grid") else { return };
    let Some(mon) = w.current_monitor().ok().flatten() else { return };
    let wa = mon.work_area();
    let _ = g.set_position(wa.position);
    let _ = g.set_size(wa.size);
    crate::platform::show_no_activate(&g);
    let _ = g.set_ignore_cursor_events(true);
    let _ = app.emit_to("grid", "grid", Grid { on: true, step, ox: 0.0, oy: 0.0, color });
}

fn hide_grid(app: &AppHandle) {
    let _ = app.emit_to("grid", "grid", Grid { on: false, step: 24, ox: 0.0, oy: 0.0, color: None });
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(260)); // let the fade finish
        if !app.state::<AppState>().dragging.load(Ordering::SeqCst) {
            if let Some(g) = app.get_webview_window("grid") {
                let _ = g.hide();
            }
        }
    });
}

/// Picks a quick position ("c" is also "Centra").
pub fn set_preset(app: &AppHandle, id: &str) {
    if !PRESETS.contains(&id) {
        return;
    }
    crate::update(app, |c| c.preset = Some(id.to_string()));
    layout(app, 10);
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: Area = Area { l: 0.0, t: 0.0, r: 1920.0, b: 1032.0, sf: 1.0 };

    fn cfg(dir: &str, preset: Option<&str>) -> config::Config {
        config::Config { panel_dir: dir.into(), preset: preset.map(String::from), ..Default::default() }
    }

    #[test]
    fn top_right_corner() {
        let (x, _) = preset_anchor("tr", SCREEN);
        // auto and left open toward the middle, without sliding
        assert_eq!(open_layout(x, &cfg("auto", Some("tr")), SCREEN), (true, 0.0));
        assert_eq!(open_layout(x, &cfg("left", Some("tr")), SCREEN), (true, 0.0));
        // forced right: the circle slides left just enough for the panel
        let (left, dx) = open_layout(x, &cfg("right", Some("tr")), SCREEN);
        assert!(!left);
        assert!((x + dx + D / 2.0 + EXT - (SCREEN.r - EDGE)).abs() < 0.01);
    }

    #[test]
    fn middle_presets_center_the_whole_shape() {
        let (x, _) = preset_anchor("c", SCREEN);
        let (left, dx) = open_layout(x, &cfg("right", Some("c")), SCREEN);
        assert!(!left);
        let union_l = x + dx - D / 2.0;
        assert!((union_l + TOTAL / 2.0 - SCREEN.r / 2.0).abs() < 0.01);
    }

    #[test]
    fn anchor_stays_on_screen() {
        let (x, y) = clamp_anchor((-500.0, 5000.0), SCREEN);
        assert_eq!((x, y), (EDGE + R, SCREEN.b - EDGE - R));
    }
}
