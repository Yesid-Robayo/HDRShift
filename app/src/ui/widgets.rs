//! Componentes dibujados a mano: disco de HDR, iconos vectoriales, interruptor, chips y botones.

use std::f32::consts::{PI, TAU};
use std::sync::Arc;

use eframe::egui::{
    self, Align2, Color32, CornerRadius, CursorIcon, Id, Mesh, Pos2, Rect, Response, Sense, Shape, Stroke, StrokeKind,
    Ui, pos2, vec2,
};

use super::theme::{self, BRAND_A, BRAND_B, ON_ACCENT, c};

fn arc_points(center: Pos2, r: f32, from: f32, to: f32) -> Vec<Pos2> {
    let n = (((to - from).abs() / TAU) * 96.0).ceil().max(2.0) as usize;
    (0..=n)
        .map(|i| {
            let a = from + (to - from) * i as f32 / n as f32;
            pos2(center.x + r * a.cos(), center.y + r * a.sin())
        })
        .collect()
}

/// El símbolo de la app: disco mitad lleno. `t` (0..1) hace aparecer los rayos.
pub fn range_disc(p: &egui::Painter, center: Pos2, r: f32, color: Color32, t: f32) {
    let width = (r * 0.16).max(1.5);
    p.circle_stroke(center, r - width / 2.0, Stroke::new(width, color));
    let half = arc_points(center, r - width / 2.0, -PI / 2.0, PI / 2.0);
    p.add(Shape::convex_polygon(half, color, Stroke::NONE));
    if t > 0.01 {
        for i in 0..8 {
            let a = i as f32 * TAU / 8.0;
            let d = vec2(a.cos(), a.sin());
            let from = center + d * (r * 1.32);
            let to = center + d * (r * (1.32 + 0.32 * t));
            p.line_segment([from, to], Stroke::new(width, theme::alpha(color, t)));
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Glyph {
    Settings,
    Back,
    Check,
    Monitor,
    Apps,
    Shortcut,
    Edit,
}

pub fn glyph(p: &egui::Painter, rect: Rect, g: Glyph, color: Color32) {
    let s = rect.width().min(rect.height());
    let c = rect.center();
    let st = Stroke::new((s * 0.1).max(1.4), color);
    match g {
        Glyph::Settings => {
            for i in 0..8 {
                let a = i as f32 * TAU / 8.0;
                let (x, y) = (a.cos(), a.sin());
                p.line_segment(
                    [pos2(c.x + x * 0.26 * s, c.y + y * 0.26 * s), pos2(c.x + x * 0.4 * s, c.y + y * 0.4 * s)],
                    Stroke::new(st.width * 1.5, color),
                );
            }
            p.circle_stroke(c, 0.25 * s, st);
            p.circle_stroke(c, 0.09 * s, st);
        }
        Glyph::Back => {
            p.add(Shape::line(
                vec![
                    pos2(c.x + 0.1 * s, c.y - 0.28 * s),
                    pos2(c.x - 0.18 * s, c.y),
                    pos2(c.x + 0.1 * s, c.y + 0.28 * s),
                ],
                st,
            ));
        }
        Glyph::Check => {
            p.add(Shape::line(
                vec![
                    pos2(c.x - 0.28 * s, c.y),
                    pos2(c.x - 0.07 * s, c.y + 0.22 * s),
                    pos2(c.x + 0.3 * s, c.y - 0.24 * s),
                ],
                st,
            ));
        }
        Glyph::Monitor => {
            let screen = Rect::from_center_size(pos2(c.x, c.y - 0.08 * s), vec2(0.8 * s, 0.52 * s));
            p.rect_stroke(screen, CornerRadius::same(2), st, StrokeKind::Middle);
            p.line_segment([pos2(c.x, screen.bottom()), pos2(c.x, c.y + 0.34 * s)], st);
            p.line_segment([pos2(c.x - 0.18 * s, c.y + 0.34 * s), pos2(c.x + 0.18 * s, c.y + 0.34 * s)], st);
        }
        Glyph::Apps => {
            for (x, y) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
                let cell = Rect::from_center_size(c + vec2(x, y) * 0.2 * s, vec2(0.28 * s, 0.28 * s));
                p.rect_stroke(cell, CornerRadius::same(2), st, StrokeKind::Middle);
            }
        }
        Glyph::Shortcut => {
            let tip = pos2(c.x + 0.32 * s, c.y - 0.32 * s);
            p.line_segment([pos2(c.x - 0.28 * s, c.y + 0.28 * s), tip], st);
            p.line_segment([pos2(c.x - 0.06 * s, tip.y), tip], st);
            p.line_segment([pos2(tip.x, c.y + 0.06 * s), tip], st);
        }
        Glyph::Edit => {
            // lápiz inclinado: cuerpo y punta
            let along = vec2(1.0, -1.0).normalized();
            let across = vec2(1.0, 1.0).normalized() * (0.13 * s);
            let (back, front) = (c + along * (-0.14 * s), c + along * (0.36 * s));
            let tip = c + along * (-0.42 * s);
            p.add(Shape::closed_line(vec![back + across, front + across, front - across, back - across, tip], st));
        }
    }
}

/// Botón redondo con un icono.
pub fn icon_button(ui: &mut Ui, g: Glyph, tooltip: &str) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(36.0, 36.0), Sense::click());
    let hover = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.12);
    let fill = theme::lerp(c().surface, c().surface_2, 0.4 + hover * 0.6);
    ui.painter().circle_filled(rect.center(), 18.0, fill);
    glyph(ui.painter(), rect.shrink(9.0), g, theme::lerp(c().muted, c().text, hover));
    resp.on_hover_cursor(CursorIcon::PointingHand).on_hover_text(tooltip)
}

