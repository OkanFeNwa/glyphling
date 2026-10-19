//! Posizione del widget: trascinamento nativo, aggancio alla griglia e ai bordi, ripristino.
//!
//! Il punto che si aggancia è il centro del cerchio, così la griglia di punti mostrata
//! durante il trascinamento indica esattamente dove atterrerà.

use crate::{config, AppState, D, OPEN_W, PAD_X, PAD_Y, WIN_W};
use serde::Serialize;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, WebviewWindow};

/// Margine dai bordi dell'area di lavoro e soglia di aggancio ai bordi, in pixel logici.
const MARGIN: f64 = 8.0;
const EDGE: f64 = 12.0;
/// Raggio del cerchio con la ghiera.
const R: f64 = D / 2.0 + 12.0;

/// Distanza orizzontale del centro del cerchio dal bordo sinistro della finestra, in pixel logici.
/// Con il riquadro a sinistra il cerchio sta a destra nella finestra.
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
        // il gestore nativo sposta la finestra finché il tasto resta premuto
        std::thread::sleep(Duration::from_millis(60));
        let t0 = Instant::now();
        while crate::platform::left_button_down() && t0.elapsed() < Duration::from_secs(120) {
            std::thread::sleep(Duration::from_millis(15));
        }
        let free = !cfg.snap || crate::platform::alt_down();
        if let Some(w) = app.get_webview_window("widget") {
            if let Some(target) = target(&app, &w, cfg.grid, free) {
                glide(&w, target);
                remember(&app, target);
            }
        }
        sync_side(&app);
        hide_grid(&app);
        app.state::<AppState>().dragging.store(false, Ordering::SeqCst);
    });
    Ok(())
}

fn panel_open(app: &AppHandle) -> bool {
    app.state::<AppState>().shape.lock().unwrap().panel.is_some()
}

/// Dove deve finire la finestra dopo il rilascio.
fn target(app: &AppHandle, w: &WebviewWindow, grid: u32, free: bool) -> Option<PhysicalPosition<i32>> {
    let pos = w.outer_position().ok()?;
    let sf = w.scale_factor().ok()?;
    let mon = w.current_monitor().ok().flatten().or_else(|| w.primary_monitor().ok().flatten())?;
    let wa = mon.work_area();
    let (wx, wy) = (wa.position.x as f64, wa.position.y as f64);
    let (ww, wh) = (wa.size.width as f64, wa.size.height as f64);

    let left = app.state::<AppState>().cfg.lock().unwrap().left;
    // centro del cerchio in pixel fisici
    let off_x = cx_off(left) * sf;
    let off_y = (PAD_Y + D / 2.0) * sf;
    let mut cx = pos.x as f64 + off_x;
    let mut cy = pos.y as f64 + off_y;

    let r = R * sf;
    let m = MARGIN * sf;
    // il riquadro aperto deve restare sullo schermo dal lato in cui si apre
    let panel = (OPEN_W - D / 2.0) * sf;
    let open = panel_open(app);
    let (lext, rext) = match (open, left) { (true, true) => (panel, r), (true, false) => (r, panel), _ => (r, r) };
    let (min_x, max_x) = (wx + lext + m, wx + ww - rext - m);
    let (min_y, max_y) = (wy + r + m, wy + wh - r - m);

    if !free {
        let g = grid as f64 * sf;
        cx = wx + ((cx - wx) / g).round() * g;
        cy = wy + ((cy - wy) / g).round() * g;
        // vicino a un bordo: si appoggia al bordo
        let e = EDGE * sf;
        if (cx - min_x).abs() < e { cx = min_x }
        if (max_x - cx).abs() < e { cx = max_x }
        if (cy - min_y).abs() < e { cy = min_y }
        if (max_y - cy).abs() < e { cy = max_y }
    }
    cx = cx.clamp(min_x, max_x.max(min_x));
    cy = cy.clamp(min_y, max_y.max(min_y));
    Some(PhysicalPosition::new((cx - off_x).round() as i32, (cy - off_y).round() as i32))
}

/// Breve movimento verso la posizione finale (~120 ms).
fn glide(w: &WebviewWindow, to: PhysicalPosition<i32>) {
    let Ok(from) = w.outer_position() else { return };
    if from == to {
        return;
    }
    const STEPS: i32 = 8;
    for i in 1..=STEPS {
        let t = i as f64 / STEPS as f64;
        let k = 1.0 - (1.0 - t).powi(3);
        let x = from.x as f64 + (to.x - from.x) as f64 * k;
        let y = from.y as f64 + (to.y - from.y) as f64 * k;
        let _ = w.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));
        std::thread::sleep(Duration::from_millis(15));
    }
}

