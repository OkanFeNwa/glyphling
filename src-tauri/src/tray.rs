//! Notification area icon and its glass menu (a small window, src/menu.html).

use crate::{config::Config, AppState};
use tauri::image::Image;
use serde::Serialize;
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize};

pub const MODES: [(&str, &str); 7] = [
    ("idle", "Mascotte"),
    ("clock", "Orologio"),
    ("calendar", "Calendario"),
    ("music", "Musica"),
    ("timer", "Timer"),
    ("weather", "Meteo"),
    ("disks", "Sistema"),
];
pub const LEVELS: [(&str, &str); 3] = [
    ("desktop", "Sul desktop"),
    ("top", "Sempre in primo piano"),
    ("normal", "Normale"),
];

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    // no system menu: right click opens the glass menu ("menu" window)
    TrayIconBuilder::with_id("main")
        .icon(Image::from_bytes(include_bytes!("../icons/tray.png"))?)
        .tooltip("Glyphling")
        .show_menu_on_left_click(false)
        .on_tray_icon_event(|tray, ev| {
            if let TrayIconEvent::Click { button, button_state: MouseButtonState::Up, position, .. } = ev {
                match button {
                    MouseButton::Left => crate::update(tray.app_handle(), |c| c.visible = !c.visible),
                    MouseButton::Right => show_menu(tray.app_handle(), position.x, position.y),
                    _ => {}
                }
            }
        })
        .build(app)?;
    Ok(())
}

#[derive(Serialize, Clone)]
struct MenuOpen {
    cfg: Config,
    version: String,
}

/// Opens the menu with its bottom right corner near the point (physical pixels). The page
/// draws the items, measures the menu and calls `place_menu` for size and position.
pub fn show_menu(app: &AppHandle, x: f64, y: f64) {
    *app.state::<AppState>().menu.lock().unwrap() = Some((x, y));
    let cfg = app.state::<AppState>().cfg.lock().unwrap().clone();
    let version = app.package_info().version.to_string();
    let _ = app.emit_to("menu", "menu-open", MenuOpen { cfg, version });
}

pub fn place_menu(app: &AppHandle, w: f64, h: f64) {
    let Some(win) = app.get_webview_window("menu") else { return };
    let Some((ax, ay)) = *app.state::<AppState>().menu.lock().unwrap() else { return };
    let mon = app.monitor_from_point(ax, ay).ok().flatten().or_else(|| app.primary_monitor().ok().flatten());
    let Some(mon) = mon else { return };
    let sf = mon.scale_factor();
    let (pw, ph) = (w * sf, h * sf);
    let wa = mon.work_area();
    let (l, t) = (wa.position.x as f64, wa.position.y as f64);
    let (r, b) = (l + wa.size.width as f64, t + wa.size.height as f64);
    let x = (ax - pw + 8.0 * sf).clamp(l, (r - pw).max(l));
    let y = (ay - ph).clamp(t, (b - ph).max(t));
    let _ = win.set_size(PhysicalSize::new(pw.round() as u32, ph.round() as u32));
    crate::platform::set_region(&win, &[crate::hit::Part { x: 0.0, y: 0.0, w, h, r: 16.0 }]);
    let _ = win.set_position(PhysicalPosition::new(x.round() as i32, y.round() as i32));
    let _ = win.show();
    let _ = win.set_focus();
}

pub fn hide_menu(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("menu") {
        let _ = w.hide();
    }
}

pub fn handle(app: &AppHandle, id: &str) {
    if let Some(m) = id.strip_prefix("mode:") {
        let m = m.to_string();
        crate::update(app, |c| {
            c.mode = m;
            c.visible = true;
        });
    } else if let Some(l) = id.strip_prefix("level:") {
        let l = l.to_string();
        crate::update(app, |c| c.level = l);
    } else {
        match id {
            "show" => crate::update(app, |c| c.visible = !c.visible),
            "move" => crate::update(app, |c| {
                c.move_enabled = !c.move_enabled;
                c.visible = true;
            }),
            "snap" => crate::update(app, |c| c.snap = !c.snap),
            "sleep" => crate::widget_action(app.clone(), "sleep".into()),
            "settings" => crate::open_settings(app, None),
            "info" => crate::open_settings(app, Some("info".into())),
            "quit" => app.exit(0),
            _ => {}
        }
    }
}

/// The menu is redrawn every time it opens: there is nothing to keep in sync.
pub fn sync(_app: &AppHandle, _c: &Config) {}