/// Botón redondo con un icono, en un sitio fijo.
pub fn icon_button_at(ui: &Ui, rect: Rect, id: Id, g: Glyph, tooltip: &str) -> Response {
    let resp = ui.interact(rect, id, Sense::click());
    let hover = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.12);
    ui.painter().circle_filled(rect.center(), rect.width() / 2.0, theme::lerp(c().surface_2, c().border, hover));
    glyph(ui.painter(), rect.shrink(9.0), g, theme::lerp(c().muted, c().text, hover));
    resp.on_hover_cursor(CursorIcon::PointingHand).on_hover_text(tooltip)
}

/// Rectángulo redondeado con un degradado horizontal dado por `color_at(t)` (abanico de triángulos).
pub fn gradient_rect(p: &egui::Painter, rect: Rect, r: f32, color_at: impl Fn(f32) -> Color32) {
    let r = r.min(rect.height() / 2.0).min(rect.width() / 2.0);
    let corners = [
        (pos2(rect.right() - r, rect.top() + r), -PI / 2.0),
        (pos2(rect.right() - r, rect.bottom() - r), 0.0),
        (pos2(rect.left() + r, rect.bottom() - r), PI / 2.0),
        (pos2(rect.left() + r, rect.top() + r), PI),
    ];
    let mut outline = Vec::new();
    for (center, start) in corners {
        outline.extend(arc_points(center, r, start, start + PI / 2.0));
    }
    let at = |p: Pos2| color_at((p.x - rect.left()) / rect.width());
    let mut mesh = Mesh::default();
    mesh.colored_vertex(rect.center(), at(rect.center()));
    for point in &outline {
        mesh.colored_vertex(*point, at(*point));
    }
    let n = outline.len() as u32;
    for i in 0..n {
        mesh.add_triangle(0, 1 + i, 1 + (i + 1) % n);
    }
    p.add(Shape::mesh(mesh));
}

/// Botón principal (degradado de marca) a lo ancho de `width`.
pub fn primary_button(ui: &mut Ui, text: &str, width: f32, enabled: bool) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 46.0), if enabled { Sense::click() } else { Sense::hover() });
    let hover = ui.ctx().animate_bool_with_time(resp.id, resp.hovered() && enabled, 0.12);
    let p = ui.painter();
    if enabled {
        let (a, b) =
            (theme::lerp(BRAND_A, Color32::WHITE, hover * 0.1), theme::lerp(BRAND_B, Color32::WHITE, hover * 0.1));
        gradient_rect(p, rect, 13.0, |t| theme::lerp(a, b, t));
        p.text(rect.center(), Align2::CENTER_CENTER, text, theme::semibold(15.0), ON_ACCENT);
    } else {
        p.rect_filled(rect, CornerRadius::same(13), c().surface_2);
        p.text(rect.center(), Align2::CENTER_CENTER, text, theme::semibold(15.0), c().faint);
    }
    if enabled { resp.on_hover_cursor(CursorIcon::PointingHand) } else { resp }
}