fn remember(app: &AppHandle, p: PhysicalPosition<i32>) {
    let state = app.state::<AppState>();
    let mut c = state.cfg.lock().unwrap();
    c.x = Some(p.x);
    c.y = Some(p.y);
    config::save(app, &c);
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
        std::thread::sleep(Duration::from_millis(260)); // lascia finire la dissolvenza
        if !app.state::<AppState>().dragging.load(Ordering::SeqCst) {
            if let Some(g) = app.get_webview_window("grid") {
                let _ = g.hide();
            }
        }
    });
}

/// Posizione iniziale: in basso a destra del monitor principale. Se il riquadro si apre
/// sempre a destra lascia lo spazio per il riquadro, altrimenti si apre verso sinistra.
pub fn default_position(w: &WebviewWindow, c: &config::Config) -> Option<PhysicalPosition<i32>> {
    let mon = w.primary_monitor().ok().flatten()?;
    let sf = mon.scale_factor();
    let wa = mon.work_area();
    let room = if c.panel_dir == "right" { OPEN_W - D / 2.0 } else { R };
    let cx = wa.position.x as f64 + wa.size.width as f64 - (room + 48.0) * sf;
    let cy = wa.position.y as f64 + wa.size.height as f64 - (R + 48.0) * sf;
    Some(PhysicalPosition::new(
        (cx - cx_off(c.left) * sf).round() as i32,
        (cy - (PAD_Y + D / 2.0) * sf).round() as i32,
    ))
}

/// All'avvio: la posizione salvata se il cerchio cade ancora su un monitor, altrimenti quella iniziale.
pub fn restore(w: &WebviewWindow, c: &config::Config) {
    let saved = match (c.x, c.y) {
        (Some(x), Some(y)) => Some(PhysicalPosition::new(x, y)),
        _ => None,
    };
    let visible = saved.filter(|p| {
        w.available_monitors().unwrap_or_default().iter().any(|m| {
            let sf = m.scale_factor();
            let cx = p.x as f64 + cx_off(c.left) * sf;
            let cy = p.y as f64 + (PAD_Y + D / 2.0) * sf;
            let (mp, ms) = (m.position(), m.size());
            cx >= mp.x as f64 && cy >= mp.y as f64 && cx < (mp.x + ms.width as i32) as f64 && cy < (mp.y + ms.height as i32) as f64
        })
    });
    if let Some(p) = visible.or_else(|| default_position(w, c)) {
        let _ = w.set_position(p);
    }
}

pub fn reset(app: &AppHandle) {
    let Some(w) = app.get_webview_window("widget") else { return };
    let cfg = app.state::<AppState>().cfg.lock().unwrap().clone();
    if let Some(p) = default_position(&w, &cfg) {
        let app = app.clone();
        std::thread::spawn(move || {
            glide(&w, p);
            remember(&app, p);
            sync_side(&app);
        });
    }
}

/// Sceglie il lato del riquadro (fisso o automatico: a sinistra se il cerchio sta nella metà
/// destra del monitor). Se il lato cambia sposta la finestra, così il cerchio resta fermo.
pub fn sync_side(app: &AppHandle) {
    let Some(w) = app.get_webview_window("widget") else { return };
    let (Ok(pos), Ok(sf)) = (w.outer_position(), w.scale_factor()) else { return };
    let cfg = app.state::<AppState>().cfg.lock().unwrap().clone();
    let cx = pos.x as f64 + cx_off(cfg.left) * sf;
    let want = match cfg.panel_dir.as_str() {
        "left" => true,
        "right" => false,
        _ => {
            let mon = w.current_monitor().ok().flatten().or_else(|| w.primary_monitor().ok().flatten());
            let Some(mon) = mon else { return };
            let wa = mon.work_area();
            cx > wa.position.x as f64 + wa.size.width as f64 / 2.0
        }
    };
    if want == cfg.left {
        return;
    }
    let x = (cx - cx_off(want) * sf).round() as i32;
    let _ = w.set_position(PhysicalPosition::new(x, pos.y));
    crate::update(app, |c| {
        c.left = want;
        c.x = Some(x);
        c.y = Some(pos.y);
    });
}
