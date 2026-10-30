//! Icono real de cada app, leído de su exe.

use std::collections::HashMap;
use std::ptr::null_mut;

use eframe::egui::{self, TextureHandle};
use windows_sys::Win32::Graphics::Gdi::{
    BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, DeleteObject, GetDC, GetDIBits, GetObjectW, ReleaseDC,
};
use windows_sys::Win32::UI::Shell::SHDefExtractIconW;
use windows_sys::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, HICON, ICONINFO};

use crate::ipc::wide;

const SIZE: u32 = 96;

/// Píxeles RGBA y lado del icono de un exe.
fn extract(path: &str) -> Option<(Vec<u8>, usize)> {
    unsafe {
        let mut icon: HICON = null_mut();
        if SHDefExtractIconW(wide(path).as_ptr(), 0, 0, &mut icon, null_mut(), SIZE) != 0 || icon.is_null() {
            return None;
        }
        let mut info: ICONINFO = std::mem::zeroed();
        let got = GetIconInfo(icon, &mut info);
        DestroyIcon(icon);
        if got == 0 {
            return None;
        }
        let mut bitmap: BITMAP = std::mem::zeroed();
        GetObjectW(info.hbmColor as _, size_of::<BITMAP>() as i32, &mut bitmap as *mut BITMAP as *mut _);
        let side = bitmap.bmWidth.max(0) as usize;
        let mut px = vec![0u8; side * side * 4];
        let mut bmi: BITMAPINFO = std::mem::zeroed();
        bmi.bmiHeader = BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: side as i32,
            biHeight: -(side as i32), // de arriba abajo
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            ..std::mem::zeroed()
        };
        let dc = GetDC(null_mut());
        let lines = GetDIBits(dc, info.hbmColor, 0, side as u32, px.as_mut_ptr() as *mut _, &mut bmi, DIB_RGB_COLORS);
        ReleaseDC(null_mut(), dc);
        DeleteObject(info.hbmColor as _);
        DeleteObject(info.hbmMask as _);
        if lines == 0 || side == 0 {
            return None;
        }
        for p in px.as_chunks_mut::<4>().0 {
            p.swap(0, 2);
        }
        // los iconos antiguos no traen transparencia
        if px.as_chunks::<4>().0.iter().all(|p| p[3] == 0) {
            for p in px.as_chunks_mut::<4>().0 {
                p[3] = 255;
            }
        }
        Some((px, side))
    }
}

/// Texturas ya cargadas: cada exe se lee una sola vez.
#[derive(Default)]
pub struct Cache {
    icons: HashMap<String, Option<TextureHandle>>,
}

impl Cache {
    pub fn get(&mut self, ctx: &egui::Context, exe: &str) -> Option<&TextureHandle> {
        self.icons
            .entry(exe.to_string())
            .or_insert_with(|| {
                let (px, side) = extract(exe)?;
                let image = egui::ColorImage::from_rgba_unmultiplied([side, side], &px);
                Some(ctx.load_texture(format!("icon-{exe}"), image, egui::TextureOptions::LINEAR))
            })
            .as_ref()
    }
}