/// Botón secundario (contorno).
pub fn secondary_button(ui: &mut Ui, text: &str, width: f32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 40.0), Sense::click());
    let hover = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.12);
    let p = ui.painter();
    p.rect(
        rect,
        CornerRadius::same(12),
        theme::lerp(c().surface, c().surface_2, hover),
        Stroke::new(1.0, theme::lerp(c().border, c().muted, hover)),
        StrokeKind::Inside,
    );
    p.text(rect.center(), Align2::CENTER_CENTER, text, theme::semibold(14.0), c().text);
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

/// Botón de texto peligroso (rojo).
pub fn danger_button(ui: &mut Ui, text: &str, width: f32) -> Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(width, 40.0), Sense::click());
    let hover = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.12);
    ui.painter().rect(
        rect,
        CornerRadius::same(12),
        theme::alpha(c().low, 0.08 + hover * 0.1),
        Stroke::new(1.0, theme::alpha(c().low, 0.5)),
        StrokeKind::Inside,
    );
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, text, theme::semibold(14.0), c().low);
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

/// Botón redondo de abrir, o de terminar si la app ya tiene su sesión.
pub fn play_button(ui: &Ui, rect: Rect, id: Id, running: bool, tooltip: &str) -> Response {
    let resp = ui.interact(rect, id, Sense::click());
    let hover = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.12);
    let p = ui.painter();
    let center = rect.center();
    let r = rect.width() / 2.0;
    if running {
        p.circle_filled(center, r, theme::lerp(c().surface_2, c().border, hover));
        let square = Rect::from_center_size(center, vec2(r * 0.66, r * 0.66));
        p.rect_filled(square, CornerRadius::same(2), c().text);
    } else {
        let light = |col| theme::lerp(col, Color32::WHITE, hover * 0.12);
        gradient_rect(p, rect, r, |t| theme::lerp(light(BRAND_A), light(BRAND_B), t));
        let h = r * 0.48;
        let triangle = vec![
            pos2(center.x - h * 0.7, center.y - h),
            pos2(center.x + h, center.y),
            pos2(center.x - h * 0.7, center.y + h),
        ];
        p.add(Shape::convex_polygon(triangle, ON_ACCENT, Stroke::NONE));
    }
    resp.on_hover_cursor(CursorIcon::PointingHand).on_hover_text(tooltip)
}

/// Píldora de estado.
pub fn chip_at(ui: &Ui, left_center: Pos2, text: &str, color: Color32) -> Rect {
    let galley = ui.painter().layout_no_wrap(text.to_string(), theme::semibold(12.0), color);
    let size = galley.size() + vec2(16.0, 6.0);
    let rect = Rect::from_min_size(pos2(left_center.x, left_center.y - size.y / 2.0), size);
    ui.painter().rect_filled(rect, CornerRadius::same(255), theme::alpha(color, 0.14));
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, color);
    rect
}

