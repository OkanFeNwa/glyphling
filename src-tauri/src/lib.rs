//! Glyphling: widget desktop in stile Nothing OS.
//!
//! Tre finestre condividono la stessa interfaccia web (`src/`):
//! - `widget`: il cerchio e il pannello, trasparente e senza bordi;
//! - `settings`: le impostazioni, creata solo quando serve;
//! - `grid`: la griglia di punti mostrata durante il trascinamento.
//! Lo stato vive qui (`Config`); ogni cambiamento viene salvato e mandato a tutte le finestre.

mod config;
mod hit;
pub mod media;
mod platform;
mod snap;
mod spectrum;
mod system;
mod tray;

use config::Config;
use serde::Serialize;
use std::sync::atomic::AtomicBool;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent};

/// Geometria del widget in pixel logici (deve corrispondere a src/host.css e al prototipo).
pub const PAD_X: f64 = 40.0;
pub const PAD_Y: f64 = 34.0;
pub const D: f64 = 172.0;
pub const OPEN_W: f64 = 646.0;
const WIN_W: f64 = 730.0;
const WIN_H: f64 = 262.0;

pub struct AppState {
    pub cfg: Mutex<Config>,
    pub shape: Mutex<hit::Shape>,
    pub dragging: AtomicBool,
    /// Qualcosa sta suonando (dal thread della musica, serve al visualizer).
    pub playing: AtomicBool,
    pub settings_page: Mutex<Option<String>>,
    pub menu: Mutex<Option<tray::MenuHandles>>,
}

/// Applica una modifica alle impostazioni; se cambia qualcosa la salva, la applica
/// alle finestre, aggiorna il menu e la manda a tutte le pagine.
pub fn update(app: &AppHandle, f: impl FnOnce(&mut Config)) {
    let state = app.state::<AppState>();
    let (before, after) = {
        let mut c = state.cfg.lock().unwrap();
        let before = c.clone();
        f(&mut c);
        if !tray::MODES.iter().any(|(id, _)| *id == c.mode) {
            c.mode = before.mode.clone();
        }
        (before, c.clone())
    };
    if before == after {
        return;
    }
    config::save(app, &after);
    if let Some(w) = app.get_webview_window("widget") {
        if before.visible != after.visible {
            let _ = if after.visible { w.show() } else { w.hide() };
        }
        if before.level != after.level {
            apply_level(&w, &after.level);
        }
    }
    tray::sync(app, &after);
    let _ = app.emit("config", &after);
}

fn apply_level(w: &WebviewWindow, level: &str) {
    match level {
        "top" => {
            let _ = w.set_always_on_bottom(false);
            let _ = w.set_always_on_top(true);
        }
        "normal" => {
            let _ = w.set_always_on_top(false);
            let _ = w.set_always_on_bottom(false);
        }
        _ => {
            let _ = w.set_always_on_top(false);
            let _ = w.set_always_on_bottom(true);
        }
    }
}

pub fn open_settings(app: &AppHandle, page: Option<String>) {
    if let Some(w) = app.get_webview_window("settings") {
        let _ = w.unminimize();
        let _ = w.show();
        let _ = w.set_focus();
        if let Some(p) = page {
            let _ = w.emit("settings-page", p);
        }
        return;
    }
    *app.state::<AppState>().settings_page.lock().unwrap() = page;
    // la finestra si crea da un altro thread: crearla dentro un gestore di eventi può bloccare Windows
    let app = app.clone();
    std::thread::spawn(move || {
        let _ = WebviewWindowBuilder::new(&app, "settings", WebviewUrl::App("index.html".into()))
            .title("Glyphling · Impostazioni")
            .inner_size(820.0, 720.0)
            .min_inner_size(560.0, 480.0)
            .theme(Some(tauri::Theme::Dark))
            .center()
            .build();
    });
}

// ---------- comandi chiamati dall'interfaccia ----------

#[tauri::command]
fn get_config(state: tauri::State<AppState>) -> Config {
    state.cfg.lock().unwrap().clone()
}

#[tauri::command]
fn set_mode(app: AppHandle, mode: String) {
    update(&app, |c| c.mode = mode);
}

#[tauri::command]
fn set_move(app: AppHandle, on: bool) {
    update(&app, |c| {
        c.move_enabled = on;
        if on {
            c.visible = true;
        }
    });
}

#[tauri::command]
fn set_snap(app: AppHandle, on: bool) {
    update(&app, |c| c.snap = on);
}

#[tauri::command]
fn set_shape(state: tauri::State<AppState>, shape: hit::Shape) {
    *state.shape.lock().unwrap() = shape;
}

#[tauri::command]
fn start_drag(app: AppHandle, window: WebviewWindow, color: Option<String>) -> Result<(), String> {
    snap::start_drag(&app, &window, color).map_err(|e| e.to_string())
}

#[tauri::command]
fn reset_position(app: AppHandle) {
    snap::reset(&app);
}

#[tauri::command]
fn widget_action(app: AppHandle, action: String) {
    let _ = app.emit_to("widget", "action", action);
}

#[tauri::command]
fn take_settings_page(state: tauri::State<AppState>) -> Option<String> {
    state.settings_page.lock().unwrap().take()
}

