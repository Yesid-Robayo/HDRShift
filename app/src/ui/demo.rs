//! Solo en compilaciones de desarrollo: abre una pantalla con datos de ejemplo y guarda una captura.
//! Las imágenes del README salen de aquí, sin mostrar las apps ni el monitor de nadie.
//!
//! `HDRSHIFT_DEMO` = `on`, `off`, `edit`, `settings` o `welcome`; `HDRSHIFT_DEMO_SHOT` = archivo .bmp.
//! `HDRSHIFT_DEMO_PORTABLE` pulsa "Guardar los datos junto al exe", para probarlo sin ratón.

use std::sync::atomic::{AtomicU32, Ordering};

use eframe::egui;
use hdrshift_core::display::Display;
use hdrshift_core::session::{Hold, HoldKind};

use super::{App, Page, editor, settings, welcome};

const MONITOR: &str = "\\\\?\\DISPLAY#DEMO001#1#{0}";
/// Da tiempo a que terminen las animaciones.
const CAPTURE_FRAME: u32 = 90;

static FRAMES: AtomicU32 = AtomicU32::new(0);

fn mode() -> Option<String> {
    std::env::var("HDRSHIFT_DEMO").ok()
}

pub fn page(hwnd: isize) -> Option<Page> {
    let page = match mode()?.as_str() {
        "welcome" => Page::Welcome(Box::new(welcome::Welcome::new())),
        "settings" => Page::Settings(Box::new(settings::Settings::new(hwnd))),
        "edit" => {
            Page::Edit(Box::new(editor::Editor::new(None, &[Display::new(MONITOR, "HDR 27", true, false)], hwnd)))
        }
        _ => Page::Home,
    };
    Some(page)
}

/// Pone datos de ejemplo en lugar de los reales.
pub fn fill(app: &mut App) -> bool {
    let Some(mode) = mode() else { return false };
    let on = mode == "on";
    app.monitors = vec![Display::new(MONITOR, "HDR 27", true, on)];
    app.snapshot.holds = app
        .cfg
        .apps
        .first()
        .filter(|_| on)
        .map(|a| Hold {
            kind: HoldKind::App(a.id.clone()),
            label: a.name.clone(),
            monitors: [MONITOR.to_string()].into(),
            started_at: hdrshift_core::now_secs() as u64 - 2520,
        })
        .into_iter()
        .collect();
    true
}

/// Guarda la captura y cierra.
pub fn capture(ctx: &egui::Context) {
    if std::env::var_os("HDRSHIFT_DEMO_PORTABLE").is_some() && FRAMES.fetch_add(1, Ordering::Relaxed) == 2 {
        // la ventana que reabre no debe repetirlo
        unsafe { std::env::remove_var("HDRSHIFT_DEMO_PORTABLE") };
        super::settings::move_to_portable(ctx);
    }
    let Ok(path) = std::env::var("HDRSHIFT_DEMO_SHOT") else { return };
    let frame = FRAMES.fetch_add(1, Ordering::Relaxed);
    ctx.request_repaint();
    if frame == CAPTURE_FRAME {
        ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::ScreenshotCallback::new(move |image| {
            write_bmp(&image, &path);
            std::process::exit(0);
        })));
    }
}

fn write_bmp(image: &egui::ColorImage, path: &str) {
    let [w, h] = image.size;
    let mut px = Vec::with_capacity(w * h * 4);
    for y in (0..h).rev() {
        for x in 0..w {
            let c = image.pixels[y * w + x];
            px.extend([c.b(), c.g(), c.r(), 255]);
        }
    }
    let mut f = Vec::with_capacity(54 + px.len());
    f.extend(b"BM");
    f.extend((54 + px.len() as u32).to_le_bytes());
    f.extend([0u8; 4]);
    f.extend(54u32.to_le_bytes());
    f.extend(40u32.to_le_bytes());
    f.extend((w as i32).to_le_bytes());
    f.extend((h as i32).to_le_bytes());
    f.extend(1u16.to_le_bytes());
    f.extend(32u16.to_le_bytes());
    f.extend([0u8; 24]);
    f.extend(px);
    let _ = std::fs::write(path, f);
}
