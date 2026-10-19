//! Icona nell'area di notifica e il suo menu (usato anche col clic destro sul widget).

use crate::{config::Config, AppState};
use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, WebviewWindow, Wry};

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

pub struct MenuHandles {
    pub menu: Menu<Wry>,
    show: CheckMenuItem<Wry>,
    modes: Vec<(&'static str, CheckMenuItem<Wry>)>,
    levels: Vec<(&'static str, CheckMenuItem<Wry>)>,
    mv: CheckMenuItem<Wry>,
    snap: CheckMenuItem<Wry>,
}

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let c = app.state::<AppState>().cfg.lock().unwrap().clone();
    let show = CheckMenuItem::with_id(app, "show", "Mostra il widget", true, c.visible, None::<&str>)?;
    let modes: Vec<_> = MODES
        .iter()
        .map(|(id, name)| Ok((*id, CheckMenuItem::with_id(app, format!("mode:{id}"), *name, true, c.mode == *id, None::<&str>)?)))
        .collect::<tauri::Result<_>>()?;
    let levels: Vec<_> = LEVELS
        .iter()
        .map(|(id, name)| Ok((*id, CheckMenuItem::with_id(app, format!("level:{id}"), *name, true, c.level == *id, None::<&str>)?)))
        .collect::<tauri::Result<_>>()?;
    let mode_refs: Vec<&dyn tauri::menu::IsMenuItem<Wry>> = modes.iter().map(|(_, i)| i as _).collect();
    let level_refs: Vec<&dyn tauri::menu::IsMenuItem<Wry>> = levels.iter().map(|(_, i)| i as _).collect();
    let mode_sub = Submenu::with_items(app, "Modalità", true, &mode_refs)?;
    let level_sub = Submenu::with_items(app, "Livello", true, &level_refs)?;
    let mv = CheckMenuItem::with_id(app, "move", "Sposta il widget", true, c.move_enabled, None::<&str>)?;
    let snap = CheckMenuItem::with_id(app, "snap", "Aggancio alla griglia", true, c.snap, None::<&str>)?;
    let sleep = MenuItem::with_id(app, "sleep", "Fai dormire / sveglia la mascotte", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Impostazioni…", true, None::<&str>)?;
    let info = MenuItem::with_id(app, "info", "Info su Glyphling", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Esci", true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &show,
            &mode_sub,
            &level_sub,
            &PredefinedMenuItem::separator(app)?,
            &mv,
            &snap,
            &sleep,
            &PredefinedMenuItem::separator(app)?,
            &settings,
            &info,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

    TrayIconBuilder::with_id("main")
        .icon(Image::from_bytes(include_bytes!("../icons/tray.png"))?)
        .tooltip("Glyphling")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, ev| handle(app, ev.id().as_ref()))
        .on_tray_icon_event(|tray, ev| {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Up, .. } = ev {
                crate::update(tray.app_handle(), |c| c.visible = !c.visible);
            }
        })
        .build(app)?;

    *app.state::<AppState>().menu.lock().unwrap() = Some(MenuHandles { menu, show, modes, levels, mv, snap });
    Ok(())
}

fn handle(app: &AppHandle, id: &str) {
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
    // le voci a spunta si invertono da sole al clic: le riallineo allo stato vero
    let c = app.state::<AppState>().cfg.lock().unwrap().clone();
    sync(app, &c);
}

/// Allinea le spunte del menu alle impostazioni.
pub fn sync(app: &AppHandle, c: &Config) {
    let state = app.state::<AppState>();
    let guard = state.menu.lock().unwrap();
    let Some(h) = guard.as_ref() else { return };
    let _ = h.show.set_checked(c.visible);
    let _ = h.mv.set_checked(c.move_enabled);
    let _ = h.snap.set_checked(c.snap);
    for (id, item) in &h.modes {
        let _ = item.set_checked(c.mode == *id);
    }
    for (id, item) in &h.levels {
        let _ = item.set_checked(c.level == *id);
    }
}

pub fn popup(app: &AppHandle, w: &WebviewWindow) {
    // copia del menu fuori dal lock: il menu a comparsa blocca finché non si chiude
    let menu = app.state::<AppState>().menu.lock().unwrap().as_ref().map(|h| h.menu.clone());
    if let Some(menu) = menu {
        let _ = w.popup_menu(&menu);
    }
}
