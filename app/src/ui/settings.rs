//! Ajustes generales y "Acerca de". Cada cambio se guarda al momento.

use std::time::Duration;

use eframe::egui::{self, TextureHandle};
use hdrshift_core::command::Command;
use hdrshift_core::config::{Config, ThemeChoice};
use hdrshift_core::i18n::{Language, tr};
use hdrshift_core::paths;
use hdrshift_core::{log, recovery, trf};

use super::theme::{self, c};
use super::{Nav, dialogs, editor, widgets};
use crate::ipc;

pub struct Settings {
    cfg: Config,
    original: Config,
    confirm_uninstall: bool,
    /// Se calcula una sola vez: `show` corre en cada fotograma y no debe tocar el disco.
    portable: bool,
    owner: isize,
    pick_link: Option<std::sync::mpsc::Receiver<Option<String>>>,
    message: Option<(String, bool)>,
    /// Hay ajustes recién guardados.
    pub changed: bool,
}

/// Carpeta del Escritorio del usuario.
fn desktop() -> String {
    std::env::var("USERPROFILE").map(|home| format!("{home}\\Desktop")).unwrap_or_default()
}

impl Settings {
    pub fn new(owner: isize) -> Self {
        let cfg = Config::load();
        Self {
            original: cfg.clone(),
            cfg,
            confirm_uninstall: false,
            portable: paths::is_portable(),
            owner,
            pick_link: None,
            message: None,
            changed: false,
        }
    }

