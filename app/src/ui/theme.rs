//! Paletas clara y oscura, tipografía y estilo de la ventana.

use std::sync::atomic::{AtomicBool, Ordering};

use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, Stroke, TextStyle, Visuals,
};
use hdrshift_core::config::ThemeChoice;
use hdrshift_core::registry;

/// Degradado de marca, del azul profundo al cian. Es igual en los dos temas y siempre lleva texto blanco.
pub const BRAND_A: Color32 = Color32::from_rgb(0x0b, 0x4f, 0x9e);
pub const BRAND_B: Color32 = Color32::from_rgb(0x0e, 0x7f, 0xc9);
pub const BRAND_C: Color32 = Color32::from_rgb(0x12, 0xb5, 0xcb);
pub const ON_ACCENT: Color32 = Color32::WHITE;

pub struct Palette {
    pub bg: Color32,
    pub surface: Color32,
    pub surface_hi: Color32,
    pub surface_2: Color32,
    pub border: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub faint: Color32,
    pub good: Color32,
    pub warn: Color32,
    pub low: Color32,
    pub accent: Color32,
    pub light: bool,
}

const DARK: Palette = Palette {
    bg: Color32::from_rgb(0x0c, 0x0c, 0x13),
    surface: Color32::from_rgb(0x15, 0x15, 0x20),
    surface_hi: Color32::from_rgb(0x1c, 0x1c, 0x2a),
    surface_2: Color32::from_rgb(0x25, 0x25, 0x36),
    border: Color32::from_rgb(0x2a, 0x2a, 0x3d),
    text: Color32::from_rgb(0xee, 0xee, 0xf6),
    muted: Color32::from_rgb(0x96, 0x96, 0xae),
    faint: Color32::from_rgb(0x66, 0x66, 0x7e),
    good: Color32::from_rgb(0x4a, 0xde, 0x9c),
    warn: Color32::from_rgb(0xf5, 0xc4, 0x51),
    low: Color32::from_rgb(0xff, 0x5d, 0x6c),
    accent: Color32::from_rgb(0x5c, 0xc0, 0xff),
    light: false,
};

const LIGHT: Palette = Palette {
    bg: Color32::from_rgb(0xf4, 0xf4, 0xf9),
    surface: Color32::from_rgb(0xff, 0xff, 0xff),
    surface_hi: Color32::from_rgb(0xfa, 0xf9, 0xff),
    surface_2: Color32::from_rgb(0xea, 0xea, 0xf2),
    border: Color32::from_rgb(0xdf, 0xdf, 0xea),
    text: Color32::from_rgb(0x1a, 0x1a, 0x28),
    muted: Color32::from_rgb(0x5e, 0x5e, 0x76),
    faint: Color32::from_rgb(0x8e, 0x8e, 0xa4),
    good: Color32::from_rgb(0x0b, 0x8f, 0x57),
    warn: Color32::from_rgb(0xa8, 0x6c, 0x00),
    low: Color32::from_rgb(0xd0, 0x30, 0x42),
    accent: Color32::from_rgb(0x0b, 0x6f, 0xb8),
    light: true,
};

static IS_LIGHT: AtomicBool = AtomicBool::new(false);

/// Colores del tema en uso.
pub fn c() -> &'static Palette {
    if IS_LIGHT.load(Ordering::Relaxed) { &LIGHT } else { &DARK }
}

/// `true` si la elección lleva al tema claro.
pub fn wants_light(choice: ThemeChoice) -> bool {
    match choice {
        ThemeChoice::Light => true,
        ThemeChoice::Dark => false,
        ThemeChoice::System => {
            let key = "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize";
            registry::user_dword(key, "AppsUseLightTheme") != Some(0)
        }
    }
}

pub const SEMIBOLD: &str = "semibold";
pub const BOLD: &str = "bold";

pub fn alpha(c: Color32, a: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), (a.clamp(0.0, 1.0) * 255.0) as u8)
}

pub fn lerp(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgb(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()))
}

/// `color` con esa opacidad sobre negro, opaco. Sirve para mezclarlo con `lerp`, que ignora el alfa.
pub fn alpha_on(color: Color32, a: f32) -> Color32 {
    lerp(Color32::BLACK, color, a)
}

/// Color del degradado de marca en `t` (0..1).
pub fn brand_gradient(t: f32) -> Color32 {
    if t < 0.5 { lerp(BRAND_A, BRAND_B, t * 2.0) } else { lerp(BRAND_B, BRAND_C, (t - 0.5) * 2.0) }
}

