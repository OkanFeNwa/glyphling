//! Glyphling setup: a custom installer, updater and uninstaller in a single executable.
//!
//! - Run normally it installs Glyphling for the current user, or updates it in place when an
//!   older version is already installed (no uninstall step, settings are kept).
//! - Run with `--uninstall` (the copy saved as `uninstall.exe` in the install folder, which is
//!   what Windows "Apps" calls) it removes the app.
//!
//! The window is a frameless page (ui/index.html) with blur behind it; the work happens here.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use serde::Serialize;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::{AppHandle, Emitter, WebviewUrl, WebviewWindowBuilder};

const VERSION: &str = env!("GLYPHLING_VERSION");
const PAYLOAD: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/payload.bin"));
/// Same key, folder and shortcut names as the earlier NSIS installer, so updates land on top of it.
const PRODUCT: &str = "Glyphling";
const EXE: &str = "glyphling.exe";
const UNINSTALL_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\Glyphling";
const IDENTIFIER: &str = "com.okanfenwa.glyphling";
const WIN_W: f64 = 760.0;
const WIN_H: f64 = 460.0;

fn install_dir() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    base.join(PRODUCT)
}

fn start_menu_link() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join(r"Microsoft\Windows\Start Menu\Programs").join(format!("{PRODUCT}.lnk")))
}

fn desktop_link() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE").map(|u| PathBuf::from(u).join("Desktop").join(format!("{PRODUCT}.lnk")))
}

#[derive(Serialize, Clone)]
struct State {
    mode: &'static str,
    version: String,
    installed: Option<String>,
    dir: String,
    has_payload: bool,
}

#[derive(Serialize, Clone)]
struct Progress {
    step: String,
    value: f64,
}

fn progress(app: &AppHandle, step: &str, value: f64) {
    let _ = app.emit("progress", Progress { step: step.into(), value });
}

fn uninstall_mode() -> bool {
    std::env::args().any(|a| a == "--uninstall")
}

// ---------- registry, processes and shortcuts (Windows) ----------

#[cfg(windows)]
mod win {
    use super::*;
    use std::os::windows::process::CommandExt;
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    use winreg::RegKey;

    const NO_WINDOW: u32 = 0x0800_0000;

    pub fn installed_version() -> Option<String> {
        RegKey::predef(HKEY_CURRENT_USER).open_subkey(UNINSTALL_KEY).ok()?.get_value("DisplayVersion").ok()
    }

    pub fn write_uninstall_entry(dir: &Path, size_kb: u64) -> std::io::Result<()> {
        let (key, _) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(UNINSTALL_KEY)?;
        let exe = dir.join(EXE);
        let un = dir.join("uninstall.exe");
        key.set_value("DisplayName", &PRODUCT)?;
        key.set_value("DisplayVersion", &VERSION)?;
        key.set_value("Publisher", &"OkanFeNwa")?;
        key.set_value("DisplayIcon", &format!("{},0", exe.display()))?;
        key.set_value("InstallLocation", &dir.display().to_string())?;
        key.set_value("UninstallString", &format!("\"{}\" --uninstall", un.display()))?;
        key.set_value("QuietUninstallString", &format!("\"{}\" --uninstall", un.display()))?;
        key.set_value("EstimatedSize", &(size_kb as u32))?;
        key.set_value("NoModify", &1u32)?;
        key.set_value("NoRepair", &1u32)?;
        Ok(())
    }

    pub fn remove_uninstall_entry() {
        let _ = RegKey::predef(HKEY_CURRENT_USER).delete_subkey_all(UNINSTALL_KEY);
        // start with Windows (written by the app through tauri-plugin-autostart)
        if let Ok(run) = RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(r"Software\Microsoft\Windows\CurrentVersion\Run", winreg::enums::KEY_ALL_ACCESS) {
            let _ = run.delete_value(PRODUCT);
        }
    }

    /// The setup page needs the WebView2 runtime, like the app.
    pub fn webview2_installed() -> bool {
        const CLIENT: &str = r"Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}";
        let has = |root: RegKey, path: String| {
            root.open_subkey(path).ok().and_then(|k| k.get_value::<String, _>("pv").ok()).is_some_and(|v| !v.is_empty() && v != "0.0.0.0")
        };
        has(RegKey::predef(HKEY_LOCAL_MACHINE), format!(r"SOFTWARE\WOW6432Node\{CLIENT}"))
            || has(RegKey::predef(HKEY_LOCAL_MACHINE), format!(r"SOFTWARE\{CLIENT}"))
            || has(RegKey::predef(HKEY_CURRENT_USER), format!(r"Software\{CLIENT}"))
    }