/// Chip seleccionable.
pub fn choice_chip(ui: &mut Ui, text: &str, selected: bool) -> Response {
    let font = theme::semibold(13.0);
    let color = if selected { ON_ACCENT } else { c().text };
    let galley = ui.painter().layout_no_wrap(text.to_string(), font, color);
    let size = galley.size() + vec2(24.0, 14.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let hover = ui.ctx().animate_bool_with_time(resp.id, resp.hovered(), 0.1);
    let fill = if selected { BRAND_A } else { theme::lerp(c().surface_2, c().border, hover) };
    ui.painter().rect_filled(rect, CornerRadius::same(255), fill);
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, color);
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

/// Interruptor tipo Windows 11 en un sitio fijo. `on_bright` es para fondos de color.
pub fn toggle_at(ui: &Ui, rect: Rect, id: Id, on: bool, enabled: bool, on_bright: bool) -> Response {
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let resp = ui.interact(rect, id, sense);
    let t = ui.ctx().animate_bool_with_time(resp.id, on, 0.15);
    let p = ui.painter();
    let (track_on, knob_on) = if on_bright { (Color32::WHITE, BRAND_B) } else { (BRAND_A, ON_ACCENT) };
    let fill = theme::lerp(c().surface_2, track_on, t);
    let edge = if on_bright { theme::alpha(Color32::WHITE, 0.0) } else { theme::lerp(c().muted, BRAND_A, t) };
    p.rect(rect, CornerRadius::same(255), fill, Stroke::new(1.0, edge), StrokeKind::Inside);
    let inset = rect.height() / 2.0;
    let x = egui::lerp((rect.left() + inset)..=(rect.right() - inset), t);
    p.circle_filled(pos2(x, rect.center().y), inset * 0.62, theme::lerp(c().text, knob_on, t));
    if enabled { resp.on_hover_cursor(CursorIcon::PointingHand) } else { resp }
}

/// Interruptor dentro del flujo normal. Su identidad no puede depender de la posición: al desplazar
/// heredaría la animación del que pasó antes por ese punto y parpadearía.
pub fn toggle(ui: &mut Ui, on: &mut bool) -> Response {
    let (rect, slot) = ui.allocate_exact_size(vec2(44.0, 24.0), Sense::hover());
    let mut resp = toggle_at(ui, rect, slot.id.with("toggle"), *on, true, false);
    if resp.clicked() {
        *on = !*on;
        resp.mark_changed();
    }
    resp
}

/// Fila de ajustes: título + descripción a la izquierda, control a la derecha.
pub fn setting_row(ui: &mut Ui, title: &str, desc: &str, control: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        let width = (ui.available_width() - 64.0).max(120.0);
        ui.vertical(|ui| {
            ui.set_max_width(width);
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.label(egui::RichText::new(title).font(theme::semibold(14.5)));
            if !desc.is_empty() {
                ui.label(egui::RichText::new(desc).size(12.5).color(c().muted));
            }
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), control);
    });
}

/// Contenedor tipo tarjeta.
pub fn card<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    egui::Frame::new()
        .fill(c().surface)
        .stroke(Stroke::new(1.0, c().border))
        .corner_radius(CornerRadius::same(16))
        .inner_margin(egui::Margin::same(16))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

/// Texto en una sola línea, recortado con puntos suspensivos si no cabe.
pub fn one_line(ui: &Ui, text: String, font: egui::FontId, color: Color32, width: f32) -> Arc<egui::Galley> {
    let mut job = egui::text::LayoutJob::simple_singleline(text, font, color);
    job.wrap = egui::text::TextWrapping::truncate_at_width(width.max(60.0));
    ui.painter().layout_job(job)
}

/// Texto pequeño y apagado.
pub fn muted(ui: &mut Ui, text: impl Into<String>) {
    ui.label(egui::RichText::new(text.into()).size(12.5).color(c().muted));
}

pub fn section_title(ui: &mut Ui, text: &str) {
    ui.add_space(6.0);
    ui.label(
        egui::RichText::new(text.to_uppercase()).font(theme::semibold(11.5)).color(c().faint).extra_letter_spacing(1.2),
    );
}

/// Etiqueta de un campo de formulario.
pub fn field_label(ui: &mut Ui, text: &str) {
    ui.label(egui::RichText::new(text).font(theme::semibold(13.5)));
}

/// Campo de texto de una línea.
pub fn text_field(ui: &mut Ui, value: &mut String, width: f32, limit: usize, hint: &str) -> Response {
    ui.add(
        egui::TextEdit::singleline(value)
            .desired_width(width)
            .char_limit(limit)
            .margin(egui::Margin::symmetric(10, 8))
            .hint_text(hint),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_toggle_keeps_its_identity_when_it_moves() {
        let ctx = egui::Context::default();
        let mut ids = Vec::new();
        for offset in [0.0, 30.0, 60.0] {
            let mut frame = ctx.run_ui(egui::RawInput::default(), |ui| {
                ui.add_space(offset);
                ids.push(toggle(ui, &mut true).id);
            });
            frame.textures_delta.clear();
        }
        assert!(ids.windows(2).all(|pair| pair[0] == pair[1]), "{ids:?}");
    }
}
