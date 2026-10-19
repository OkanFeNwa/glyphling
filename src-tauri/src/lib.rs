//! Glyphling: a Nothing OS style desktop widget.
//!
//! Three windows share the same web interface (`src/`):
//! - `widget`: the circle and the panel, transparent and borderless;
//! - `settings`: the settings, created only when needed;
//! - `grid`: the dot grid shown while dragging.
//! The state lives here (`Config`); every change is saved and sent to all windows.

mod config;
mod hit;
pub mod media;
mod sensor;
mod updater;
mod platform;
mod snap;
mod spectrum;
mod system;
mod tray;

use config::Config;
use serde::Serialize;
use std::sync::atomic::AtomicBool;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent, PhysicalPosition};

/// Widget geometry in logical pixels (must match src/host.css and the prototype).
pub const PAD_X: f64 = 40.0;
pub const PAD_Y: f64 = 60.0; // room above the circle for the dial label
pub const D: f64 = 172.0;
/// Width of the open widget from the circle's left edge to the end of the panel (--pw).
pub const OPEN_W: f64 = 560.0;
pub const WIN_W: f64 = 730.0;
const WIN_H: f64 = 288.0;

pub struct AppState {
    pub cfg: Mutex<Config>,
    pub shape: Mutex<hit::Shape>,
    pub dragging: AtomicBool,
    /// Something is playing (set by the music thread, used by the visualizer).
    pub playing: AtomicBool,
    pub settings_page: Mutex<Option<String>>,
    /// Point (physical pixels) the tray icon menu is anchored to.
    pub menu: Mutex<Option<(f64, f64)>>,
    /// While the first-run tutorial runs, the widget window covers the whole monitor;
    /// this is where it was before (physical pixels).
    pub tutorial: Mutex<Option<PhysicalPosition<i32>>>,
    pub update: Mutex<Option<updater::Update>>,
}

pub fn in_tutorial(app: &AppHandle) -> bool {
    app.state::<AppState>().tutorial.lock().unwrap().is_some()
}

/// Applies a change to the settings; if anything changed, saves it, applies it
/// to the windows, updates the menu and sends it to all pages.
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
            if after.visible {
                // after "Show desktop" the window may be minimized: show() alone is not enough
                let _ = w.unminimize();
                let _ = w.show();
                apply_level(&w, &after.level);
            } else {
                let _ = w.hide();
            }
        }
        if before.level != after.level {
            apply_level(&w, &after.level);
        }
    }
    tray::sync(app, &after);
    let _ = app.emit("config", &after);
    if (before.mode == "idle") != (after.mode == "idle") {
        // the circle slides while the panel opens (0.6 s in the page)
        snap::layout(app, 40);
    }
}

fn apply_level(w: &WebviewWindow, level: &str) {
    platform::set_desktop_owner(w, level == "desktop");
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
    // create the window from another thread: creating it inside an event handler can deadlock on Windows
    let app = app.clone();
    std::thread::spawn(move || {
        let built = WebviewWindowBuilder::new(&app, "settings", WebviewUrl::App("index.html".into()))
            .title("Glyphling · Impostazioni")
            .inner_size(840.0, 740.0)
            // no Windows frame: the page draws the title bar and buttons
            .decorations(false)
            .transparent(true)
            .shadow(false)
            .resizable(false)
            .theme(Some(tauri::Theme::Dark))
            .center()
            .build();
        if let Ok(w) = built {
            // rounded corners like the page's glass
            platform::set_region(&w, &[hit::Part { x: 0.0, y: 0.0, w: 840.0, h: 740.0, r: 30.0 }]);
            platform::no_frame(&w);
            // Windows can bring the frame back when the focus changes
            let wc = w.clone();
            w.on_window_event(move |e| {
                if matches!(e, WindowEvent::Focused(_) | WindowEvent::Moved(_) | WindowEvent::ThemeChanged(_)) {
                    platform::strip_frame(&wc);
                }
            });
        }
    });
}

// ---------- commands called by the interface ----------

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

/// Quick position: tl, tc, tr, ml, c, mr, bl, bc, br.
#[tauri::command]
fn set_preset(app: AppHandle, id: String) {
    snap::set_preset(&app, &id);
}

/// Marks the first-run tutorial as shown, so it never opens by itself again.
#[tauri::command]
fn set_tutorial_done(app: AppHandle) {
    update(&app, |c| c.tutorial_done = true);
}

/// Side the panel opens on: right, left or auto.
/// Which apps may be the music source (see `media::Filter`).
#[tauri::command]
fn set_music_filter(app: AppHandle, mode: String, apps: Vec<String>) {
    if !["all", "allow", "block"].contains(&mode.as_str()) {
        return;
    }
    let apps: Vec<String> = apps.iter().map(|a| media::compact(a)).filter(|a| a.len() >= 3).take(64).collect();
    media::set_filter(&mode, &apps);
    update(&app, |c| {
        c.music_filter = mode;
        c.music_apps = apps;
    });
}

