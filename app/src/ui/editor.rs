//! Añadir o editar una app.

use std::sync::mpsc::{Receiver, TryRecvError};
use std::time::Duration;

use eframe::egui::{self, Sense, vec2};
use hdrshift_core::command::Command;
use hdrshift_core::config::{AppEntry, Config, new_id};
use hdrshift_core::display::{Display, resolve};
use hdrshift_core::i18n::tr;
use hdrshift_core::shortcut::{self, Link};
use hdrshift_core::{log, trf};

use super::theme::{self, c};
use super::widgets;
use super::{Nav, dialogs, page_header, save_config};
use crate::ipc;

type Pick = Receiver<Option<String>>;

pub struct Editor {
    entry: AppEntry,
    is_new: bool,
    owner: isize,
    pick_exe: Option<Pick>,
    pick_link: Option<Pick>,
    advanced: bool,
    confirm_delete: bool,
    error: Option<String>,
    done: Option<String>,
}

fn file_stem(path: &str) -> String {
    std::path::Path::new(path).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default()
}

fn folder_of(path: &str) -> String {
    std::path::Path::new(path).parent().map(|p| p.display().to_string()).unwrap_or_default()
}

/// Sin los caracteres que Windows no acepta en un nombre de archivo.
fn safe_file_name(name: &str) -> String {
    let clean: String =
        name.chars().map(|c| if r#"<>:"/\|?*"#.contains(c) || c.is_control() { '_' } else { c }).collect();
    let clean = clean.trim().trim_end_matches('.').to_string();
    if clean.is_empty() { "HDRShift".into() } else { clean }
}

/// Espera sin bloquear a la ventana de archivos.
fn poll(slot: &mut Option<Pick>, ui: &egui::Ui) -> Option<String> {
    let rx = slot.as_ref()?;
    match rx.try_recv() {
        Ok(path) => {
            *slot = None;
            path
        }
        Err(TryRecvError::Disconnected) => {
            *slot = None;
            None
        }
        Err(TryRecvError::Empty) => {
            ui.ctx().request_repaint_after(Duration::from_millis(100));
            None
        }
    }
}

/// Crea un acceso directo a este mismo exe.
pub fn write_shortcut(path: &str, args: &str, description: &str, icon: Option<&str>) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let work_dir = exe.parent().map(|p| p.to_path_buf()).unwrap_or_default();
    let icon = icon.filter(|i| std::path::Path::new(i).exists()).map(std::path::PathBuf::from).unwrap_or(exe.clone());
    shortcut::create(&Link {
        path: std::path::Path::new(path),
        target: &exe,
        args,
        work_dir: &work_dir,
        description,
        icon: &icon,
    })
}

impl Editor {
    pub fn new(entry: Option<&AppEntry>, monitors: &[Display], owner: isize) -> Self {
        let is_new = entry.is_none();
        let entry = entry.cloned().unwrap_or_else(|| AppEntry {
            id: new_id(),
            // lo normal es querer HDR en todos los monitores que lo admiten
            monitors: monitors.iter().filter(|d| d.supports_hdr).map(|d| d.id.clone()).collect(),
            ..Default::default()
        });
        Self {
            advanced: !entry.args.is_empty() || !entry.watch.is_empty(),
            entry,
            is_new,
            owner,
            pick_exe: None,
            pick_link: None,
            confirm_delete: false,
            error: None,
            done: None,
        }
    }

    fn validate(&self, cfg: &Config) -> Option<String> {
        let e = &self.entry;
        if e.name.trim().is_empty() {
            return Some(tr("Ponle un nombre.", "Give it a name.").into());
        }
        if !std::path::Path::new(e.exe.trim()).is_file() {
            return Some(tr("Elige un programa que exista.", "Pick a program that exists.").into());
        }
        // dos apps con el mismo exe se pisarían al detectarlas y al terminar su sesión
        if cfg.apps.iter().any(|a| a.id != e.id && a.exe.eq_ignore_ascii_case(e.exe.trim())) {
            return Some(tr("Ese programa ya está en la lista.", "That program is already on the list.").into());
        }
        let own = std::env::current_exe().ok().map(|p| p.display().to_string().to_lowercase());
        if own.as_deref() == Some(e.exe.trim().to_lowercase().as_str()) {
            return Some(
                tr(
                    "HDRShift no puede abrirse a sí mismo: usa el interruptor de arriba.",
                    "HDRShift can't open itself: use the switch at the top.",
                )
                .into(),
            );
        }
        None
    }

