//! The few direct Windows calls. On other systems they return a neutral value.

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

    /// Shows a window without stealing focus (used by the grid while dragging).
    pub fn show_no_activate(w: &tauri::WebviewWindow) {
        if let Ok(h) = w.hwnd() {
            unsafe { ShowWindow(h.0 as _, SW_SHOWNOACTIVATE) };
        }
    }

    /// On the desktop level the widget is owned by the desktop window (Progman), like the
    /// old gadgets: this way "Show desktop" (Win+D or the taskbar corner) does not hide it.
    pub fn set_desktop_owner(w: &tauri::WebviewWindow, on: bool) {
        let Ok(h) = w.hwnd() else { return };
        let class: Vec<u16> = "Progman".encode_utf16().chain(Some(0)).collect();
        let owner = if on { unsafe { FindWindowW(class.as_ptr(), std::ptr::null()) } } else { std::ptr::null_mut() };
        unsafe { SetWindowLongPtrW(h.0 as _, GWLP_HWNDPARENT, owner as isize) };
    }

    /// Removes the caption and frame styles for good. A frameless window keeps them on some
    /// PCs, and Windows then paints a standard title bar over the page when the window is
    /// not focused. Called again whenever the focus changes.
    pub fn strip_frame(w: &tauri::WebviewWindow) {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            GetWindowLongPtrW, SetWindowPos, GWL_STYLE, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, WS_CAPTION, WS_THICKFRAME,
        };
        let Ok(h) = w.hwnd() else { return };
        let h = h.0 as _;
        unsafe {
            let style = GetWindowLongPtrW(h, GWL_STYLE);
            let clean = style & !((WS_CAPTION | WS_THICKFRAME) as isize);
            if clean != style {
                SetWindowLongPtrW(h, GWL_STYLE, clean);
                SetWindowPos(h, std::ptr::null_mut(), 0, 0, 0, 0, SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
            }
        }
    }

    /// Removes the non-client area for good. The window becomes a plain popup (no caption, frame
    /// or system menu) and its window procedure ignores WM_NCCALCSIZE and never paints or
    /// activates the non-client area, so a focus change cannot bring a title bar back.
    /// Subclassing must happen on the thread that owns the window, hence the hop.
    pub fn no_frame(w: &tauri::WebviewWindow) {
        let w2 = w.clone();
        let _ = w.run_on_main_thread(move || no_frame_here(&w2));
    }

    fn no_frame_here(w: &tauri::WebviewWindow) {
        use windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute;
        use windows_sys::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            GetWindowLongPtrW, SetWindowLongPtrW, SetWindowPos, GWL_EXSTYLE, GWL_STYLE, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
            SWP_NOZORDER, WM_NCACTIVATE, WM_NCCALCSIZE, WM_NCPAINT, WS_CAPTION, WS_EX_CLIENTEDGE, WS_EX_DLGMODALFRAME, WS_EX_STATICEDGE,
            WS_EX_WINDOWEDGE, WS_MAXIMIZEBOX, WS_POPUP, WS_SYSMENU, WS_THICKFRAME,
        };
        unsafe extern "system" fn proc(h: *mut std::ffi::c_void, msg: u32, wp: usize, lp: isize, _id: usize, _d: usize) -> isize {
            match msg {
                WM_NCCALCSIZE if wp != 0 => 0,
                WM_NCPAINT => 0,
                WM_NCACTIVATE => DefSubclassProc(h, msg, wp, -1),
                _ => DefSubclassProc(h, msg, wp, lp),
            }
        }
        let Ok(h) = w.hwnd() else { return };
        let h = h.0 as _;
        unsafe {
            // DWMWA_NCRENDERING_POLICY = 2, DWMNCRP_DISABLED = 1
            let off: i32 = 1;
            DwmSetWindowAttribute(h, 2, &off as *const i32 as *const _, 4);
            SetWindowSubclass(h, Some(proc), 1, 0);
            let style = GetWindowLongPtrW(h, GWL_STYLE);
            let clean = (style & !((WS_CAPTION | WS_THICKFRAME | WS_SYSMENU | WS_MAXIMIZEBOX) as isize)) | WS_POPUP as isize;
            SetWindowLongPtrW(h, GWL_STYLE, clean);
            let ex = GetWindowLongPtrW(h, GWL_EXSTYLE);
            SetWindowLongPtrW(h, GWL_EXSTYLE, ex & !((WS_EX_DLGMODALFRAME | WS_EX_WINDOWEDGE | WS_EX_CLIENTEDGE | WS_EX_STATICEDGE) as isize));
            SetWindowPos(h, std::ptr::null_mut(), 0, 0, 0, 0, SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
        }
    }

    /// Clips the window to the shape (union of rounded rectangles, logical pixels): outside it
    /// clicks pass through and the background blur stays inside the shape.
    pub fn set_region(w: &tauri::WebviewWindow, parts: &[crate::hit::Part]) {
        use windows_sys::Win32::Graphics::Gdi::{CombineRgn, CreateRoundRectRgn, DeleteObject, SetWindowRgn, HRGN, RGN_OR};
        let (Ok(h), Ok(sf)) = (w.hwnd(), w.scale_factor()) else { return };
        unsafe {
            let mut rgn: HRGN = std::ptr::null_mut();
            for p in parts {
                let px = |v: f64| (v * sf).round() as i32;
                let d = px(2.0 * p.r.min(p.w / 2.0).min(p.h / 2.0));
                let r = CreateRoundRectRgn(px(p.x), px(p.y), px(p.x + p.w) + 1, px(p.y + p.h) + 1, d, d);
                if rgn.is_null() {
                    rgn = r;
                } else {
                    CombineRgn(rgn, rgn, r, RGN_OR);
                    DeleteObject(r);
                }
            }
            // after SetWindowRgn the region belongs to Windows: do not delete it
            // (no parts: a null region, which means the whole window)
            SetWindowRgn(h.0 as _, rgn, 1);
        }
    }

    /// If Windows minimized or hid the widget, puts it back without stealing focus.
    /// Returns true if it had to restore it.
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
    pub fn strip_frame(_: &tauri::WebviewWindow) {}
    pub fn no_frame(_: &tauri::WebviewWindow) {}
    pub fn set_region(_: &tauri::WebviewWindow, _: &[crate::hit::Part]) {}
    pub fn ensure_shown(_: &tauri::WebviewWindow) -> bool {
        false
    }
    pub fn os_name() -> String {
        format!("{} · {}", std::env::consts::OS, std::env::consts::ARCH)
    }
}

pub use imp::*;
