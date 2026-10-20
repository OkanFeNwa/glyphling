//! Music: what is playing in any app (Spotify, browser, VLC...), controls,
//! cover art and volume, through Windows media controls (GSMTC) and Core Audio.

use crate::AppState;
use serde::Serialize;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};

#[derive(Serialize, Clone, PartialEq, Default, Debug)]
pub struct Media {
    pub active: bool,
    pub app: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub playing: bool,
    pub position: f64,
    pub duration: f64,
    /// Cover art as a data URL; changes only with the track.
    pub cover: Option<String>,
    /// True when the cover changed (even if it disappeared); otherwise `cover` is not sent.
    pub cover_changed: bool,
    pub volume: Option<f32>,
    /// Id of the app being followed (matches the keys of the source filter).
    pub source: String,
}

/// Which apps may be the music source: "all", "allow" (only the listed ones) or "block" (all but the listed).
#[derive(Clone, Default)]
pub struct Filter {
    pub mode: String,
    pub apps: Vec<String>,
}

static FILTER: std::sync::Mutex<Option<Filter>> = std::sync::Mutex::new(None);
/// Apps (compact exe names) the volume slider lowers; `None` = the app being followed.
static VOLKEYS: std::sync::Mutex<Option<Vec<String>>> = std::sync::Mutex::new(None);
static LAST: std::sync::Mutex<String> = std::sync::Mutex::new(String::new());

pub fn set_vol_keys(keys: Option<Vec<String>>) {
    *VOLKEYS.lock().unwrap() = keys;
}

pub fn set_filter(mode: &str, apps: &[String]) {
    *FILTER.lock().unwrap() = Some(Filter { mode: mode.into(), apps: apps.to_vec() });
}

/// Lowercase letters and digits only, so "Microsoft.ZuneMusic" and "zunemusic" compare.
pub fn compact(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(|c| c.to_lowercase()).collect()
}

/// Key that identifies an app in the filter, from its media-session id.
pub fn key_of(id: &str) -> String {
    let s = id.rsplit('!').next().unwrap_or(id);
    compact(s.trim_end_matches(".exe").trim_end_matches(".EXE"))
}

fn eligible(id: &str) -> bool {
    let f = FILTER.lock().unwrap().clone().unwrap_or_default();
    let id = compact(id);
    let listed = f.apps.iter().any(|k| !k.is_empty() && id.contains(k.as_str()));
    match f.mode.as_str() {
        "allow" => listed,
        "block" => !listed,
        _ => true,
    }
}

/// Readable name from the app identifier ("Spotify.exe", "SpotifyAB.SpotifyMusic_...!Spotify").
fn app_name(id: &str) -> String {
    let s = id.rsplit('!').next().unwrap_or(id);
    let s = s.trim_end_matches(".exe").trim_end_matches(".EXE");
    let s = s.rsplit(['\\', '/']).next().unwrap_or(s);
    match s.to_ascii_lowercase().as_str() {
        "msedge" => "Edge".into(),
        "chrome" => "Chrome".into(),
        "firefox" => "Firefox".into(),
        "vlc" => "VLC".into(),
        "microsoft.zunemusic_8wekyb3d8bbwe" | "microsoft.media.player" => "Lettore multimediale".into(),
        _ => {
            let mut c = s.chars();
            c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
        }
    }
}

