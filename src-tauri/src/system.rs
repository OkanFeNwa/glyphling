//! PC stats: CPU, RAM, GPU (NVIDIA through NVML), temperatures and disks.
//!
//! Read only while the System mode is visible.
//!
//! CPU temperature: the real die/package sensor can only be read from kernel mode
//! (CPU model specific registers), so it needs a driver and administrator rights.
//! Glyphling does not install drivers. Sources, in order:
//! 1. HWiNFO, through its shared memory (Settings > "Shared Memory Support").
//! 2. LibreHardwareMonitor, through its web server (port 8085).
//! 3. Windows' ACPI thermal zone (no extra program, no admin rights). It is the
//!    firmware's own reading: close to the CPU on most laptops, often a motherboard
//!    sensor or a fixed value on desktops, so it is shown as approximate.

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

/// Looks up the CPU temperature in LibreHardwareMonitor's JSON.
fn lhm_cpu_temp() -> Option<f32> {
    let resp = ureq::get("http://127.0.0.1:8085/data.json").timeout(Duration::from_millis(1500)).call().ok()?;
    // read bytes: the page may not be UTF-8, and the degree sign is not needed anyway
    let mut bytes = Vec::new();
    std::io::Read::read_to_end(&mut std::io::Read::take(resp.into_reader(), 4_000_000), &mut bytes).ok()?;
    let v: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    const NAMES: [&str; 8] = ["Core (Tctl/Tdie)", "CPU Package", "Core (Tctl)", "Tctl/Tdie", "Package", "Core Average", "Core Max", "CPU"];
    fn walk<'a>(v: &'a serde_json::Value, out: &mut Vec<(&'a str, &'a str)>) {
        if let (Some(t), Some(val)) = (v.get("Text").and_then(|x| x.as_str()), v.get("Value").and_then(|x| x.as_str())) {
            // temperatures end with the unit "C" ("45,0 °C"); fan speeds, loads and clocks do not
            if val.trim_end().ends_with('C') && val.contains(|c: char| c.is_ascii_digit()) {
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
    NAMES
        .iter()
        .find_map(|n| temps.iter().find(|(t, _)| t == n).and_then(|(_, v)| num(v)))
        // any other core sensor, then the first one in the tree (the CPU is listed first)
        .or_else(|| temps.iter().find(|(t, _)| t.starts_with("Core") || t.contains("CPU")).and_then(|(_, v)| num(v)))
}

/// Reads the CPU temperature from the first source that has one.
struct CpuTemp {
    #[cfg(windows)]
    zones: Option<win::Zones>,
}

impl CpuTemp {
    fn new() -> Self {
        CpuTemp {
            #[cfg(windows)]
            zones: None,
        }
    }

    fn read(&mut self) -> Option<(f32, &'static str)> {
        #[cfg(windows)]
        if let Some(t) = win::hwinfo_cpu_temp() {
            return Some((t, "hwinfo"));
        }
        if let Some(t) = lhm_cpu_temp() {
            return Some((t, "lhm"));
        }
        #[cfg(windows)]
        if let Some(t) = self.zones.get_or_insert_with(win::Zones::open).read() {
            return Some((t, "acpi"));
        }
        None
    }
}

#[cfg(windows)]
mod win {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Memory::{MapViewOfFile, OpenFileMappingW, UnmapViewOfFile, FILE_MAP_READ};
    use windows_sys::Win32::System::Performance::*;

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    /// CPU temperature from HWiNFO's shared memory (HWiNFO_SENSORS_SHARED_MEM2 layout,
    /// packed): a header, then sensor and reading arrays at the offsets it gives.
    pub fn hwinfo_cpu_temp() -> Option<f32> {
        const NAMES: [&str; 6] = ["CPU (Tctl/Tdie)", "CPU Package", "CPU (Tdie)", "CPU (Tctl)", "CPU Die (average)", "Core Max"];
        unsafe {
            let h = OpenFileMappingW(FILE_MAP_READ, 0, wide("Global\\HWiNFO_SENS_SM2").as_ptr());
            if h.is_null() {
                return None;
            }
            let view = MapViewOfFile(h, FILE_MAP_READ, 0, 0, 0);
            let out = if view.Value.is_null() {
                None
            } else {
                let base = view.Value as *const u8;
                let u32_at = |o: usize| std::ptr::read_unaligned(base.add(o) as *const u32);
                // "HWiS" when active, "DEAD" when HWiNFO stopped sharing
                let found = if ![0x5369_5748, 0x4857_6953].contains(&u32_at(0)) {
                    None
                } else {
                    let (off, size, n) = (u32_at(32) as usize, u32_at(36) as usize, u32_at(40) as usize);
                    let label = |p: *const u8| {
                        let b = std::slice::from_raw_parts(p, 128);
                        String::from_utf8_lossy(&b[..b.iter().position(|&c| c == 0).unwrap_or(128)]).into_owned()
                    };
                    let mut temps = vec![];
                    for i in 0..n.min(4096) {
                        let r = base.add(off + i * size);
                        // reading type 1 = temperature; label at 12, value (f64) at 284
                        if std::ptr::read_unaligned(r as *const u32) == 1 {
                            temps.push((label(r.add(12)), std::ptr::read_unaligned(r.add(284) as *const f64)));
                        }
                    }
                    NAMES.iter().find_map(|n| temps.iter().find(|(l, _)| l == n).map(|(_, v)| *v as f32))
                };
                UnmapViewOfFile(view);
                found
            };
            CloseHandle(h);
            out.filter(|t| (1.0..130.0).contains(t))
        }
    }

    /// Windows' "Thermal Zone Information" performance counters (readable without admin rights).
    pub struct Zones {
        query: isize,
        counter: Option<(isize, bool)>, // (counter, tenths of kelvin)
    }

    impl Zones {
        pub fn open() -> Self {
            let mut z = Zones { query: 0, counter: None };
            unsafe {
                if PdhOpenQueryW(std::ptr::null(), 0, &mut z.query) != 0 {
                    return z;
                }
                for (path, hp) in [("\\Thermal Zone Information(*)\\High Precision Temperature", true), ("\\Thermal Zone Information(*)\\Temperature", false)] {
                    let mut c = 0;
                    if PdhAddEnglishCounterW(z.query, wide(path).as_ptr(), 0, &mut c) == 0 {
                        z.counter = Some((c, hp));
                        break;
                    }
                }
            }
            z
        }

        /// Hottest plausible zone in °C. Zones stuck at 27.85 °C (a common firmware
        /// placeholder) or outside 20..115 °C are ignored.
        pub fn read(&self) -> Option<f32> {
            let (c, hp) = self.counter?;
            unsafe {
                if PdhCollectQueryData(self.query) != 0 {
                    return None;
                }
                let (mut size, mut count) = (0u32, 0u32);
                if PdhGetFormattedCounterArrayW(c, PDH_FMT_DOUBLE, &mut size, &mut count, std::ptr::null_mut()) != PDH_MORE_DATA {
                    return None;
                }
                let mut buf = vec![0u8; size as usize + 64];
                let items = buf.as_mut_ptr() as *mut PDH_FMT_COUNTERVALUE_ITEM_W;
                if PdhGetFormattedCounterArrayW(c, PDH_FMT_DOUBLE, &mut size, &mut count, items) != 0 {
                    return None;
                }
                (0..count as usize)
                    .map(|i| &*items.add(i))
                    .filter(|it| it.FmtValue.CStatus == 0)
                    .map(|it| {
                        let k = it.FmtValue.Anonymous.doubleValue;
                        (if hp { k / 10.0 } else { k }) - 273.15
                    })
                    .filter(|t| (20.0..115.0).contains(t) && (t - 27.85).abs() > 0.06)
                    .fold(None, |m: Option<f64>, t| Some(m.map_or(t, |m| m.max(t))))
                    .map(|t| t as f32)
            }
        }
    }

    impl Drop for Zones {
        fn drop(&mut self) {
            if self.query != 0 {
                unsafe { PdhCloseQuery(self.query) };
            }
        }
    }
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
        let mut cpu_temp = CpuTemp::new();
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
            let (tc, src) = cpu_temp.read().unzip();
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
                tc_source: src.map(String::from),
            };
            let _ = app.emit("pc", &s);
            std::thread::sleep(Duration::from_millis(1500));
        }
    });
}