    fn save(&mut self, cfg: &mut Config) -> bool {
        self.entry.exe = self.entry.exe.trim().to_string();
        self.entry.name = self.entry.name.trim().to_string();
        match cfg.apps.iter_mut().find(|a| a.id == self.entry.id) {
            Some(a) => *a = self.entry.clone(),
            None => cfg.apps.push(self.entry.clone()),
        }
        self.is_new = false;
        save_config(cfg)
    }

    fn create_shortcut(&mut self, path: &str) {
        let args = format!("--launch {}", self.entry.id);
        let description = trf!("Abrir {} con HDR", "Open {} with HDR", self.entry.name);
        match write_shortcut(path, &args, &description, Some(&self.entry.exe)) {
            Ok(()) => {
                self.error = None;
                self.done = Some(tr("Acceso directo creado.", "Shortcut created.").into());
            }
            Err(e) => {
                log::error(&format!("no se pudo crear el acceso directo: {e}"));
                self.error = Some(tr("No se pudo crear el acceso directo.", "Couldn't create the shortcut.").into());
            }
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui, monitors: &[Display], cfg: &mut Config) -> Nav {
        let title = if self.is_new { tr("Añadir app", "Add app") } else { tr("Editar app", "Edit app") };
        if page_header(ui, title) {
            return Nav::Home;
        }
        if let Some(path) = poll(&mut self.pick_exe, ui) {
            if self.entry.name.trim().is_empty() {
                self.entry.name = file_stem(&path);
            }
            self.entry.exe = path;
            self.error = None;
        }
        if let Some(path) = poll(&mut self.pick_link, ui) {
            self.create_shortcut(&path);
        }

        let mut nav = Nav::Stay;
        let footer = 62.0;
        egui::ScrollArea::vertical().max_height(ui.available_height() - footer).auto_shrink([false, false]).show(ui, |ui| {
            widgets::card(ui, |ui| {
                widgets::field_label(ui, tr("Programa", "Program"));
                ui.horizontal(|ui| {
                    let width = ui.available_width() - 110.0;
                    widgets::text_field(ui, &mut self.entry.exe, width, 1024, "C:\\Games\\Game.exe");
                    if widgets::secondary_button(ui, tr("Examinar", "Browse"), 100.0).clicked() && self.pick_exe.is_none() {
                        self.pick_exe = Some(dialogs::pick_exe(
                            self.owner,
                            tr("Elige el programa", "Pick the program"),
                            tr("Programas (*.exe)", "Programs (*.exe)"),
                            folder_of(&self.entry.exe),
                        ));
                    }
                });
                ui.add_space(4.0);
                widgets::field_label(ui, tr("Nombre", "Name"));
                widgets::text_field(ui, &mut self.entry.name, f32::INFINITY, 48, "");
            });

            widgets::section_title(ui, tr("Encender el HDR en", "Turn HDR on in"));
            widgets::card(ui, |ui| {
                if monitors.is_empty() {
                    widgets::muted(ui, tr("No se detectó ningún monitor.", "No monitor detected."));
                }
                for d in monitors {
                    let chosen = |m: &String| resolve(monitors, m).is_some_and(|x| x.id == d.id);
                    let selected = self.entry.monitors.iter().any(chosen);
                    let mut on = selected;
                    let desc = if d.supports_hdr { "" } else { tr("No admite HDR", "Doesn't support HDR") };
                    widgets::setting_row(ui, &d.name, desc, |ui| {
                        if d.supports_hdr {
                            widgets::toggle(ui, &mut on);
                        }
                    });
                    if on != selected {
                        self.entry.monitors.retain(|m| !chosen(m));
                        if on {
                            self.entry.monitors.push(d.id.clone());
                        }
                    }
                }
            });

            let label = if self.advanced { tr("Ocultar opciones avanzadas", "Hide advanced options") } else { tr("Opciones avanzadas", "Advanced options") };
            if ui.link(label).clicked() {
                self.advanced = !self.advanced;
            }
            if self.advanced {
                widgets::card(ui, |ui| {
                    widgets::field_label(ui, tr("Argumentos", "Arguments"));
                    widgets::text_field(ui, &mut self.entry.args, f32::INFINITY, 1024, "");
                    ui.add_space(4.0);
                    widgets::field_label(ui, tr("Otro proceso que cuenta como esta app", "Another process that counts as this app"));
                    widgets::text_field(ui, &mut self.entry.watch, f32::INFINITY, 128, "game.exe");
                    widgets::muted(ui, tr(
                        "Solo hace falta si su lanzador abre el juego desde otra carpeta y el HDR se apaga antes de tiempo.",
                        "Only needed if its launcher opens the game from another folder and HDR turns off too soon.",
                    ));
                });
            }

            if !self.is_new {
                widgets::section_title(ui, tr("Más", "More"));
                ui.horizontal(|ui| {
                    let w = (ui.available_width() - 10.0) / 2.0;
                    if widgets::secondary_button(ui, tr("Crear acceso directo", "Create shortcut"), w).clicked() && self.pick_link.is_none() {
                        self.pick_link = Some(dialogs::save_shortcut(
                            self.owner,
                            tr("Guardar acceso directo", "Save shortcut"),
                            tr("Acceso directo (*.lnk)", "Shortcut (*.lnk)"),
                            folder_of(&self.entry.exe),
                            format!("{} (HDR).lnk", safe_file_name(&self.entry.name)),
                        ));
                    }
                    if !self.confirm_delete && widgets::danger_button(ui, tr("Quitar de la lista", "Remove from the list"), w).clicked() {
                        self.confirm_delete = true;
                    }
                });
                if self.confirm_delete {
                    widgets::card(ui, |ui| {
                        let name = &self.entry.name;
                        ui.label(egui::RichText::new(trf!("¿Quitar {name}?", "Remove {name}?")).font(theme::semibold(14.5)));
                        widgets::muted(ui, tr(
                            "El programa no se borra. Sus accesos directos dejarán de funcionar.",
                            "The program isn't deleted. Its shortcuts will stop working.",
                        ));
                        ui.horizontal(|ui| {
                            let w = (ui.available_width() - 10.0) / 2.0;
                            if widgets::danger_button(ui, tr("Quitar", "Remove"), w).clicked() {
                                ipc::send(&Command::End(self.entry.id.clone()));
                                cfg.apps.retain(|a| a.id != self.entry.id);
                                if save_config(cfg) {
                                    nav = Nav::Home;
                                }
                            }
                            if widgets::secondary_button(ui, tr("Cancelar", "Cancel"), w).clicked() {
                                self.confirm_delete = false;
                            }
                        });
                    });
                }
            }
            ui.add_space(8.0);
        });

        ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
            if widgets::primary_button(ui, tr("Guardar", "Save"), ui.available_width(), true).clicked() {
                self.error = self.validate(cfg);
                self.done = None;
                if self.error.is_none() && self.save(cfg) {
                    nav = Nav::Home;
                }
            }
            if let Some(text) = &self.error {
                ui.label(egui::RichText::new(text).size(13.0).color(c().low));
            } else if let Some(text) = &self.done {
                ui.horizontal(|ui| {
                    let (rect, _) = ui.allocate_exact_size(vec2(16.0, 16.0), Sense::hover());
                    widgets::glyph(ui.painter(), rect, widgets::Glyph::Check, c().good);
                    ui.label(egui::RichText::new(text).size(13.0).color(c().good));
                });
            }
        });
        nav
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcut_names_have_no_forbidden_characters() {
        assert_eq!(safe_file_name("Juego: Edición \"Oro\"?"), "Juego_ Edición _Oro__");
        assert_eq!(safe_file_name("..."), "HDRShift");
    }
}
