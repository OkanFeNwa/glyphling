//! Musica: cosa sta suonando in qualunque app (Spotify, browser, VLC...), comandi,
//! copertina e volume, tramite i controlli multimediali di Windows (GSMTC) e Core Audio.

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
    /// Copertina come data URL; cambia solo con il brano.
    pub cover: Option<String>,
    /// Vero quando la copertina è cambiata (anche se è sparita); altrimenti `cover` non viene inviata.
    pub cover_changed: bool,
    pub volume: Option<f32>,
}

/// Nome leggibile dall'identificativo dell'app ("Spotify.exe", "SpotifyAB.SpotifyMusic_...!Spotify").
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
    use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
    use windows::Win32::Media::Audio::{eConsole, eRender, IMMDeviceEnumerator, MMDeviceEnumerator};
    use windows::Win32::System::Com::{CoCreateInstance, CoInitializeEx, CLSCTX_ALL, COINIT_MULTITHREADED};

    pub fn com_init() {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        }
    }

    pub fn manager() -> Option<Mgr> {
        Mgr::RequestAsync().ok()?.get().ok()
    }

    fn session() -> Option<Session> {
        manager()?.GetCurrentSession().ok()
    }

    /// Ora in tick da 100 ns dal 1601, come i tempi di WinRT.
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

    /// Stato attuale. La copertina si legge solo se il brano è cambiato rispetto a `prev`.
    pub fn read(mgr: &Mgr, prev: &Media) -> Media {
        let Ok(s) = mgr.GetCurrentSession() else { return Media { volume: prev.volume, ..Default::default() } };
        let mut m = Media { active: true, volume: prev.volume, ..Default::default() };
        m.app = s.SourceAppUserModelId().map(|h| super::app_name(&h.to_string())).unwrap_or_default();
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
            // la posizione è quella dell'ultimo aggiornamento: la porto ad adesso
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

    fn endpoint() -> windows::core::Result<IAudioEndpointVolume> {
        com_init();
        unsafe {
            let en: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
            let dev = en.GetDefaultAudioEndpoint(eRender, eConsole)?;
            dev.Activate::<IAudioEndpointVolume>(CLSCTX_ALL, None)
        }
    }

    pub fn volume() -> Option<f32> {
        unsafe { endpoint().ok()?.GetMasterVolumeLevelScalar().ok() }
    }

    pub fn set_volume(v: f32) {
        if let Ok(ep) = endpoint() {
            unsafe {
                let _ = ep.SetMasterVolumeLevelScalar(v.clamp(0.0, 1.0), std::ptr::null());
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
    pub fn volume() -> Option<f32> {
        None
    }
    pub fn set_volume(_: f32) {}
}

pub use imp::{com_init, control, set_volume};

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
            // in modalità Musica ogni secondo, altrimenti ogni 5 secondi solo per sapere se suona qualcosa
            std::thread::sleep(Duration::from_millis(if music { 1000 } else { 5000 }));
            tick = tick.wrapping_add(1);
            if mgr.is_none() || tick % 30 == 0 {
                mgr = imp::manager();
            }
            let Some(m) = &mgr else { continue };
            let mut now = imp::read(m, &last);
            if music && (tick % 3 == 0 || last.volume.is_none()) {
                now.volume = imp::volume();
            }
            let st = app.state::<AppState>();
            st.playing.store(now.playing, std::sync::atomic::Ordering::Relaxed);
            // la posizione cambia sempre: invio solo se cambia altro o se la modalità è visibile
            let changed = Media { position: 0.0, ..now.clone() } != Media { position: 0.0, ..last.clone() };
            if changed || music {
                let mut out = now.clone();
                out.cover_changed = now.cover != last.cover;
                if !out.cover_changed {
                    out.cover = None; // la copertina viaggia solo quando cambia
                }
                let _ = app.emit_to("widget", "media", &out);
            }
            last = now;
        }
    });
}
