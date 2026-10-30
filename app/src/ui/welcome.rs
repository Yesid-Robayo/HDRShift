//! Primera pantalla: elegir idioma y entender qué hace la app en tres frases.

use eframe::egui::{self, TextureHandle, vec2};
use hdrshift_core::config::Config;
use hdrshift_core::i18n::{self, Language, tr};

use super::theme::{self, c};
use super::widgets::{self, Glyph};
use super::{Nav, save_config};

pub struct Welcome {
    cfg: Config,
}

impl Welcome {
    pub fn new() -> Self {
        Self { cfg: Config::default() }
    }

    pub fn show(&mut self, ui: &mut egui::Ui, logo: &TextureHandle) -> Nav {
        let mut nav = Nav::Stay;
        ui.add_space(26.0);
        ui.vertical_centered(|ui| {
            ui.add(egui::Image::new(logo).fit_to_exact_size(vec2(84.0, 84.0)));
            ui.add_space(6.0);
            ui.label(egui::RichText::new("HDRShift").font(theme::bold(30.0)));
            widgets::muted(
                ui,
                tr(
                    "HDR automático para cada app, y todo vuelve como estaba.",
                    "Automatic HDR for each app, and everything goes back as it was.",
                ),
            );
        });
        ui.add_space(14.0);
        widgets::card(ui, |ui| {
            for (g, title, text) in [
                (
                    Glyph::Apps,
                    tr("Eliges tus apps", "Pick your apps"),
                    tr(
                        "Tus juegos o el reproductor que quieras ver en HDR.",
                        "Your games or the player you want in HDR.",
                    ),
                ),
                (
                    Glyph::Shortcut,
                    tr("Abres con su acceso directo", "Open them with their shortcut"),
                    tr("O deja que HDRShift las detecte solo.", "Or let HDRShift detect them on its own."),
                ),
                (
                    Glyph::Monitor,
                    tr("Al cerrar, vuelve como estaba", "When you close, it goes back"),
                    tr("Si ya tenías HDR encendido, no lo toca.", "If HDR was already on, it's left alone."),
                ),
            ] {
                ui.horizontal(|ui| {
                    let (rect, _) = ui.allocate_exact_size(vec2(28.0, 28.0), egui::Sense::hover());
                    widgets::glyph(ui.painter(), rect, g, c().accent);
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 1.0;
                        ui.label(egui::RichText::new(title).font(theme::semibold(14.5)));
                        widgets::muted(ui, text);
                    });
                });
            }
        });
        widgets::section_title(ui, tr("Idioma", "Language"));
        ui.horizontal(|ui| {
            for (language, label) in [(Language::Spanish, "Español"), (Language::English, "English")] {
                if widgets::choice_chip(ui, label, self.cfg.language == language).clicked() {
                    self.cfg.language = language;
                    i18n::set(language);
                }
            }
        });
        ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
            if widgets::primary_button(ui, tr("Empezar", "Get started"), ui.available_width(), true).clicked()
                && save_config(&self.cfg)
            {
                nav = Nav::Home;
            }
        });
        nav
    }
}