#[cfg(windows)]
mod imp {
    use super::Media;
    use base64::Engine;
    use windows::core::Interface;
    use windows::Media::Control::{
        GlobalSystemMediaTransportControlsSession as Session, GlobalSystemMediaTransportControlsSessionManager as Mgr,
        GlobalSystemMediaTransportControlsSessionPlaybackStatus as Status,
    };
    use windows::Storage::Streams::{DataReader, IRandomAccessStreamReference};
    use windows::Win32::Media::Audio::{
        eConsole, eRender, AudioSessionStateActive, IAudioSessionControl2, DEVICE_STATE_ACTIVE, IAudioSessionManager2, IMMDeviceEnumerator, ISimpleAudioVolume,
        MMDeviceEnumerator,
    };
    use windows::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION};
    use windows::Win32::Foundation::CloseHandle;
    use windows::core::PWSTR;
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED};

    pub fn com_init() {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
    }

    pub fn manager() -> Option<Mgr> {
        Mgr::RequestAsync().ok()?.get().ok()
    }

    fn id_of(s: &Session) -> String {
        s.SourceAppUserModelId().map(|h| h.to_string()).unwrap_or_default()
    }

    fn is_playing(s: &Session) -> bool {
        s.GetPlaybackInfo().and_then(|i| i.PlaybackStatus()).map(|st| st == Status::Playing).unwrap_or(false)
    }

    /// The one app being followed: the previous one while it still plays, else any playing
    /// allowed app, else the system's current session if allowed, else the first allowed one.
    pub fn pick(mgr: &Mgr) -> Option<Session> {
        let all: Vec<Session> = mgr.GetSessions().ok()?.into_iter().filter(|s| super::eligible(&id_of(s))).collect();
        let last = super::LAST.lock().unwrap().clone();
        let chosen = all
            .iter()
            .find(|s| id_of(s) == last && is_playing(s))
            .or_else(|| all.iter().find(|s| is_playing(s)))
            .or_else(|| mgr.GetCurrentSession().ok().and_then(|c| all.iter().find(|s| id_of(s) == id_of(&c))))
            .or_else(|| all.first())
            .cloned();
        if let Some(c) = &chosen {
            *super::LAST.lock().unwrap() = id_of(c);
        }
        chosen
    }

    fn session() -> Option<Session> {
        pick(&manager()?)
    }

    /// Apps with a media session right now: (key, readable name).
    pub fn sessions() -> Vec<(String, String, bool)> {
        let Some(m) = manager() else { return vec![] };
        let Ok(list) = m.GetSessions() else { return vec![] };
        list.into_iter()
            .map(|s| {
                let id = id_of(&s);
                (super::key_of(&id), super::app_name(&id), is_playing(&s))
            })
            .collect()
    }

    /// Current time in 100 ns ticks since 1601, like WinRT times.
    fn now_ticks() -> i64 {
        let d = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
        (d.as_nanos() / 100) as i64 + 116_444_736_000_000_000
    }

    fn read_cover(r: &IRandomAccessStreamReference) -> Option<String> {
        let stream = r.OpenReadAsync().ok()?.get().ok()?;
        let size = stream.Size().ok()? as u32;
        if size == 0 || size > 4_000_000 {
            return None;
        }
        let ctype = stream.ContentType().map(|s| s.to_string()).unwrap_or_default();
        let reader = DataReader::CreateDataReader(&stream.cast::<windows::Storage::Streams::IInputStream>().ok()?).ok()?;
        reader.LoadAsync(size).ok()?.get().ok()?;
        let mut buf = vec![0u8; size as usize];
        reader.ReadBytes(&mut buf).ok()?;
        let ctype = if ctype.starts_with("image/") { ctype } else { "image/png".into() };
        Some(format!("data:{};base64,{}", ctype, base64::engine::general_purpose::STANDARD.encode(buf)))
    }

    /// Current state. The cover is read only if the track changed compared to `prev`.
    pub fn read(mgr: &Mgr, prev: &Media) -> Media {
        let Some(s) = pick(mgr) else { return Media { volume: prev.volume, ..Default::default() } };
        let mut m = Media { active: true, volume: prev.volume, ..Default::default() };
        m.source = id_of(&s);
        m.app = super::app_name(&m.source);
        let props = s.TryGetMediaPropertiesAsync().ok().and_then(|op| op.get().ok());
        if let Some(p) = &props {
            m.title = p.Title().map(|h| h.to_string()).unwrap_or_default();
            m.artist = p.Artist().map(|h| h.to_string()).unwrap_or_default();
            m.album = p.AlbumTitle().map(|h| h.to_string()).unwrap_or_default();
        }
        if let Ok(info) = s.GetPlaybackInfo() {
            m.playing = info.PlaybackStatus().map(|st| st == Status::Playing).unwrap_or(false);
        }
        if let Ok(tl) = s.GetTimelineProperties() {
            let pos = tl.Position().map(|t| t.Duration).unwrap_or(0);
            let end = tl.EndTime().map(|t| t.Duration).unwrap_or(0);
            let start = tl.StartTime().map(|t| t.Duration).unwrap_or(0);
            let mut p = pos - start;
            // the position is from the last update: bring it forward to now
            if m.playing {
                if let Ok(last) = tl.LastUpdatedTime() {
                    let gap = now_ticks() - last.UniversalTime;
                    if gap > 0 && gap < 36_000_000_000 {
                        p += gap;
                    }
                }
            }
            m.duration = ((end - start).max(0)) as f64 / 1e7;
            m.position = (p.max(0) as f64 / 1e7).min(if m.duration > 0.0 { m.duration } else { f64::MAX });
        }
        let same = prev.title == m.title && prev.artist == m.artist && prev.app == m.app;
        m.cover = if same && prev.cover.is_some() {
            prev.cover.clone()
        } else {
            props.and_then(|p| p.Thumbnail().ok()).and_then(|t| read_cover(&t))
        };
        m
    }

    pub fn control(action: &str, value: f64) -> bool {
        let Some(s) = session() else { return false };
        let r = match action {
            "toggle" => s.TryTogglePlayPauseAsync().and_then(|o| o.get()),
            "next" => s.TrySkipNextAsync().and_then(|o| o.get()),
            "prev" => s.TrySkipPreviousAsync().and_then(|o| o.get()),
            "seek" => {
                let start = s.GetTimelineProperties().and_then(|t| t.StartTime()).map(|t| t.Duration).unwrap_or(0);
                s.TryChangePlaybackPositionAsync(start + (value * 1e7) as i64).and_then(|o| o.get())
            }
            _ => return false,
        };
        r.unwrap_or(false)
    }

    /// File name of the process ("Spotify.exe").
    fn exe_name(pid: u32) -> Option<String> {
        unsafe {
            let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
            let mut buf = [0u16; 520];
            let mut n = buf.len() as u32;
            let r = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut n);
            let _ = CloseHandle(h);
            r.ok()?;
            let path = String::from_utf16_lossy(&buf[..n as usize]);
            path.rsplit(['\\', '/']).next().map(String::from)
        }
    }

    fn exe_stem(pid: u32) -> Option<String> {
        exe_name(pid).map(|n| super::compact(n.trim_end_matches(".exe").trim_end_matches(".EXE")))
    }

    /// Audio sessions of every active output device: (device number, is default, pid, session).
    /// An app can play on any device, and the mixer shows them all.
    fn all_sessions() -> Vec<(usize, bool, u32, windows::Win32::Media::Audio::IAudioSessionControl)> {
        let mut out = vec![];
        com_init();
        unsafe {
            let Ok(en) = CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL) else { return out };
            let default_id = en.GetDefaultAudioEndpoint(eRender, eConsole).ok().and_then(|d| d.GetId().ok()).and_then(|i| i.to_string().ok());
            let Ok(devs) = en.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE) else { return out };
            for d in 0..devs.GetCount().unwrap_or(0) {
                let Ok(dev) = devs.Item(d) else { continue };
                let is_default = dev.GetId().ok().and_then(|i| i.to_string().ok()) == default_id && default_id.is_some();
                let Ok(mgr) = dev.Activate::<IAudioSessionManager2>(CLSCTX_ALL, None) else { continue };
                let Ok(list) = mgr.GetSessionEnumerator() else { continue };
                for i in 0..list.GetCount().unwrap_or(0) {
                    let Ok(c) = list.GetSession(i) else { continue };
                    let Ok(c2) = c.cast::<IAudioSessionControl2>() else { continue };
                    let pid = c2.GetProcessId().unwrap_or(0);
                    // pid 0 is the system sounds session; never touch it or ourselves
                    if pid != 0 && pid != std::process::id() {
                        out.push((d as usize, is_default, pid, c));
                    }
                }
            }
        }
        out
    }

    fn sessions_of_default_device() -> Vec<(u32, windows::Win32::Media::Audio::IAudioSessionControl)> {
        all_sessions().into_iter().map(|(_, _, pid, c)| (pid, c)).collect()
    }

    fn is_target(stem: &str, keys: &Option<Vec<String>>, id: &str) -> bool {
        match keys {
            Some(keys) => stem.len() >= 3 && keys.iter().any(|k| *k == stem || (k.len() >= 3 && (stem.contains(k.as_str()) || k.contains(stem)))),
            None => stem.len() >= 3 && id.contains(stem),
        }
    }

    /// Apps with an audio session in the Windows mixer: (key, file name).
    pub fn audio_apps() -> Vec<(String, String, bool)> {
        let mut out: Vec<(String, String, bool)> = vec![];
        for (pid, c) in sessions_of_default_device() {
            let active = unsafe { c.GetState().map(|st| st == AudioSessionStateActive).unwrap_or(false) };
            if let Some(n) = exe_name(pid) {
                let k = super::compact(n.trim_end_matches(".exe").trim_end_matches(".EXE"));
                match out.iter_mut().find(|(x, _, _)| *x == k) {
                    Some(o) => o.2 |= active,
                    None => out.push((k, n, active)),
                }
            }
        }
        out
    }

    /// Audio sessions (volume controls) the slider acts on: the process chosen in Settings,
    /// or else the processes whose name matches the followed media app (browsers have several).
    /// Nothing else is ever touched, never the system volume.
    fn app_volumes(source: &str) -> Vec<ISimpleAudioVolume> {
        let keys = super::VOLKEYS.lock().unwrap().clone();
        let id = super::compact(source);
        if keys.is_none() && id.is_empty() {
            return vec![];
        }
        sessions_of_default_device()
            .into_iter()
            .filter(|(pid, _)| exe_stem(*pid).is_some_and(|stem| is_target(&stem, &keys, &id)))
            .filter_map(|(_, c)| c.cast::<ISimpleAudioVolume>().ok())
            .collect()
    }

    /// Volume of the followed app only; None when it has no audio session yet.
    pub fn volume(source: &str) -> Option<f32> {
        app_volumes(source).iter().filter_map(|v| unsafe { v.GetMasterVolume().ok() }).reduce(f32::max)
    }

    pub fn set_volume(source: &str, v: f32) {
        for c in app_volumes(source) {
            unsafe {
                let _ = c.SetMasterVolume(v.clamp(0.0, 1.0), std::ptr::null());
            }
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use super::Media;
    pub struct Mgr;
    pub fn com_init() {}
    pub fn manager() -> Option<Mgr> {
        None
    }
    pub fn read(_: &Mgr, prev: &Media) -> Media {
        prev.clone()
    }
    pub fn control(_: &str, _: f64) -> bool {
        false
    }
    pub fn volume(_: &str) -> Option<f32> {
        None
    }
    pub fn set_volume(_: &str, _: f32) {}
    pub fn sessions() -> Vec<(String, String, bool)> {
        vec![]
    }
    pub fn audio_apps() -> Vec<(String, String, bool)> {
        vec![]
    }
}

pub use imp::{audio_apps, com_init, control, sessions};

/// Readable name of the app being followed (empty when none).
pub fn now_app() -> String {
    let id = LAST.lock().unwrap().clone();
    if id.is_empty() { String::new() } else { app_name(&id) }
}

/// Sets the volume of the app being followed (never the system volume).
pub fn set_volume(v: f32) {
    let src = LAST.lock().unwrap().clone();
    imp::set_volume(&src, v);
}

pub fn spawn(app: AppHandle) {
    std::thread::spawn(move || {
        imp::com_init();
        let mut mgr = None;
        let mut last = Media::default();
        let mut tick = 0u32;
        loop {
            let music = {
                let st = app.state::<AppState>();
                let c = st.cfg.lock().unwrap();
                c.mode == "music" && c.visible
            };
            // every second in Music mode, otherwise every 5 seconds just to know if something is playing
            std::thread::sleep(Duration::from_millis(if music { 1000 } else { 5000 }));
            tick = tick.wrapping_add(1);
            if mgr.is_none() || tick % 30 == 0 {
                mgr = imp::manager();
            }
            let Some(m) = &mgr else { continue };
            let mut now = imp::read(m, &last);
            if music && (tick % 3 == 0 || last.volume.is_none()) {
                now.volume = imp::volume(&now.source);
            }
            let st = app.state::<AppState>();
            st.playing.store(now.playing, std::sync::atomic::Ordering::Relaxed);
            // the position always changes: send only if something else changed or the mode is visible
            let changed = Media { position: 0.0, ..now.clone() } != Media { position: 0.0, ..last.clone() };
            // the position only moves while playing: with nothing to show, send nothing new
            if changed || (music && now.playing) {
                let mut out = now.clone();
                out.cover_changed = now.cover != last.cover;
                if !out.cover_changed {
                    out.cover = None; // the cover is sent only when it changes
                }
                let _ = app.emit_to("widget", "media", &out);
            }
            last = now;
        }
    });
}
