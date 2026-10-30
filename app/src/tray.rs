//! Icono de la bandeja del sistema (Shell_NotifyIcon), su menú y los avisos de Windows.

use std::mem::{size_of, zeroed};
use std::ptr::{null, null_mut};

use hdrshift_core::i18n::tr;
use hdrshift_core::trf;
use windows_sys::Win32::Foundation::{HWND, POINT};
use windows_sys::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateBitmap, CreateDIBSection, DIB_RGB_COLORS, DeleteObject,
};
use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LOAD_LIBRARY_SEARCH_SYSTEM32, LoadLibraryExW};
use windows_sys::Win32::UI::HiDpi::{GetDpiForSystem, GetSystemMetricsForDpi};
use windows_sys::Win32::UI::Shell::{
    NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_SHOWTIP, NIF_TIP, NIIF_LARGE_ICON, NIIF_USER, NIM_ADD, NIM_DELETE, NIM_MODIFY,
    NIM_SETVERSION, NOTIFYICON_VERSION_4, NOTIFYICONDATAW, Shell_NotifyIconW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreateIconIndirect, CreatePopupMenu, DestroyIcon, DestroyMenu, GetCursorPos, HICON, ICONINFO,
    MF_GRAYED, MF_SEPARATOR, MF_STRING, SM_CXICON, SM_CXSMICON, SetForegroundWindow, TPM_BOTTOMALIGN, TPM_RETURNCMD,
    TPM_RIGHTBUTTON, TrackPopupMenu,
};

use crate::icon::{Canvas, TrayState};
use crate::ipc::{MSG_TRAY, wide};

pub const CMD_OPEN: usize = 1;
pub const CMD_TOGGLE: usize = 2;
pub const CMD_QUIT: usize = 3;
/// `CMD_END + i` termina la sesión número `i`.
pub const CMD_END: usize = 100;

/// Convierte un lienzo en HICON (32 bits con alfa).
pub fn hicon(c: &Canvas) -> HICON {
    unsafe {
        let s = c.size as i32;
        let mut bmi: BITMAPINFO = zeroed();
        bmi.bmiHeader = BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: s,
            biHeight: -s, // de arriba abajo
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            ..zeroed()
        };
        let mut bits: *mut core::ffi::c_void = null_mut();
        let color = CreateDIBSection(null_mut(), &bmi, DIB_RGB_COLORS, &mut bits, null_mut(), 0);
        if color.is_null() || bits.is_null() {
            return null_mut();
        }
        let rgba = c.rgba8();
        let dst = std::slice::from_raw_parts_mut(bits as *mut u8, rgba.len());
        for (d, sp) in dst.as_chunks_mut::<4>().0.iter_mut().zip(rgba.as_chunks::<4>().0) {
            *d = [sp[2], sp[1], sp[0], sp[3]];
        }
        let mask = CreateBitmap(s, s, 1, 1, null());
        let info = ICONINFO { fIcon: 1, xHotspot: 0, yHotspot: 0, hbmMask: mask, hbmColor: color };
        let icon = CreateIconIndirect(&info);
        DeleteObject(color);
        DeleteObject(mask);
        icon
    }
}

fn small_icon_size() -> u32 {
    unsafe { GetSystemMetricsForDpi(SM_CXSMICON, GetDpiForSystem()).clamp(16, 64) as u32 }
}

fn large_icon_size() -> u32 {
    unsafe { GetSystemMetricsForDpi(SM_CXICON, GetDpiForSystem()).clamp(32, 128) as u32 }
}

fn copy_into(dst: &mut [u16], s: &str) {
    let w: Vec<u16> = s.encode_utf16().take(dst.len() - 1).collect();
    dst[..w.len()].copy_from_slice(&w);
    dst[w.len()] = 0;
}

/// Menús emergentes en modo oscuro (API no documentada de uxtheme que usan muchas apps; si no
/// existe, el menú simplemente sale claro).
pub fn enable_dark_menus() {
    unsafe {
        let lib = LoadLibraryExW(wide("uxtheme.dll").as_ptr(), null_mut(), LOAD_LIBRARY_SEARCH_SYSTEM32);
        if lib.is_null() {
            return;
        }
        // ordinal 135 = SetPreferredAppMode, 136 = FlushMenuThemes
        if let Some(f) = GetProcAddress(lib, 135usize as *const u8) {
            let set: extern "system" fn(i32) -> i32 = std::mem::transmute(f);
            set(1); // AllowDark: sigue el tema de Windows
        }
        if let Some(f) = GetProcAddress(lib, 136usize as *const u8) {
            let flush: extern "system" fn() = std::mem::transmute(f);
            flush();
        }
    }
}