    fn poll_shortcut(&mut self, ui: &egui::Ui) {
        let Some(rx) = &self.pick_link else { return };
        match rx.try_recv() {
            Ok(Some(path)) => {
                self.pick_link = None;
                let description = tr("Encender o apagar el HDR", "Turn HDR on or off");
                self.message = Some(match editor::write_shortcut(&path, "--toggle", description, None) {
                    Ok(()) => (tr("Acceso directo creado.", "Shortcut created.").into(), true),
                    Err(e) => {
                        log::error(&format!("no se pudo crear el acceso directo: {e}"));
                        (tr("No se pudo crear el acceso directo.", "Couldn't create the shortcut.").into(), false)
                    }
                });
            }
            Ok(None) | Err(std::sync::mpsc::TryRecvError::Disconnected) => self.pick_link = None,
            Err(std::sync::mpsc::TryRecvError::Empty) => ui.ctx().request_repaint_after(Duration::from_millis(100)),
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui, logo: &TextureHandle) -> Nav {
        if super::page_header(ui, tr("Ajustes", "Settings")) {
            return Nav::Home;
        }
        self.poll_shortcut(ui);
        egui::ScrollArea::vertical().id_salt("settings").auto_shrink([false, false]).show(ui, |ui| {
            self.look(ui);
            self.background(ui);
            self.shortcuts(ui);
            self.data(ui);
            self.about(ui, logo);
            self.exit(ui);
            ui.add_space(12.0);
        });
        self.apply(ui);
        Nav::Stay
    }

    /// Guarda lo que haya cambiado.
    fn apply(&mut self, ui: &egui::Ui) {
        if self.cfg == self.original {
            return;
        }
        if super::save_config(&self.cfg) {
            self.original = self.cfg.clone();
            self.changed = true;
            ui.ctx().request_repaint();
        }
    }

    fn look(&mut self, ui: &mut egui::Ui) {
        let cfg = &mut self.cfg;
        widgets::section_title(ui, tr("Apariencia", "Appearance"));
        widgets::card(ui, |ui| {
            ui.label(egui::RichText::new(tr("Tema", "Theme")).font(theme::semibold(14.5)));
            ui.horizontal_wrapped(|ui| {
                for (choice, label) in [
                    (ThemeChoice::System, tr("Como Windows", "Same as Windows")),
                    (ThemeChoice::Dark, tr("Oscuro", "Dark")),
                    (ThemeChoice::Light, tr("Claro", "Light")),
                ] {
                    if widgets::choice_chip(ui, label, cfg.theme == choice).clicked() {
                        cfg.theme = choice;
                    }
                }
            });
            ui.add_space(4.0);
            ui.label(egui::RichText::new(tr("Idioma", "Language")).font(theme::semibold(14.5)));
            ui.horizontal_wrapped(|ui| {
                for (language, label) in [(Language::Spanish, "Español"), (Language::English, "English")] {
                    if widgets::choice_chip(ui, label, cfg.language == language).clicked() {
                        cfg.language = language;
                    }
                }
            });
        });
    }

    fn background(&mut self, ui: &mut egui::Ui) {
        let cfg = &mut self.cfg;
        widgets::section_title(ui, tr("En segundo plano", "In the background"));
        widgets::card(ui, |ui| {
            widgets::setting_row(
                ui,
                tr("Quedarse en la bandeja", "Stay in the tray"),
                tr(
                    "El interruptor siempre a mano. Apagado, HDRShift no deja ningún proceso sin apps abiertas, salvo que una tenga Encender solo o actives Iniciar con Windows.",
                    "The switch always at hand. Off, HDRShift leaves no process running with no apps open, unless one has Turn on by itself or Start with Windows is on.",
                ),
                |ui| {
                    widgets::toggle(ui, &mut cfg.stay_in_tray);
                },
            );
            widgets::setting_row(
                ui,
                tr("Iniciar con Windows", "Start with Windows"),
                tr(
                    "Arranca en la bandeja, sin abrir la ventana. Si una app tiene Encender solo, arranca igualmente.",
                    "Starts in the tray, without opening the window. If an app has Turn on by itself, it starts anyway.",
                ),
                |ui| {
                    widgets::toggle(ui, &mut cfg.start_with_windows);
                },
            );
        });
    }

    fn shortcuts(&mut self, ui: &mut egui::Ui) {
        widgets::section_title(ui, tr("Interruptor", "Switch"));
        widgets::card(ui, |ui| {
            widgets::muted(
                ui,
                tr(
                    "Un acceso directo que enciende o apaga el HDR de un clic, sin abrir esta ventana.",
                    "A shortcut that turns HDR on or off in one click, without opening this window.",
                ),
            );
            if widgets::secondary_button(
                ui,
                tr("Crear acceso directo del interruptor", "Create the switch shortcut"),
                ui.available_width(),
            )
            .clicked()
                && self.pick_link.is_none()
            {
                self.pick_link = Some(dialogs::save_shortcut(
                    self.owner,
                    tr("Guardar acceso directo", "Save shortcut"),
                    tr("Acceso directo (*.lnk)", "Shortcut (*.lnk)"),
                    desktop(),
                    "HDR.lnk".into(),
                ));
            }
            if let Some((text, ok)) = &self.message {
                ui.label(egui::RichText::new(text).size(13.0).color(if *ok { c().good } else { c().low }));
            }
        });
    }

    fn data(&mut self, ui: &mut egui::Ui) {
        widgets::section_title(ui, tr("Datos y diagnóstico", "Data and diagnostics"));
        widgets::card(ui, |ui| {
            ui.horizontal(|ui| {
                let w = (ui.available_width() - 10.0) / 2.0;
                if widgets::secondary_button(ui, tr("Abrir carpeta de datos", "Open data folder"), w).clicked() {
                    ipc::open_external(&paths::data_dir().display().to_string());
                }
                if widgets::secondary_button(ui, tr("Abrir el log", "Open the log"), w).clicked() {
                    ipc::open_external(&paths::log_file().display().to_string());
                }
            });
            widgets::muted(
                ui,
                tr(
                    "Si algo falla, adjunta hdrshift.log en un issue de GitHub.",
                    "If something fails, attach hdrshift.log to a GitHub issue.",
                ),
            );
            if self.portable {
                widgets::muted(
                    ui,
                    tr("Portable: tus datos están junto al exe.", "Portable: your data is next to the exe."),
                );
            } else if widgets::secondary_button(
                ui,
                tr("Guardar los datos junto al exe (portable)", "Keep the data next to the exe (portable)"),
                ui.available_width(),
            )
            .clicked()
            {
                move_to_portable(ui.ctx());
            }
        });
    }

    fn about(&mut self, ui: &mut egui::Ui, logo: &TextureHandle) {
        widgets::section_title(ui, tr("Acerca de", "About"));
        widgets::card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add(egui::Image::new(logo).fit_to_exact_size(egui::vec2(44.0, 44.0)));
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ui.label(egui::RichText::new("HDRShift").font(theme::bold(17.0)));
                    widgets::muted(
                        ui,
                        trf!("Versión {} · Licencia MIT", "Version {} · MIT license", hdrshift_core::VERSION),
                    );
                });
            });
            ui.add_space(4.0);
            widgets::muted(
                ui,
                tr(
                    "Solo cambia el ajuste de HDR de Windows, el mismo de Configuración. No se conecta a internet ni pide permisos de administrador.",
                    "It only changes the Windows HDR setting, the same one as in Settings. It doesn't go online or ask for administrator rights.",
                ),
            );
            ui.horizontal(|ui| {
                if ui.link(tr("Buscar actualización", "Check for updates")).clicked() {
                    ipc::open_external(&format!("{}/releases/latest", hdrshift_core::REPO_URL));
                }
                if ui.link("GitHub").clicked() {
                    ipc::open_external(hdrshift_core::REPO_URL);
                }
                if ui.link(tr("Apoyar en Ko-fi", "Support on Ko-fi")).clicked() {
                    ipc::open_external(hdrshift_core::KOFI_URL);
                }
            });
        });
    }

    fn exit(&mut self, ui: &mut egui::Ui) {
        ui.add_space(8.0);
        if ipc::service_running()
            && widgets::danger_button(
                ui,
                tr("Cerrar HDRShift por completo", "Quit HDRShift completely"),
                ui.available_width(),
            )
            .clicked()
        {
            ipc::send(&Command::Quit);
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if !self.confirm_uninstall {
            if widgets::secondary_button(ui, tr("Desinstalar…", "Uninstall…"), ui.available_width()).clicked() {
                self.confirm_uninstall = true;
            }
            return;
        }
        widgets::card(ui, |ui| {
            ui.label(
                egui::RichText::new(tr("¿Desinstalar HDRShift?", "Uninstall HDRShift?")).font(theme::semibold(14.5)),
            );
            widgets::muted(
                ui,
                tr(
                    "Deja el HDR como estaba, quita el inicio con Windows y cierra la app. El exe lo borras tú.",
                    "It puts HDR back as it was, removes the start with Windows and closes the app. You delete the exe yourself.",
                ),
            );
            if widgets::secondary_button(
                ui,
                tr("Desinstalar y conservar mis datos", "Uninstall and keep my data"),
                ui.available_width(),
            )
            .clicked()
            {
                uninstall(ui, false);
            }
            if widgets::danger_button(
                ui,
                tr("Desinstalar y borrar mis datos", "Uninstall and delete my data"),
                ui.available_width(),
            )
            .clicked()
            {
                uninstall(ui, true);
            }
            if widgets::secondary_button(ui, tr("Cancelar", "Cancel"), ui.available_width()).clicked() {
                self.confirm_uninstall = false;
            }
        });
    }
}

