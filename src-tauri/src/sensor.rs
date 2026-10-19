//! Optional CPU temperature sensor: LibreHardwareMonitor, installed only when the user asks.
//!
//! It is downloaded from its official GitHub releases into the app's data folder, set up to
//! expose its web server on port 8085 and started at logon through a scheduled task with
//! highest privileges (reading the CPU sensors needs administrator rights, so Windows asks
//! for consent once). `system.rs` then picks the temperature up from the web server.

use std::path::PathBuf;
use tauri::{AppHandle, Manager};

pub const TASK: &str = "Glyphling-Sensor";

pub fn dir(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_local_data_dir().ok().map(|d| d.join("sensors").join("LibreHardwareMonitor"))
}

pub fn installed(app: &AppHandle) -> bool {
    dir(app).is_some_and(|d| d.join("LibreHardwareMonitor.exe").exists())
}

#[cfg(windows)]
pub fn install(app: &AppHandle) -> Result<(), String> {
    use std::io::Read;
    use std::os::windows::process::CommandExt;
    use std::process::Command;
    const NO_WINDOW: u32 = 0x0800_0000;

    let dir = dir(app).ok_or("cartella dati non disponibile")?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    // 1. latest official release
    let body = ureq::get("https://api.github.com/repos/LibreHardwareMonitor/LibreHardwareMonitor/releases/latest")
        .set("User-Agent", "Glyphling")
        .timeout(std::time::Duration::from_secs(20))
        .call()
        .map_err(|e| format!("download non riuscito: {e}"))?
        .into_string()
        .map_err(|e| e.to_string())?;
    let rel: serde_json::Value = serde_json::from_str(&body).map_err(|e| e.to_string())?;
    let assets = rel["assets"].as_array().ok_or("nessun file nella release")?;
    let url = assets
        .iter()
        .find(|a| a["name"] == "LibreHardwareMonitor.zip")
        .or_else(|| assets.iter().find(|a| a["name"].as_str().is_some_and(|n| n.starts_with("LibreHardwareMonitor") && n.ends_with(".zip") && !n.contains("NET"))))
        .and_then(|a| a["browser_download_url"].as_str())
        .ok_or("file di LibreHardwareMonitor non trovato")?
        .to_string();
    if !url.starts_with("https://github.com/") {
        return Err("indirizzo di download inatteso".into());
    }

    // 2. download and unpack
    let zip = dir.with_extension("zip");
    let mut bytes = Vec::new();
    ureq::get(&url)
        .set("User-Agent", "Glyphling")
        .timeout(std::time::Duration::from_secs(120))
        .call()
        .map_err(|e| format!("download non riuscito: {e}"))?
        .into_reader()
        .take(80_000_000)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    std::fs::write(&zip, bytes).map_err(|e| e.to_string())?;
    let q = |p: &std::path::Path| p.to_string_lossy().replace('\'', "''");
    let st = Command::new("powershell")
        .creation_flags(NO_WINDOW)
        .args(["-NoProfile", "-Command", &format!("Expand-Archive -LiteralPath '{}' -DestinationPath '{}' -Force", q(&zip), q(&dir))])
        .status()
        .map_err(|e| e.to_string())?;
    let _ = std::fs::remove_file(&zip);
    let exe = dir.join("LibreHardwareMonitor.exe");
    if !st.success() || !exe.exists() {
        return Err("impossibile estrarre LibreHardwareMonitor".into());
    }

    // 3. web server on, starts minimized to the tray
    let cfg = dir.join("LibreHardwareMonitor.config");
    let _ = std::fs::write(
        cfg,
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<configuration><appSettings>\n\
         <add key=\"startMinMenuItem\" value=\"true\" />\n<add key=\"minTrayMenuItem\" value=\"true\" />\n\
         <add key=\"minCloseMenuItem\" value=\"true\" />\n<add key=\"runWebServerMenuItem\" value=\"true\" />\n\
         <add key=\"listenerPort\" value=\"8085\" />\n</appSettings></configuration>\n",
    );

    // 4. scheduled task at logon with highest privileges: the only step that needs consent
    let ps1 = dir.join("setup-task.ps1");
    let user = format!("{}\\{}", std::env::var("USERDOMAIN").unwrap_or_default(), std::env::var("USERNAME").unwrap_or_default());
    std::fs::write(
        &ps1,
        format!(
            "param([string]$User)\n$a = New-ScheduledTaskAction -Execute '{exe}' -WorkingDirectory '{dir}'\n\
             $t = New-ScheduledTaskTrigger -AtLogOn -User $User\n\
             $p = New-ScheduledTaskPrincipal -UserId $User -RunLevel Highest -LogonType Interactive\n\
             $s = New-ScheduledTaskSettingsSet -AllowStartIfOnBatteries -DontStopIfGoingOnBatteries -ExecutionTimeLimit ([TimeSpan]::Zero)\n\
             Register-ScheduledTask -TaskName '{task}' -Action $a -Trigger $t -Principal $p -Settings $s -Force | Out-Null\n\
             Start-ScheduledTask -TaskName '{task}'\n",
            exe = q(&exe),
            dir = q(&dir),
            task = TASK
        ),
    )
    .map_err(|e| e.to_string())?;
    let arg = format!(
        "Start-Process powershell -Verb RunAs -Wait -WindowStyle Hidden -ArgumentList '-NoProfile','-ExecutionPolicy','Bypass','-File','\"{}\"','-User','\"{}\"'",
        q(&ps1),
        user.replace('\'', "''")
    );
    let st = Command::new("powershell").creation_flags(NO_WINDOW).args(["-NoProfile", "-Command", &arg]).status().map_err(|e| e.to_string())?;
    if !st.success() {
        return Err("autorizzazione negata".into());
    }

    // 5. wait for the web server
    for _ in 0..20 {
        if ureq::get("http://127.0.0.1:8085/data.json").timeout(std::time::Duration::from_millis(800)).call().is_ok() {
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
    Err("il sensore non si è avviato (autorizzazione negata?)".into())
}

#[cfg(not(windows))]
pub fn install(_: &AppHandle) -> Result<(), String> {
    Err("disponibile solo su Windows".into())
}

/// Whether LibreHardwareMonitor's web server answers.
pub fn web_alive() -> bool {
    ureq::get("http://127.0.0.1:8085/data.json").timeout(std::time::Duration::from_millis(1200)).call().is_ok()
}

/// Starts the sensor's scheduled task (no elevation needed: the task already runs elevated).
#[cfg(windows)]
pub fn start() {
    use std::os::windows::process::CommandExt;
    let _ = std::process::Command::new("schtasks").args(["/Run", "/TN", TASK]).creation_flags(0x0800_0000).status();
}

#[cfg(not(windows))]
pub fn start() {}

/// If it was installed but is not running (closed, crashed, task not fired), start it again.
pub fn spawn(app: AppHandle) {
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(20));
        if installed(&app) && !web_alive() {
            start();
        }
    });
}