pub struct Tray {
    hwnd: HWND,
    icon: HICON,
    balloon_icon: HICON,
    state: TrayState,
}

impl Tray {
    pub fn new(hwnd: HWND, state: TrayState) -> Self {
        let balloon_icon = hicon(&crate::icon::app_icon(large_icon_size()));
        let mut t = Self { hwnd, icon: null_mut(), balloon_icon, state };
        t.add();
        t
    }

    fn base(&self) -> NOTIFYICONDATAW {
        let mut nid: NOTIFYICONDATAW = unsafe { zeroed() };
        nid.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = self.hwnd;
        nid.uID = 1;
        nid
    }

    /// Añade el icono (también tras reiniciarse el Explorador).
    pub fn add(&mut self) {
        self.set_icon(self.state);
        let mut nid = self.base();
        nid.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP | NIF_SHOWTIP;
        nid.uCallbackMessage = MSG_TRAY;
        nid.hIcon = self.icon;
        copy_into(&mut nid.szTip, "HDRShift");
        unsafe {
            Shell_NotifyIconW(NIM_ADD, &nid);
            nid.Anonymous.uVersion = NOTIFYICON_VERSION_4;
            Shell_NotifyIconW(NIM_SETVERSION, &nid);
        }
    }

    fn set_icon(&mut self, st: TrayState) {
        let new = hicon(&crate::icon::tray_icon(small_icon_size(), st));
        if !self.icon.is_null() {
            unsafe { DestroyIcon(self.icon) };
        }
        self.icon = new;
        self.state = st;
    }

    pub fn update(&mut self, st: TrayState, tip: &str) {
        if self.state != st {
            self.set_icon(st);
        }
        let mut nid = self.base();
        nid.uFlags = NIF_ICON | NIF_TIP | NIF_SHOWTIP;
        nid.hIcon = self.icon;
        copy_into(&mut nid.szTip, tip);
        unsafe { Shell_NotifyIconW(NIM_MODIFY, &nid) };
    }

    /// Aviso de Windows (sale como notificación en Windows 10/11).
    pub fn notify(&self, title: &str, text: &str) {
        let mut nid = self.base();
        nid.uFlags = NIF_INFO;
        copy_into(&mut nid.szInfoTitle, title);
        copy_into(&mut nid.szInfo, text);
        nid.dwInfoFlags = NIIF_USER | NIIF_LARGE_ICON;
        nid.hBalloonIcon = self.balloon_icon;
        unsafe { Shell_NotifyIconW(NIM_MODIFY, &nid) };
    }
}

/// Menú contextual. `sessions` son las apps con el HDR encendido. Es una función libre porque
/// `TrackPopupMenu` corre un bucle modal que vuelve a entrar en el procedimiento de ventana.
pub fn show_menu(hwnd: HWND, sessions: &[String], hdr_on: bool) -> usize {
    unsafe {
        let m = CreatePopupMenu();
        for (i, name) in sessions.iter().enumerate() {
            let line = trf!("Terminar la sesión de {name}", "End the session of {name}");
            AppendMenuW(m, MF_STRING, CMD_END + i, wide(&line).as_ptr());
        }
        if !sessions.is_empty() {
            AppendMenuW(m, MF_SEPARATOR, 0, null());
        }
        let toggle = if hdr_on { tr("Apagar el HDR", "Turn HDR off") } else { tr("Encender el HDR", "Turn HDR on") };
        AppendMenuW(m, MF_STRING | if sessions.is_empty() { 0 } else { MF_GRAYED }, CMD_TOGGLE, wide(toggle).as_ptr());
        AppendMenuW(m, MF_STRING, CMD_OPEN, wide(tr("Abrir HDRShift", "Open HDRShift")).as_ptr());
        AppendMenuW(m, MF_SEPARATOR, 0, null());
        AppendMenuW(m, MF_STRING, CMD_QUIT, wide(tr("Salir", "Quit")).as_ptr());
        let mut pt = POINT { x: 0, y: 0 };
        GetCursorPos(&mut pt);
        SetForegroundWindow(hwnd);
        let cmd = TrackPopupMenu(m, TPM_RIGHTBUTTON | TPM_BOTTOMALIGN | TPM_RETURNCMD, pt.x, pt.y, 0, hwnd, null());
        DestroyMenu(m);
        cmd as usize
    }
}

impl Drop for Tray {
    fn drop(&mut self) {
        let nid = self.base();
        unsafe {
            Shell_NotifyIconW(NIM_DELETE, &nid);
            if !self.icon.is_null() {
                DestroyIcon(self.icon);
            }
            if !self.balloon_icon.is_null() {
                DestroyIcon(self.balloon_icon);
            }
        }
    }
}
