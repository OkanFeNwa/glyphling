//! Le poche chiamate dirette a Windows. Sugli altri sistemi restituiscono un valore neutro.

#[cfg(windows)]
mod imp {
    use windows_sys::Win32::Foundation::POINT;
    use windows_sys::Win32::System::Registry::{RegGetValueW, HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ};
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LBUTTON, VK_MENU};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        FindWindowW, GetCursorPos, IsIconic, IsWindowVisible, SetWindowLongPtrW, ShowWindow, GWLP_HWNDPARENT, SW_SHOWNOACTIVATE,
    };

    pub fn cursor_pos() -> Option<(i32, i32)> {
        let mut p = POINT { x: 0, y: 0 };
        (unsafe { GetCursorPos(&mut p) } != 0).then_some((p.x, p.y))
    }

    pub fn left_button_down() -> bool {
        (unsafe { GetAsyncKeyState(VK_LBUTTON as i32) } as u16 & 0x8000) != 0
    }

    pub fn alt_down() -> bool {
        (unsafe { GetAsyncKeyState(VK_MENU as i32) } as u16 & 0x8000) != 0
    }

    /// Mostra una finestra senza rubare il focus (serve alla griglia durante il trascinamento).
    pub fn show_no_activate(w: &tauri::WebviewWindow) {
        if let Ok(h) = w.hwnd() {
            unsafe { ShowWindow(h.0 as _, SW_SHOWNOACTIVATE) };
        }
    }

    /// Sul livello desktop il widget appartiene alla finestra del desktop (Progman), come i
    /// vecchi gadget: così "Mostra desktop" (Win+D o l'angolo della barra) non lo nasconde.
    pub fn set_desktop_owner(w: &tauri::WebviewWindow, on: bool) {
        let Ok(h) = w.hwnd() else { return };
        let class: Vec<u16> = "Progman".encode_utf16().chain(Some(0)).collect();
        let owner = if on { unsafe { FindWindowW(class.as_ptr(), std::ptr::null()) } } else { std::ptr::null_mut() };
        unsafe { SetWindowLongPtrW(h.0 as _, GWLP_HWNDPARENT, owner as isize) };
    }

    /// Se Windows ha ridotto a icona o nascosto il widget, lo rimette dov'era senza rubare il focus.
    /// Restituisce true se ha dovuto ripristinarlo.
    pub fn ensure_shown(w: &tauri::WebviewWindow) -> bool {
        let Ok(h) = w.hwnd() else { return false };
        let h = h.0 as _;
        unsafe {
            if IsIconic(h) != 0 || IsWindowVisible(h) == 0 {
                ShowWindow(h, SW_SHOWNOACTIVATE);
                return true;
            }
        }
        false
    }

    fn reg(name: &str) -> Option<String> {
        let wide = |s: &str| s.encode_utf16().chain(Some(0)).collect::<Vec<u16>>();
        let key = wide(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion");
        let val = wide(name);
        let mut buf = [0u16; 128];
        let mut len = (buf.len() * 2) as u32;
        let r = unsafe {
            RegGetValueW(HKEY_LOCAL_MACHINE, key.as_ptr(), val.as_ptr(), RRF_RT_REG_SZ, std::ptr::null_mut(), buf.as_mut_ptr() as _, &mut len)
        };
        if r != 0 {
            return None;
        }
        let n = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        Some(String::from_utf16_lossy(&buf[..n]))
    }

    pub fn os_name() -> String {
        let build: u32 = reg("CurrentBuildNumber").and_then(|b| b.parse().ok()).unwrap_or(0);
        let name = if build >= 22000 { "Windows 11" } else { "Windows 10" };
        match reg("DisplayVersion") {
            Some(v) => format!("{name} {v} · build {build} · {}", std::env::consts::ARCH),
            None => format!("{name} · build {build} · {}", std::env::consts::ARCH),
        }
    }
}

#[cfg(not(windows))]
mod imp {
    pub fn cursor_pos() -> Option<(i32, i32)> {
        None
    }
    pub fn left_button_down() -> bool {
        false
    }
    pub fn alt_down() -> bool {
        false
    }
    pub fn show_no_activate(w: &tauri::WebviewWindow) {
        let _ = w.show();
    }
    pub fn set_desktop_owner(_: &tauri::WebviewWindow, _: bool) {}
    pub fn ensure_shown(_: &tauri::WebviewWindow) -> bool {
        false
    }
    pub fn os_name() -> String {
        format!("{} · {}", std::env::consts::OS, std::env::consts::ARCH)
    }
}

pub use imp::*;
