# Glyphling

A tiny animated dot-matrix companion for your Windows desktop: an LED mascot inside a circle, with a glass panel for clock, calendar, music, timer, weather and PC stats. Built with Tauri 2 (Rust backend, web interface). Not affiliated with Nothing.

The full design document lives in the project's shared files, not in this repository.

## Download

Installers are published on the [Releases](../../releases) page. Download `Glyphling-Setup-x.y.z.exe` and run it. The same file installs, updates in place (settings, themes and position are kept) and, as `uninstall.exe` in the install folder, removes the app. The setup is not signed yet, so Windows SmartScreen may warn on first launch ("More info" → "Run anyway").

Glyphling checks the GitHub releases by itself (Settings > Info can turn that off), tells you when a new version exists and, only when you agree, downloads the setup and installs it in place.

## What it does (0.9)

- **Widget**: transparent, frameless, clipped to its real shape (circle plus panel), with a dark glass look. Windows' own blur behind the window is not used: on some PCs it turned the transparent parts black. The panel opens to the right, to the left or automatically, and its layout mirrors when it opens to the left. Turn the dial (or the mouse wheel) to pick a mode, right click the circle to go back to the mascot.
- **First run**: a short tutorial walks through the mascot, the dial, the panel, the tray icon and the settings (Settings > Info to see it again). While it runs the widget window briefly covers the screen to dim it and point at the tray icon.
- **Placement**: the circle is the only reference point. Drag it anywhere (snapping to a 24 px grid, Alt = free) or pick a quick position: corners, edge middles or center, 24 px from the edges and above the taskbar. When the panel opens it picks a side (Auto: toward the middle of the screen, or the other side if only that one has room); if the side has no room the circle slides away from the edge while the panel is open and comes back when it closes. With the middle quick positions, circle + panel are centered together. Levels: on the desktop (survives Show desktop / Win+D), normal, always on top.
- **Tray icon**: left click shows or hides the widget, right click opens a glass menu.
- **Settings**: frameless glass window with sidebar navigation.
- **Real data**:
  - Music from any player using the Windows media controls (Spotify, browsers, VLC...): title, artist, progress, controls, cover art on the LEDs, master volume, visualizer from the real audio spectrum.
  - Weather from Open-Meteo; city chosen in Settings (Milan by default).
  - Google Calendar through the secret iCal address pasted in Settings (recurring events included).
  - CPU, RAM and disks; NVIDIA GPU usage and temperature; CPU temperature (see below).
- **Circle**: 512 to 2048 LED points; 24/12 hour clock and °C/°F (Settings).
- **Font**: Glyphling Dot, our own 5x7 dot font (`tools/make_font.py`). If Nothing's 5x7 font is installed on the PC it is used instead (it cannot be redistributed).
- Easter eggs: shake the mouse left and right over the mascot, or circle around it fast.
- Music follows one app at a time. Keep a list of the apps you want (running ones or any .exe) and the widget shows and controls only those; the circle's volume lowers one chosen mixer entry, never the system volume.
- Starts with Windows (optional), single instance.

## CPU temperature

Windows has no public API for the CPU's own temperature sensor. The value lives in model-specific registers of the processor, which can only be read from kernel mode, so every monitoring tool ships a kernel driver and runs with administrator rights. Glyphling deliberately does not: a widget that starts with Windows should not install a driver or ask for elevation at every boot, and the drivers commonly used for this (such as WinRing0) are now blocked by Windows Defender as vulnerable.

So Glyphling reads the temperature from the first source that has it:

1. **HWiNFO** (free, [hwinfo.com](https://www.hwinfo.com)). In HWiNFO open *Settings* and enable *Shared Memory Support*, then keep HWiNFO running (sensors only mode works, it can be minimized to the tray). Glyphling reads the shared memory, looking for `CPU (Tctl/Tdie)` on AMD or `CPU Package` on Intel. In the free version shared memory turns itself off after 12 hours and has to be re-enabled.
2. **LibreHardwareMonitor** (open source, [GitHub](https://github.com/LibreHardwareMonitor/LibreHardwareMonitor)). Enable *Options > Remote Web Server* (port 8085) and keep it running.
3. **Windows thermal zone**, with no extra program and no admin rights. This is the ACPI thermal zone exposed by the firmware through Windows performance counters. On most laptops it follows the CPU closely; on many desktops it is a motherboard sensor or a fixed placeholder (27.85 °C is ignored), so Settings marks it as approximate.

If no source is found, Settings offers to install LibreHardwareMonitor for you. It is downloaded from its official releases into the app's data folder and started at logon through a scheduled task with highest privileges (Windows asks for administrator consent once); the uninstaller removes the task and the files on a best-effort basis. Nothing is installed without pressing that button.

Settings shows the current reading and its source. NVIDIA GPU temperature needs nothing extra (NVML ships with the driver).

## Repository layout

- `src/index.html`: the interface, generated by `tools/port_prototype.py` from the design prototype (`tools/prototipo-*.html`). Do not edit it by hand: update the prototype and run the script again.
- `src/host.js`, `src/host.css`, `src/host-pre.js`: adapt the prototype to the app windows and connect it to the backend.
- `src/menu.html`: the tray menu. `src/grid.html`: the dot grid shown while dragging.
- `src-tauri/`: the app backend. `lib.rs` (state and commands), `tray.rs` (icon and menu), `snap.rs` (dragging, snapping, panel side), `hit.rs` (shape), `platform.rs` (Win32 calls), `config.rs`, `system.rs` (PC stats), `media.rs` (music and volume), `spectrum.rs` (visualizer).
- `setup/`: the custom installer / updater / uninstaller (a small Tauri app with its own page in `setup/ui/`). The built app is compressed and embedded into it at build time.
- `.github/workflows/release.yml`: builds everything on Windows and publishes a release when a `v*` tag is pushed.

## Building

On Windows with Rust (`rustup`) and Node.js LTS:

```
npm ci
npx tauri dev                       # run the app in development
npx tauri build --no-bundle         # build src-tauri/target/release/glyphling.exe
$env:GLYPHLING_PAYLOAD_DIR = "$PWD\src-tauri\target\release"
cargo build --release --manifest-path setup/Cargo.toml --target-dir src-tauri/target
# -> src-tauri/target/release/glyphling-setup.exe
```

Releasing: bump the version in `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml` and `setup/Cargo.toml`, add `.github/release-notes/vX.Y.Z.md`, then push to `main`. The workflow sees the new version, builds the setup and publishes release `vX.Y.Z` with it (pushing a `vX.Y.Z` tag works too).

Cross-compiling from Linux works for checking the code (`--target x86_64-pc-windows-gnu`, needs mingw-w64), but a gnu build needs `WebView2Loader.dll` next to each executable, so the single-file setup must be built with MSVC (as the workflow does).

## Notes

- Install folder: `%LOCALAPPDATA%\Glyphling`. Settings: `%APPDATA%\com.okanfenwa.glyphling\config.json`.
- Calendar address, city and chosen disks stay on the PC (the windows' local storage).
- Fonts: Glyphling Dot (ours), Doto and Space Mono (SIL Open Font License), in `src/fonts/`.
- Icons: [Lucide](https://lucide.dev) (ISC License).
