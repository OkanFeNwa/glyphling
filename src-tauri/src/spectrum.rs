//! Visualizer: spettro reale dell'audio in uscita.
//!
//! Cattura in loopback WASAPI l'audio che va alle casse (la stessa tecnica di OBS), ne
//! calcola la FFT e manda 19 bande (una per colonna di LED) a 30 fotogrammi al secondo.
//! Gira solo quando la modalità Musica è visibile e qualcosa sta suonando.

use crate::AppState;
use std::sync::atomic::Ordering;
use std::time::Duration;
use tauri::{AppHandle, Manager};

pub const BANDS: usize = 19;
const FFT_N: usize = 1024;

fn wanted(app: &AppHandle) -> bool {
    let st = app.state::<AppState>();
    let c = st.cfg.lock().unwrap();
    c.mode == "music" && c.visible && st.playing.load(Ordering::Relaxed)
}

/// FFT radix-2 in place (n potenza di 2).
fn fft(re: &mut [f32], im: &mut [f32]) {
    let n = re.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let ang = -2.0 * std::f32::consts::PI / len as f32;
        let (wr, wi) = (ang.cos(), ang.sin());
        for start in (0..n).step_by(len) {
            let (mut cr, mut ci) = (1.0f32, 0.0f32);
            for k in 0..len / 2 {
                let (a, b) = (start + k, start + k + len / 2);
                let tr = re[b] * cr - im[b] * ci;
                let ti = re[b] * ci + im[b] * cr;
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
                let ncr = cr * wr - ci * wi;
                ci = cr * wi + ci * wr;
                cr = ncr;
            }
        }
        len <<= 1;
    }
}

/// Dalle ultime FFT_N campioni alle 19 bande, in scala logaritmica da 40 Hz a 14 kHz.
pub struct Analyzer {
    rate: f32,
    peak: f32,
    smooth: [f32; BANDS],
}

impl Analyzer {
    pub fn new(rate: f32) -> Self {
        Self { rate, peak: 0.02, smooth: [0.0; BANDS] }
    }

    pub fn bands(&mut self, samples: &[f32]) -> [f32; BANDS] {
        let mut re = [0f32; FFT_N];
        let mut im = [0f32; FFT_N];
        let n = samples.len().min(FFT_N);
        for i in 0..n {
            let w = 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / (FFT_N - 1) as f32).cos(); // Hann
            re[i] = samples[samples.len() - n + i] * w;
        }
        fft(&mut re, &mut im);
        let bin_hz = self.rate / FFT_N as f32;
        let (lo, hi) = (40f32, 14_000f32.min(self.rate / 2.0));
        let mut raw = [0f32; BANDS];
        for (b, slot) in raw.iter_mut().enumerate() {
            let f0 = lo * (hi / lo).powf(b as f32 / BANDS as f32);
            let f1 = lo * (hi / lo).powf((b + 1) as f32 / BANDS as f32);
            let (i0, i1) = (((f0 / bin_hz) as usize).max(1), ((f1 / bin_hz) as usize).max(2));
            let mut m = 0f32;
            for i in i0..i1.min(FFT_N / 2) {
                m = m.max((re[i] * re[i] + im[i] * im[i]).sqrt());
            }
            // le alte frequenze hanno meno energia: le alzo un po'
            *slot = m * (1.0 + b as f32 * 0.12);
        }
        // guadagno automatico: segue il picco e lo lascia scendere piano
        let top = raw.iter().cloned().fold(0f32, f32::max);
        self.peak = if top > self.peak { top } else { (self.peak * 0.995).max(0.02) };
        let mut out = [0f32; BANDS];
        for b in 0..BANDS {
            let v = (raw[b] / self.peak).clamp(0.0, 1.0).powf(0.7);
            self.smooth[b] = if v > self.smooth[b] { v } else { self.smooth[b] * 0.8 + v * 0.2 };
            out[b] = self.smooth[b];
        }
        // ordine a specchio come nel prototipo: bassi al centro
        let mut mirrored = [0f32; BANDS];
        let c = BANDS / 2;
        for (k, slot) in mirrored.iter_mut().enumerate() {
            let d = (k as isize - c as isize).unsigned_abs();
            *slot = out[(d * 2).min(BANDS - 1)];
        }
        mirrored
    }
}