    pub fn message(text: &str, question: bool) -> bool {
        use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, IDYES, MB_ICONINFORMATION, MB_OK, MB_YESNO};
        let w = |s: &str| s.encode_utf16().chain(Some(0)).collect::<Vec<u16>>();
        let (t, c) = (w(text), w("Glyphling"));
        let flags = if question { MB_YESNO | MB_ICONINFORMATION } else { MB_OK | MB_ICONINFORMATION };
        unsafe { MessageBoxW(std::ptr::null_mut(), t.as_ptr(), c.as_ptr(), flags) == IDYES }
    }

    pub fn install_webview2() -> bool {
        let tmp = std::env::temp_dir().join("MicrosoftEdgeWebview2Setup.exe");
        let Ok(resp) = ureq::get("https://go.microsoft.com/fwlink/p/?LinkId=2124703").call() else { return false };
        let mut data = Vec::new();
        if std::io::Read::read_to_end(&mut resp.into_reader(), &mut data).is_err() || std::fs::write(&tmp, data).is_err() {
            return false;
        }
        std::process::Command::new(&tmp).args(["/silent", "/install"]).status().is_ok_and(|s| s.success())
    }

    /// Closes the running app (its windows only hide on close, so it is ended by name).
    pub fn stop_app() {
        let _ = std::process::Command::new("taskkill").args(["/F", "/IM", EXE]).creation_flags(NO_WINDOW).status();
    }

    pub fn shortcut(link: &Path, target: &Path) {
        let ps = format!(
            "$s=(New-Object -ComObject WScript.Shell).CreateShortcut('{}');$s.TargetPath='{}';$s.WorkingDirectory='{}';$s.IconLocation='{},0';$s.Save()",
            link.display().to_string().replace('\'', "''"),
            target.display().to_string().replace('\'', "''"),
            target.parent().map(|p| p.display().to_string()).unwrap_or_default().replace('\'', "''"),
            target.display().to_string().replace('\'', "''"),
        );
        let _ = std::process::Command::new("powershell").args(["-NoProfile", "-NonInteractive", "-Command", &ps]).creation_flags(NO_WINDOW).status();
    }

    pub fn launch(exe: &Path) {
        let _ = std::process::Command::new(exe).current_dir(exe.parent().unwrap_or(Path::new("."))).spawn();
    }

    /// The uninstaller cannot delete itself while running: a hidden cmd waits for it to exit.
    pub fn delete_later(dir: &Path) {
        let cmd = format!("ping -n 3 127.0.0.1 >nul & rmdir /s /q \"{}\"", dir.display());
        let _ = std::process::Command::new("cmd").args(["/C", &cmd]).creation_flags(NO_WINDOW).spawn();
    }

    /// Asks Windows 11 for its own antialiased rounded corners (Windows 10 keeps square ones).
    pub fn round(w: &tauri::WebviewWindow) {
        use windows_sys::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND};
        let Ok(h) = w.hwnd() else { return };
        let pref = DWMWCP_ROUND;
        unsafe {
            DwmSetWindowAttribute(h.0 as _, DWMWA_WINDOW_CORNER_PREFERENCE as u32, &pref as *const _ as *const _, std::mem::size_of_val(&pref) as u32);
        }
    }
}

#[cfg(not(windows))]
mod win {
    use super::*;
    pub fn installed_version() -> Option<String> {
        None
    }
    pub fn write_uninstall_entry(_: &Path, _: u64) -> std::io::Result<()> {
        Ok(())
    }
    pub fn remove_uninstall_entry() {}
    pub fn webview2_installed() -> bool {
        true
    }
    pub fn message(text: &str, _: bool) -> bool {
        eprintln!("{text}");
        false
    }
    pub fn install_webview2() -> bool {
        false
    }
    pub fn stop_app() {}
    pub fn shortcut(_: &Path, _: &Path) {}
    pub fn launch(_: &Path) {}
    pub fn delete_later(_: &Path) {}
    pub fn round(_: &tauri::WebviewWindow) {}
}

// ---------- payload ----------

/// Files in the embedded payload: (name, original size, compressed bytes).
fn payload_files() -> Vec<(String, u64, &'static [u8])> {
    let mut out = vec![];
    let mut p = PAYLOAD;
    let take = |p: &mut &'static [u8], n: usize| -> Option<&'static [u8]> {
        if p.len() < n {
            return None;
        }
        let (a, b) = p.split_at(n);
        *p = b;
        Some(a)
    };
    while !p.is_empty() {
        let Some(l) = take(&mut p, 2) else { break };
        let Some(name) = take(&mut p, u16::from_le_bytes([l[0], l[1]]) as usize) else { break };
        let Some(size) = take(&mut p, 8) else { break };
        let Some(clen) = take(&mut p, 8) else { break };
        let Some(data) = take(&mut p, u64::from_le_bytes(clen.try_into().unwrap()) as usize) else { break };
        out.push((String::from_utf8_lossy(name).into_owned(), u64::from_le_bytes(size.try_into().unwrap()), data));
    }
    out
}

/// Writes a file, retrying for a few seconds in case the old app is still releasing it.
fn write_retry(path: &Path, data: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("new");
    std::fs::write(&tmp, data)?;
    let mut last = Ok(());
    for _ in 0..40 {
        last = std::fs::rename(&tmp, path);
        if last.is_ok() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(150));
    }
    let _ = std::fs::remove_file(&tmp);
    last
}

// ---------- commands ----------