/// Newer version found so far, if any.
#[tauri::command]
fn update_state(state: tauri::State<AppState>) -> Option<updater::Update> {
    state.update.lock().unwrap().clone()
}

/// Manual check from Settings.
#[tauri::command]
async fn check_update(app: AppHandle) -> Result<Option<updater::Update>, String> {
    tauri::async_runtime::spawn_blocking(move || updater::check(&app, false)).await.map_err(|e| e.to_string())?
}

/// Downloads the new setup and runs it; the app closes.
#[tauri::command]
async fn install_update(app: AppHandle) -> Result<(), String> {
    let u = app.state::<AppState>().update.lock().unwrap().clone().ok_or("nessun aggiornamento")?;
    tauri::async_runtime::spawn_blocking(move || updater::install(&app, &u)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
fn set_auto_update(app: AppHandle, on: bool) {
    update(&app, |c| c.auto_update = on);
}

#[derive(serde::Serialize)]
struct SensorState {
    installed: bool,
    /// LibreHardwareMonitor's web server answers.
    running: bool,
}

/// State of the optional CPU temperature sensor (installed, running).
#[tauri::command]
async fn sensor_state(app: AppHandle) -> SensorState {
    tauri::async_runtime::spawn_blocking(move || SensorState { installed: sensor::installed(&app), running: sensor::web_alive() })
        .await
        .unwrap_or(SensorState { installed: false, running: false })
}

/// Starts the installed sensor again.
#[tauri::command]
async fn start_sensor() -> bool {
    tauri::async_runtime::spawn_blocking(|| {
        sensor::start();
        for _ in 0..10 {
            if sensor::web_alive() {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
        false
    })
    .await
    .unwrap_or(false)
}

/// Downloads and starts LibreHardwareMonitor (the user already agreed in Settings).
#[tauri::command]
async fn install_sensor(app: AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || sensor::install(&app)).await.map_err(|e| e.to_string())?
}

/// The first time the temperature is unavailable, Settings opens once on the offer.
/// Returns "wait" while the tutorial is running, so the page asks again later.
#[tauri::command]
fn sensor_offer(app: AppHandle) -> &'static str {
    if in_tutorial(&app) {
        return "wait";
    }
    if sensor::installed(&app) {
        return "done";
    }
    let mut first = false;
    update(&app, |c| {
        first = !c.sensor_offered;
        c.sensor_offered = true;
    });
    if first {
        open_settings(&app, Some("gen".into()));
    }
    "done"
}

/// Name of the app the music mode is following right now.
#[tauri::command]
fn music_now() -> String {
    media::now_app()
}

/// Apps that have a media session right now (for adding a custom app): (key, name).
#[tauri::command]
async fn music_sessions() -> Vec<(String, String)> {
    tauri::async_runtime::spawn_blocking(|| {
        media::com_init();
        media::sessions()
    })
    .await
    .unwrap_or_default()
}

#[tauri::command]
fn set_panel_dir(app: AppHandle, dir: String) {
    if !["right", "left", "auto"].contains(&dir.as_str()) {
        return;
    }
    update(&app, |c| c.panel_dir = dir);
    snap::layout(&app, 10);
}

#[tauri::command]
fn set_shape(window: WebviewWindow, state: tauri::State<AppState>, shape: hit::Shape) {
    // during the tutorial the window covers the screen and stays whole
    if state.tutorial.lock().unwrap().is_none() {
        platform::set_region(&window, &shape.parts);
    }
    *state.shape.lock().unwrap() = shape;
}

#[derive(serde::Serialize)]
struct TutorialPlace {
    /// Where the widget's usual window sits inside the enlarged one (logical pixels).
    dx: f64,
    dy: f64,
    /// The tray icon, if Windows says where it is (logical pixels, relative to the window).
    tray: Option<[f64; 4]>,
}

/// Tutorial on: the widget window grows to cover its monitor (taskbar included, so the
/// tray icon can be pointed at) without moving the widget on screen. Off: back as it was.
#[tauri::command]
fn tutorial(app: AppHandle, window: WebviewWindow, on: bool) -> Option<TutorialPlace> {
    let state = app.state::<AppState>();
    if !on {
        let saved = state.tutorial.lock().unwrap().take();
        if let Some(p) = saved {
            let _ = window.set_size(tauri::LogicalSize::new(WIN_W, WIN_H));
            let _ = window.set_position(p);
            let level = state.cfg.lock().unwrap().level.clone();
            apply_level(&window, &level);
            let shape = state.shape.lock().unwrap().clone();
            platform::set_region(&window, &shape.parts);
        }
        return None;
    }
    let pos = window.outer_position().ok()?;
    let sf = window.scale_factor().ok()?;
    let mon = window.current_monitor().ok().flatten()?;
    let (mp, ms) = (*mon.position(), *mon.size());
    *state.tutorial.lock().unwrap() = Some(pos);
    platform::set_region(&window, &[]);
    let _ = window.set_ignore_cursor_events(false);
    platform::set_desktop_owner(&window, false);
    let _ = window.set_always_on_bottom(false);
    let _ = window.set_always_on_top(true);
    let _ = window.set_position(mp);
    let _ = window.set_size(ms);
    let _ = window.set_focus();
    let tray = app.tray_by_id("main").and_then(|t| t.rect().ok().flatten()).map(|r| {
        let p = r.position.to_physical::<f64>(sf);
        let z = r.size.to_physical::<f64>(sf);
        [(p.x - mp.x as f64) / sf, (p.y - mp.y as f64) / sf, z.width / sf, z.height / sf]
    });
    Some(TutorialPlace { dx: (pos.x - mp.x) as f64 / sf, dy: (pos.y - mp.y) as f64 / sf, tray })
}

#[tauri::command]
fn start_drag(app: AppHandle, window: WebviewWindow, color: Option<String>) -> Result<(), String> {
    snap::start_drag(&app, &window, color).map_err(|e| e.to_string())
}

#[tauri::command]
fn reset_position(app: AppHandle) {
    snap::set_preset(&app, "c");
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
fn menu_place(app: AppHandle, w: f64, h: f64) {
    tray::place_menu(&app, w, h);
}

#[tauri::command]
fn menu_hide(app: AppHandle) {
    tray::hide_menu(&app);
}

#[tauri::command]
fn menu_action(app: AppHandle, id: String) {
    tray::hide_menu(&app);
    tray::handle(&app, &id);
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

/// Downloads a text file (for the iCal calendar): https only, 5 MB at most.
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

/// Disk list for the settings, even when the System mode is not active.
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
        // single instance: a second launch opens the settings of the running one
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| open_settings(app, None)))
        .plugin(tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, None))
        .setup(|app| {
            let handle = app.handle().clone();
            let cfg = config::load(&handle);
            media::set_filter(&cfg.music_filter, &cfg.music_apps);
            app.manage(AppState {
                cfg: Mutex::new(cfg.clone()),
                shape: Mutex::new(hit::Shape::default()),
                dragging: AtomicBool::new(false),
                playing: AtomicBool::new(false),
                settings_page: Mutex::new(None),
                menu: Mutex::new(None),
                tutorial: Mutex::new(None),
                update: Mutex::new(None),
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
            snap::layout(app.handle(), 0);
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

            // tray icon menu: glass window, hidden until the icon is right-clicked
            let menu = WebviewWindowBuilder::new(app, "menu", WebviewUrl::App("menu.html".into()))
                .title("Glyphling · menu")
                .decorations(false)
                .transparent(true)
                .shadow(false)
                .skip_taskbar(true)
                .always_on_top(true)
                .resizable(false)
                .focused(false)
                .visible(false)
                .inner_size(300.0, 600.0)
                .build()?;
            let mh = handle.clone();
            menu.on_window_event(move |ev| {
                if let tauri::WindowEvent::Focused(false) = ev {
                    tray::hide_menu(&mh);
                }
            });
            // the widget must stay visible even after "Show desktop" or other Windows tricks
            let wd = handle.clone();
            std::thread::spawn(move || loop {
                std::thread::sleep(std::time::Duration::from_millis(700));
                let st = wd.state::<AppState>();
                let (visible, level) = { let c = st.cfg.lock().unwrap(); (c.visible, c.level.clone()) };
                if !visible || st.dragging.load(std::sync::atomic::Ordering::Relaxed) {
                    continue;
                }
                if let Some(w) = wd.get_webview_window("widget") {
                    if platform::ensure_shown(&w) {
                        apply_level(&w, &level);
                    }
                }
            });
            tray::create(&handle)?;
            hit::spawn(handle.clone());
            system::spawn(handle.clone());
            media::spawn(handle.clone());
            updater::spawn(handle.clone());
            sensor::spawn(handle.clone());
            spectrum::spawn(handle);
            Ok(())
        })
        .on_window_event(|window, event| {
            // Alt+F4 on the widget hides it instead of closing the app
            if let WindowEvent::CloseRequested { api, .. } = event {
                if window.label() == "widget" {
                    api.prevent_close();
                    update(window.app_handle(), |c| c.visible = false);
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_config,
            set_panel_dir,
            set_music_filter,
            music_sessions,
            music_now,
            sensor_state,
            start_sensor,
            install_sensor,
            sensor_offer,
            update_state,
            check_update,
            install_update,
            set_auto_update,
            set_preset,
            tutorial,
            set_tutorial_done,
            list_disks,
            set_mode,
            set_move,
            set_snap,
            set_shape,
            start_drag,
            reset_position,
            widget_action,
            take_settings_page,
            menu_place,
            menu_hide,
            menu_action,
            get_autostart,
            media_control,
            set_volume,
            fetch_text,
            set_autostart,
            app_info
        ])
        .run(tauri::generate_context!())
        .expect("error while starting Glyphling");
}