#[tauri::command]
fn popup_menu(app: AppHandle, window: WebviewWindow) {
    tray::popup(&app, &window);
}

#[tauri::command]
fn get_autostart(app: AppHandle) -> bool {
    use tauri_plugin_autostart::ManagerExt;
    app.autolaunch().is_enabled().unwrap_or(false)
}

#[tauri::command]
fn set_autostart(app: AppHandle, on: bool) -> Result<bool, String> {
    use tauri_plugin_autostart::ManagerExt;
    let a = app.autolaunch();
    let r = if on { a.enable() } else { a.disable() };
    r.map_err(|e| e.to_string())?;
    Ok(a.is_enabled().unwrap_or(false))
}

#[tauri::command]
async fn media_control(action: String, value: Option<f64>) -> bool {
    tauri::async_runtime::spawn_blocking(move || {
        media::com_init();
        media::control(&action, value.unwrap_or(0.0))
    })
    .await
    .unwrap_or(false)
}

#[tauri::command]
async fn set_volume(v: f32) {
    let _ = tauri::async_runtime::spawn_blocking(move || media::set_volume(v)).await;
}

/// Scarica un testo (per il calendario iCal): solo https, al massimo 5 MB.
#[tauri::command]
async fn fetch_text(url: String) -> Result<String, String> {
    if !url.starts_with("https://") {
        return Err("serve un indirizzo https".into());
    }
    tauri::async_runtime::spawn_blocking(move || {
        use std::io::Read;
        let r = ureq::get(&url).timeout(std::time::Duration::from_secs(20)).call().map_err(|e| e.to_string())?;
        let mut s = String::new();
        r.into_reader().take(5_000_000).read_to_string(&mut s).map_err(|e| e.to_string())?;
        Ok(s)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Serialize)]
struct AppInfo {
    version: String,
    os: String,
    position: String,
    config_dir: String,
}

/// Elenco dei dischi per le impostazioni, anche quando la modalità Sistema non è attiva.
#[tauri::command]
async fn list_disks() -> Vec<system::Disk> {
    tauri::async_runtime::spawn_blocking(system::disk_list).await.unwrap_or_default()
}

#[tauri::command]
fn app_info(app: AppHandle) -> AppInfo {
    let position = app
        .get_webview_window("widget")
        .and_then(|w| {
            let p = w.outer_position().ok()?;
            let m = w.current_monitor().ok().flatten();
            let name = m.as_ref().and_then(|m| m.name().cloned()).unwrap_or_else(|| "monitor".into());
            let name = name.trim_start_matches(r"\\.\").to_string();
            Some(format!("{}, {} px · {}", p.x, p.y, name))
        })
        .unwrap_or_else(|| "—".into());
    AppInfo {
        version: app.package_info().version.to_string(),
        os: platform::os_name(),
        position,
        config_dir: config::dir(&app).map(|d| d.display().to_string()).unwrap_or_default(),
    }
}

pub fn run() {
    tauri::Builder::default()
        // una sola copia: un secondo avvio apre le impostazioni di quella già attiva
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| open_settings(app, None)))
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, None))
        .setup(|app| {
            let handle = app.handle().clone();
            let cfg = config::load(&handle);
            app.manage(AppState {
                cfg: Mutex::new(cfg.clone()),
                shape: Mutex::new(hit::Shape::default()),
                dragging: AtomicBool::new(false),
                playing: AtomicBool::new(false),
                settings_page: Mutex::new(None),
                menu: Mutex::new(None),
            });

            let widget = WebviewWindowBuilder::new(app, "widget", WebviewUrl::App("index.html".into()))
                .title("Glyphling")
                .inner_size(WIN_W, WIN_H)
                .resizable(false)
                .maximizable(false)
                .decorations(false)
                .transparent(true)
                .shadow(false)
                .skip_taskbar(true)
                .focused(false)
                .visible(false)
                .build()?;
            snap::restore(&widget, &cfg);
            apply_level(&widget, &cfg.level);
            if cfg.visible {
                widget.show()?;
            }

            let grid = WebviewWindowBuilder::new(app, "grid", WebviewUrl::App("grid.html".into()))
                .title("Glyphling · griglia")
                .decorations(false)
                .transparent(true)
                .shadow(false)
                .skip_taskbar(true)
                .always_on_top(true)
                .resizable(false)
                .focused(false)
                .visible(false)
                .build()?;
            #[cfg(windows)]
            grid.set_ignore_cursor_events(true)?;

            tray::create(&handle)?;
            hit::spawn(handle.clone());
            system::spawn(handle.clone());
            media::spawn(handle.clone());
            spectrum::spawn(handle);
            Ok(())
        })
        .on_window_event(|window, event| {
            // Alt+F4 sul widget lo nasconde invece di chiudere l'app
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "widget" {
                    api.prevent_close();
                    update(window.app_handle(), |c| c.visible = false);
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_config,
            list_disks,
            set_mode,
            set_move,
            set_snap,
            set_shape,
            start_drag,
            reset_position,
            widget_action,
            take_settings_page,
            popup_menu,
            get_autostart,
            media_control,
            set_volume,
            fetch_text,
            set_autostart,
            app_info
        ])
        .run(tauri::generate_context!())
        .expect("errore all'avvio di Glyphling");
}