/// Copia los ajustes a la carpeta portable. Desde ese momento la app usa esa carpeta.
fn make_portable() -> std::io::Result<()> {
    // antes de crear la carpeta: en cuanto existe, `paths` ya apunta a ella
    let (config, ledger) = (paths::config_file(), paths::ledger_file());
    let target = paths::portable_dir().ok_or(std::io::ErrorKind::NotFound)?;
    std::fs::create_dir_all(&target)?;
    if config.exists() {
        std::fs::copy(&config, target.join("config.json"))?;
    }
    // lo pendiente solo debe estar en un sitio
    if ledger.exists() {
        std::fs::copy(&ledger, target.join("ledger.json"))?;
        std::fs::remove_file(&ledger)?;
    }
    Ok(())
}

/// Cierra el servicio, que deja el HDR como estaba, pasa a modo portable y reabre la ventana.
pub fn move_to_portable(ctx: &egui::Context) {
    ipc::send(&Command::Quit);
    std::thread::sleep(Duration::from_secs(1));
    match make_portable() {
        Ok(()) => {
            ipc::spawn_self("--reopen");
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        Err(e) => log::error(&format!("no se pudo pasar a portable: {e}")),
    }
}

fn uninstall(ui: &egui::Ui, delete_data: bool) {
    let mut cfg = Config::load();
    cfg.start_with_windows = false;
    cfg.stay_in_tray = false;
    for app in &mut cfg.apps {
        app.auto = false;
    }
    let _ = cfg.save();
    crate::autostart::apply(false);
    ipc::send(&Command::Quit);
    // el servicio apaga el HDR que encendió y escribe sus últimos datos al salir
    std::thread::sleep(Duration::from_secs(2));
    if delete_data {
        // sin el registro de deudas ya no se puede recuperar nada: la entrada de inicio sobraría
        recovery::disarm();
        paths::remove_data();
    }
    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
}