#[tauri::command]
fn get_state() -> State {
    State {
        mode: if uninstall_mode() { "uninstall" } else { "install" },
        version: VERSION.into(),
        installed: win::installed_version(),
        dir: install_dir().display().to_string(),
        has_payload: !PAYLOAD.is_empty(),
    }
}

#[tauri::command]
async fn install(app: AppHandle, desktop: bool) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || do_install(&app, desktop)).await.map_err(|e| e.to_string())?
}

fn do_install(app: &AppHandle, desktop: bool) -> Result<(), String> {
    let files = payload_files();
    if files.is_empty() {
        return Err("Questo setup non contiene l'app.".into());
    }
    let dir = install_dir();
    progress(app, "Chiudo Glyphling se è aperto", 0.04);
    win::stop_app();
    std::fs::create_dir_all(&dir).map_err(|e| format!("Non riesco a creare la cartella: {e}"))?;

    let total: u64 = files.iter().map(|f| f.1).sum::<u64>().max(1);
    let mut done = 0u64;
    for (name, size, packed) in &files {
        progress(app, &format!("Copio {name}"), 0.08 + 0.72 * done as f64 / total as f64);
        let data = miniz_oxide::inflate::decompress_to_vec(packed).map_err(|_| format!("{name} è danneggiato"))?;
        write_retry(&dir.join(name), &data).map_err(|e| format!("Non riesco a scrivere {name}: {e}"))?;
        done += size;
    }

    progress(app, "Preparo la disinstallazione", 0.84);
    let me = std::env::current_exe().map_err(|e| e.to_string())?;
    let un = dir.join("uninstall.exe");
    if me != un {
        let bytes = std::fs::read(&me).map_err(|e| e.to_string())?;
        write_retry(&un, &bytes).map_err(|e| format!("Non riesco a scrivere il disinstallatore: {e}"))?;
    }
    win::write_uninstall_entry(&dir, total / 1024).map_err(|e| format!("Non riesco a registrare l'app: {e}"))?;

    progress(app, "Creo i collegamenti", 0.92);
    let exe = dir.join(EXE);
    if let Some(l) = start_menu_link() {
        win::shortcut(&l, &exe);
    }
    if let Some(l) = desktop_link() {
        if desktop {
            win::shortcut(&l, &exe);
        }
    }
    progress(app, "Fatto", 1.0);
    Ok(())
}

#[tauri::command]
async fn uninstall(app: AppHandle, remove_data: bool) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        progress(&app, "Chiudo Glyphling", 0.1);
        win::stop_app();
        std::thread::sleep(Duration::from_millis(400));
        progress(&app, "Tolgo i collegamenti", 0.35);
        for l in [start_menu_link(), desktop_link()].into_iter().flatten() {
            let _ = std::fs::remove_file(l);
        }
        progress(&app, "Tolgo l'app dal registro", 0.55);
        win::remove_uninstall_entry();
        if remove_data {
            progress(&app, "Cancello le impostazioni", 0.75);
            for var in ["APPDATA", "LOCALAPPDATA"] {
                if let Some(base) = std::env::var_os(var) {
                    let _ = std::fs::remove_dir_all(PathBuf::from(base).join(IDENTIFIER));
                }
            }
        }
        progress(&app, "Cancello i file", 0.9);
        let dir = install_dir();
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for e in entries.flatten() {
                let _ = std::fs::remove_file(e.path());
            }
        }
        progress(&app, "Fatto", 1.0);
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Closes the setup, optionally starting the app; after an uninstall the folder is removed.
#[tauri::command]
fn finish(app: AppHandle, launch: bool) {
    if uninstall_mode() {
        win::delete_later(&install_dir());
    } else if launch {
        win::launch(&install_dir().join(EXE));
    }
    app.exit(0);
}

fn main() {
    if !win::webview2_installed() {
        let ok = win::message(
            "Glyphling ha bisogno di Microsoft Edge WebView2, un componente di Windows che su questo PC manca.\n\nLo scarico e lo installo adesso?",
            true,
        );
        if !ok || !win::install_webview2() {
            win::message("Senza WebView2 Glyphling non può funzionare. Riprova più tardi.", false);
            return;
        }
    }
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![get_state, install, uninstall, finish])
        .setup(|app| {
            let w = WebviewWindowBuilder::new(app, "setup", WebviewUrl::App("index.html".into()))
                .title(if uninstall_mode() { "Disinstalla Glyphling" } else { "Installa Glyphling" })
                .inner_size(WIN_W, WIN_H)
                // an opaque window: a transparent one showed black edges on some PCs
                .decorations(false)
                .shadow(false)
                .resizable(false)
                .theme(Some(tauri::Theme::Dark))
                .background_color(tauri::window::Color(18, 18, 18, 255))
                .center()
                .build()?;
            // on some PCs the window first came up with a Windows title bar and the page
            // smaller than the window: drop the frame again and resize so the page fills it
            let _ = w.set_decorations(false);
            let _ = w.set_size(tauri::LogicalSize::new(WIN_W, WIN_H));
            let _ = w.center();
            win::round(&w);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("setup failed to start");
}