pub fn semibold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(SEMIBOLD.into()))
}

pub fn bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name(BOLD.into()))
}

/// Usa Segoe UI de Windows (se lee de C:\Windows\Fonts, no se incluye en el exe). Si falta,
/// quedan las fuentes de egui.
fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let dir = std::path::Path::new(&std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into())).join("Fonts");
    let mut load = |key: &str, file: &str| -> bool {
        match std::fs::read(dir.join(file)) {
            Ok(bytes) => {
                fonts.font_data.insert(key.into(), FontData::from_owned(bytes).into());
                true
            }
            Err(_) => false,
        }
    };
    let regular = load("segoe", "segoeui.ttf");
    let semi = load("segoe-sb", "seguisb.ttf");
    let bold = load("segoe-b", "segoeuib.ttf");
    let fallback: Vec<String> = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    let family = |main: Option<&str>| -> Vec<String> {
        let mut v: Vec<String> = main.map(|m| vec![m.to_string()]).unwrap_or_default();
        v.extend(fallback.iter().cloned());
        v
    };
    let base = regular.then_some("segoe");
    fonts.families.insert(FontFamily::Proportional, family(base));
    fonts.families.insert(FontFamily::Name(SEMIBOLD.into()), family(if semi { Some("segoe-sb") } else { base }));
    fonts.families.insert(FontFamily::Name(BOLD.into()), family(if bold { Some("segoe-b") } else { base }));
    ctx.set_fonts(fonts);
}

/// Pone los colores del tema pedido.
pub fn set_theme(ctx: &egui::Context, light: bool) {
    IS_LIGHT.store(light, Ordering::Relaxed);
    let p = c();
    let mut v = if light { Visuals::light() } else { Visuals::dark() };
    v.panel_fill = p.bg;
    v.window_fill = p.surface;
    v.extreme_bg_color = p.surface_2;
    v.faint_bg_color = p.surface;
    v.override_text_color = Some(p.text);
    v.hyperlink_color = p.accent;
    v.selection.bg_fill = alpha(BRAND_A, 0.35);
    v.selection.stroke = Stroke::new(1.0, p.accent);
    v.text_cursor.stroke = Stroke::new(1.5, p.accent);
    v.window_corner_radius = CornerRadius::same(14);
    v.window_stroke = Stroke::new(1.0, p.border);
    let hover = lerp(p.surface_2, p.text, 0.08);
    let active = lerp(p.surface_2, p.text, 0.14);
    for (w, fill) in [
        (&mut v.widgets.noninteractive, p.surface),
        (&mut v.widgets.inactive, p.surface_2),
        (&mut v.widgets.hovered, hover),
        (&mut v.widgets.active, active),
        (&mut v.widgets.open, p.surface_2),
    ] {
        w.bg_fill = fill;
        w.weak_bg_fill = fill;
        w.corner_radius = CornerRadius::same(10);
        w.bg_stroke = Stroke::new(1.0, p.border);
    }
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, p.text);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, p.text);
    v.widgets.hovered.fg_stroke = Stroke::new(1.0, p.text);
    v.widgets.active.fg_stroke = Stroke::new(1.0, p.text);
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, alpha(BRAND_A, 0.6));
    v.widgets.active.bg_stroke = Stroke::new(1.0, BRAND_A);
    let (preference, theme) = if light {
        (egui::ThemePreference::Light, egui::Theme::Light)
    } else {
        (egui::ThemePreference::Dark, egui::Theme::Dark)
    };
    ctx.set_theme(preference);
    ctx.set_visuals_of(theme, v);
}

pub fn apply(ctx: &egui::Context, light: bool) {
    install_fonts(ctx);
    set_theme(ctx, light);
    ctx.all_styles_mut(|s| {
        s.text_styles = [
            (TextStyle::Heading, semibold(22.0)),
            (TextStyle::Body, FontId::proportional(14.5)),
            (TextStyle::Button, FontId::proportional(14.5)),
            (TextStyle::Small, FontId::proportional(12.5)),
            (TextStyle::Monospace, FontId::monospace(13.0)),
        ]
        .into();
        s.spacing.item_spacing = egui::vec2(10.0, 10.0);
        s.spacing.button_padding = egui::vec2(14.0, 8.0);
        s.spacing.interact_size.y = 34.0;
        s.spacing.window_margin = Margin::same(16);
        s.interaction.selectable_labels = false;
    });
}
