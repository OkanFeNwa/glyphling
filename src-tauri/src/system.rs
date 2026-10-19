//! PC stats: CPU, RAM, GPU (NVIDIA through NVML), temperatures and disks.
//!
//! Read only while the System mode is visible. CPU temperature has no official
//! API on Windows: we take it from LibreHardwareMonitor, if it is running with its
//! web server enabled (port 8085), without installing drivers of our own.

use crate::AppState;
use serde::Serialize;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};

#[derive(Serialize, Clone, Default)]
pub struct Disk {
    pub lt: String,
    pub name: String,
    pub used: f64,
    pub size: f64,
}

#[derive(Serialize, Clone, Default)]
pub struct Stats {
    pub cpu: f32,
    pub ram: f32,
    pub ram_used: f64,
    pub ram_total: f64,
    pub gpu: Option<u32>,
    pub tc: Option<f32>,
    pub tg: Option<u32>,
    pub up_min: u64,
    pub disks: Vec<Disk>,
    /// Where the CPU temperature comes from (for the settings).
    pub tc_source: Option<String>,
}

const GB: f64 = 1_000_000_000.0;

pub fn disk_list() -> Vec<Disk> {
    let list = sysinfo::Disks::new_with_refreshed_list();
    let mut out: Vec<Disk> = list
        .iter()
        .filter(|d| d.total_space() > 0)
        .map(|d| {
            let mp = d.mount_point().to_string_lossy().to_string();
            let lt = mp.chars().next().filter(|c| c.is_ascii_alphabetic()).map(|c| c.to_ascii_uppercase().to_string()).unwrap_or(mp.clone());
            let label = d.name().to_string_lossy().to_string();
            let name = if label.is_empty() || label == mp { if d.is_removable() { "Rimovibile".into() } else { "Disco locale".into() } } else { label };
            let size = d.total_space() as f64 / GB;
            Disk { lt, name, used: ((size - d.available_space() as f64 / GB) * 10.0).round() / 10.0, size: (size * 10.0).round() / 10.0 }
        })
        .collect();
    out.sort_by(|a, b| a.lt.cmp(&b.lt));
    out.dedup_by(|a, b| a.lt == b.lt);
    out
}

/// Looks up the first CPU temperature sensor in LibreHardwareMonitor's JSON.
fn lhm_cpu_temp() -> Option<f32> {
    let body = ureq::get("http://127.0.0.1:8085/data.json").timeout(Duration::from_millis(600)).call().ok()?.into_string().ok()?;
    let v: serde_json::Value = serde_json::from_str(&body).ok()?;
    const NAMES: [&str; 6] = ["Core (Tctl/Tdie)", "CPU Package", "Core (Tctl)", "Tctl/Tdie", "Package", "Core Average"];
    fn walk<'a>(v: &'a serde_json::Value, out: &mut Vec<(&'a str, &'a str)>) {
        if let (Some(t), Some(val)) = (v.get("Text").and_then(|x| x.as_str()), v.get("Value").and_then(|x| x.as_str())) {
            if val.contains("°C") {
                out.push((t, val));
            }
        }
        if let Some(ch) = v.get("Children").and_then(|c| c.as_array()) {
            for c in ch {
                walk(c, out);
            }
        }
    }
    let mut temps = vec![];
    walk(&v, &mut temps);
    let num = |s: &str| s.split_whitespace().next()?.replace(',', ".").parse::<f32>().ok();
    NAMES.iter().find_map(|n| temps.iter().find(|(t, _)| t == n).and_then(|(_, v)| num(v)))
}

#[cfg(windows)]
struct Gpu(Option<nvml_wrapper::Nvml>);

#[cfg(windows)]
impl Gpu {
    fn init() -> Self {
        Gpu(nvml_wrapper::Nvml::init().ok())
    }
    fn read(&self) -> (Option<u32>, Option<u32>) {
        use nvml_wrapper::enum_wrappers::device::TemperatureSensor;
        let Some(n) = &self.0 else { return (None, None) };
        let Ok(d) = n.device_by_index(0) else { return (None, None) };
        (d.utilization_rates().ok().map(|u| u.gpu), d.temperature(TemperatureSensor::Gpu).ok())
    }
}

#[cfg(not(windows))]
struct Gpu;

#[cfg(not(windows))]
impl Gpu {
    fn init() -> Self {
        Gpu
    }
    fn read(&self) -> (Option<u32>, Option<u32>) {
        (None, None)
    }
}

pub fn spawn(app: AppHandle) {
    std::thread::spawn(move || {
        let mut sys = sysinfo::System::new();
        let mut gpu: Option<Gpu> = None; // NVML is loaded the first time it is needed
        let mut cached: Vec<Disk> = vec![];
        let mut disks_at: Option<Instant> = None;
        loop {
            let wanted = {
                let st = app.state::<AppState>();
                let c = st.cfg.lock().unwrap();
                c.mode == "disks" && c.visible
            };
            if !wanted {
                std::thread::sleep(Duration::from_millis(500));
                disks_at = None; // on return the disks are read again right away
                continue;
            }
            let gpu = gpu.get_or_insert_with(Gpu::init);
            sys.refresh_cpu_usage();
            sys.refresh_memory();
            if disks_at.map_or(true, |t| t.elapsed() > Duration::from_secs(30)) {
                cached = disk_list();
                disks_at = Some(Instant::now());
            }
            let (g, tg) = gpu.read();
            let tc = lhm_cpu_temp();
            let total = sys.total_memory() as f64;
            let used = sys.used_memory() as f64;
            let s = Stats {
                cpu: sys.global_cpu_usage(),
                ram: if total > 0.0 { (used / total * 100.0) as f32 } else { 0.0 },
                ram_used: used / (1024.0 * 1024.0 * 1024.0),
                ram_total: total / (1024.0 * 1024.0 * 1024.0),
                gpu: g,
                tc,
                tg,
                up_min: sysinfo::System::uptime() / 60,
                disks: cached.clone(),
                tc_source: tc.map(|_| "LibreHardwareMonitor".into()),
            };
            let _ = app.emit("pc", &s);
            std::thread::sleep(Duration::from_millis(1500));
        }
    });
}
