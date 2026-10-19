//! Updates: looks for a newer release on GitHub, downloads its setup and runs it in
//! `--update` mode, which installs over the current copy and starts the app again.
//! Releases are public, so no sign-in is needed; any network error is silent unless the
//! user asked for the check.

use crate::AppState;
use serde::Serialize;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

const API: &str = "https://api.github.com/repos/OkanFeNwa/glyphling/releases?per_page=10";

#[derive(Serialize, Clone, Default, PartialEq, Debug)]
pub struct Update {
    pub version: String,
    pub url: String,
}

fn parse(v: &str) -> Option<(u32, u32, u32)> {
    let mut p = v.trim_start_matches('v').split('.').map(|n| n.parse::<u32>().ok());
    Some((p.next()??, p.next()??, p.next()??))
}

/// Newest published release that is newer than this build, with its setup file.
pub fn find() -> Result<Option<Update>, String> {
    let body = ureq::get(API)
        .set("User-Agent", "Glyphling")
        .set("Accept", "application/vnd.github+json")
        .timeout(Duration::from_secs(20))
        .call()
        .map_err(|e| format!("controllo non riuscito: {e}"))?
        .into_string()
        .map_err(|e| e.to_string())?;
    let list: serde_json::Value = serde_json::from_str(&body).map_err(|e| e.to_string())?;
    let me = parse(env!("CARGO_PKG_VERSION")).ok_or("versione non valida")?;
    let mut best: Option<((u32, u32, u32), Update)> = None;
    for r in list.as_array().into_iter().flatten() {
        if r["draft"].as_bool().unwrap_or(false) {
            continue;
        }
        let Some(v) = r["tag_name"].as_str().and_then(parse) else { continue };
        if v <= me || best.as_ref().is_some_and(|(b, _)| *b >= v) {
            continue;
        }
        let want = format!("Glyphling-Setup-{}.{}.{}.exe", v.0, v.1, v.2);
        let url = r["assets"]
            .as_array()
            .into_iter()
            .flatten()
            .find(|a| a["name"] == want.as_str())
            .and_then(|a| a["browser_download_url"].as_str());
        if let Some(url) = url.filter(|u| u.starts_with("https://github.com/OkanFeNwa/glyphling/")) {
            best = Some((v, Update { version: format!("{}.{}.{}", v.0, v.1, v.2), url: url.into() }));
        }
    }
    Ok(best.map(|(_, u)| u))
}

/// Downloads the setup and hands over to it; the app quits so the files can be replaced.
#[cfg(windows)]
pub fn install(app: &AppHandle, u: &Update) -> Result<(), String> {
    use std::io::Read;
    let mut bytes = Vec::new();
    ureq::get(&u.url)
        .set("User-Agent", "Glyphling")
        .timeout(Duration::from_secs(300))
        .call()
        .map_err(|e| format!("download non riuscito: {e}"))?
        .into_reader()
        .take(200_000_000)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() < 1_000_000 || &bytes[..2] != b"MZ" {
        return Err("file scaricato non valido".into());
    }
    let dir = std::env::temp_dir().join("glyphling-update");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let exe = dir.join(format!("Glyphling-Setup-{}.exe", u.version));
    std::fs::write(&exe, bytes).map_err(|e| e.to_string())?;
    std::process::Command::new(&exe).arg("--update").spawn().map_err(|e| e.to_string())?;
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(600));
        app.exit(0);
    });
    Ok(())
}

#[cfg(not(windows))]
pub fn install(_: &AppHandle, _: &Update) -> Result<(), String> {
    Err("disponibile solo su Windows".into())
}

/// Looks once; tells every window if something newer exists. With `notify`, Settings opens
/// on the Info page the first time a given version is found.
pub fn check(app: &AppHandle, notify: bool) -> Result<Option<Update>, String> {
    let found = find()?;
    let st = app.state::<AppState>();
    *st.update.lock().unwrap() = found.clone();
    let _ = app.emit("update", &found);
    if let (Some(u), true) = (&found, notify) {
        let mut first = false;
        crate::update(app, |c| {
            first = c.update_notified != u.version;
            c.update_notified = u.version.clone();
        });
        if first {
            crate::open_settings(app, Some("info".into()));
        }
    }
    Ok(found)
}

pub fn spawn(app: AppHandle) {
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(45));
        loop {
            let auto = app.state::<AppState>().cfg.lock().unwrap().auto_update;
            if auto {
                let _ = check(&app, true);
            }
            std::thread::sleep(Duration::from_secs(6 * 3600));
        }
    });
}