#[cfg(windows)]
fn capture(app: &AppHandle) -> windows::core::Result<()> {
    use tauri::Emitter;
    use windows::Win32::Media::Audio::*;
    use windows::Win32::System::Com::{CoCreateInstance, CoTaskMemFree, CLSCTX_ALL};

    crate::media::com_init();
    unsafe {
        let en: IMMDeviceEnumerator = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;
        let dev = en.GetDefaultAudioEndpoint(eRender, eConsole)?;
        let client: IAudioClient = dev.Activate(CLSCTX_ALL, None)?;
        let fmt = client.GetMixFormat()?;
        let (ch, bits, rate) = ((*fmt).nChannels as usize, (*fmt).wBitsPerSample, (*fmt).nSamplesPerSec);
        let r = client.Initialize(AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_LOOPBACK, 2_000_000, 0, fmt, None);
        CoTaskMemFree(Some(fmt as _));
        r?;
        let cap: IAudioCaptureClient = client.GetService()?;
        client.Start()?;
        let mut an = Analyzer::new(rate as f32);
        let mut ring: Vec<f32> = Vec::with_capacity(FFT_N * 2);
        let mut since = std::time::Instant::now();
        while wanted(app) {
            std::thread::sleep(Duration::from_millis(10));
            let mut packet = cap.GetNextPacketSize()?;
            while packet > 0 {
                let mut data = std::ptr::null_mut();
                let (mut frames, mut flags) = (0u32, 0u32);
                cap.GetBuffer(&mut data, &mut frames, &mut flags, None, None)?;
                let silent = flags & (AUDCLNT_BUFFERFLAGS_SILENT.0 as u32) != 0;
                for f in 0..frames as usize {
                    let mut s = 0f32;
                    if !silent && bits == 32 && ch > 0 {
                        let p = data as *const f32;
                        for c in 0..ch {
                            s += *p.add(f * ch + c);
                        }
                        s /= ch as f32;
                    } else if !silent && bits == 16 && ch > 0 {
                        let p = data as *const i16;
                        for c in 0..ch {
                            s += *p.add(f * ch + c) as f32 / 32768.0;
                        }
                        s /= ch as f32;
                    }
                    ring.push(s);
                }
                cap.ReleaseBuffer(frames)?;
                packet = cap.GetNextPacketSize()?;
            }
            if ring.len() > FFT_N {
                let cut = ring.len() - FFT_N;
                ring.drain(..cut);
            }
            if since.elapsed() >= Duration::from_millis(33) && ring.len() == FFT_N {
                since = std::time::Instant::now();
                let b = an.bands(&ring);
                let _ = app.emit_to("widget", "spectrum", b.to_vec());
            }
        }
        let _ = client.Stop();
    }
    Ok(())
}

#[cfg(not(windows))]
fn capture(_: &AppHandle) -> Result<(), ()> {
    std::thread::sleep(Duration::from_millis(500));
    Ok(())
}

pub fn spawn(app: AppHandle) {
    std::thread::spawn(move || loop {
        if wanted(&app) {
            // se il dispositivo cambia o la cattura fallisce, riprova dopo un attimo
            if capture(&app).is_err() {
                std::thread::sleep(Duration::from_secs(2));
            }
        } else {
            std::thread::sleep(Duration::from_millis(300));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tone_lights_the_matching_band() {
        let rate = 48_000.0;
        let tone: Vec<f32> = (0..FFT_N).map(|i| (2.0 * std::f32::consts::PI * 1000.0 * i as f32 / rate).sin()).collect();
        let mut an = Analyzer::new(rate);
        let b = an.bands(&tone);
        let (imax, _) = b.iter().enumerate().fold((0, 0f32), |m, (i, &v)| if v > m.1 { (i, v) } else { m });
        // 1 kHz cade circa a metà della scala logaritmica, quindi lontano dal centro (bassi)
        assert!(imax != BANDS / 2, "{b:?}");
        assert!(b.iter().cloned().fold(0f32, f32::max) > 0.9);
    }
}
