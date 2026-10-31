//! Ventana de HDRShift (proceso aparte del servicio: solo existe mientras está abierta).

#[cfg(debug_assertions)]
mod demo;
mod dialogs;
mod editor;
mod exe_icons;
mod settings;
mod theme;
mod welcome;
mod widgets;

use std::time::{Duration, Instant, SystemTime};

use eframe::egui::{
    self, Align2, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, StrokeKind, TextureHandle, pos2, vec2,
};
use hdrshift_core::command::Command;
use hdrshift_core::config::{AppEntry, Config};
use hdrshift_core::display::{Display, Displays, SystemDisplays, resolve};
use hdrshift_core::i18n::{self, tr};
use hdrshift_core::ledger::FileLedger;
use hdrshift_core::session::{Hold, HoldKind};
use hdrshift_core::status::Snapshot;
use hdrshift_core::{log, paths, trf};

use crate::ipc::{self, wide};
use theme::{BRAND_A, c};
use widgets::Glyph;

const WINDOW_TITLE: &str = "HDRShift";
const POLL: Duration = Duration::from_secs(1);
/// Justo después de una orden se mira más seguido, para que el cambio se vea enseguida.
const POLL_FAST: Duration = Duration::from_millis(150);
const FAST_FOR: Duration = Duration::from_secs(3);
/// Clase de las ventanas de winit: evita confundirse con una carpeta que se llame igual.
const WINDOW_CLASS: &str = "Window Class";
/// Tiempo que `--reopen` espera a que se cierre la ventana anterior.
const REOPEN_TRIES: u32 = 50;
const REOPEN_GAP: Duration = Duration::from_millis(100);

/// `reopening`: la ventana anterior se está cerrando, hay que esperarla en vez de traerla al frente.
pub fn run(reopening: bool) {
    unsafe {
        use windows_sys::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError};
        use windows_sys::Win32::System::Threading::CreateMutexW;
        use windows_sys::Win32::UI::WindowsAndMessaging::{FindWindowW, SW_RESTORE, SetForegroundWindow, ShowWindow};
        let name = wide("Local\\HDRShift.Window");
        let mut mutex = CreateMutexW(std::ptr::null(), 1, name.as_ptr());
        let mut taken = GetLastError() == ERROR_ALREADY_EXISTS;
        for _ in 0..REOPEN_TRIES {
            if !(taken && reopening) {
                break;
            }
            CloseHandle(mutex);
            std::thread::sleep(REOPEN_GAP);
            mutex = CreateMutexW(std::ptr::null(), 1, name.as_ptr());
            taken = GetLastError() == ERROR_ALREADY_EXISTS;
        }
        if taken {
            // ya hay una ventana: traerla al frente
            let h = FindWindowW(wide(WINDOW_CLASS).as_ptr(), wide(WINDOW_TITLE).as_ptr());
            if !h.is_null() {
                ShowWindow(h, SW_RESTORE);
                SetForegroundWindow(h);
            }
            return;
        }
        log::init("ventana");
        let cfg = Config::load();
        i18n::set(cfg.language);
        let icon = crate::icon::app_icon(64);
        let options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_title(WINDOW_TITLE)
                .with_inner_size([470.0, 660.0])
                .with_min_inner_size([420.0, 560.0])
                .with_icon(egui::IconData { rgba: icon.rgba8(), width: 64, height: 64 }),
            centered: true,
            ..Default::default()
        };
        if let Err(e) = eframe::run_native(WINDOW_TITLE, options, Box::new(|cc| Ok(Box::new(App::new(cc))))) {
            log::error(&format!("la ventana no pudo arrancar: {e}"));
        }
    }
}

fn window_handle(cc: &eframe::CreationContext<'_>) -> isize {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    match cc.window_handle().map(|h| h.as_raw()) {
        Ok(RawWindowHandle::Win32(h)) => h.hwnd.get(),
        _ => 0,
    }
}

/// Barra de título del mismo color que el fondo (Windows 11).
fn style_title_bar(hwnd: isize) {
    use windows_sys::Win32::Graphics::Dwm::DwmSetWindowAttribute;
    if hwnd == 0 {
        return;
    }
    let colorref = |c: Color32| (c.r() as u32) | ((c.g() as u32) << 8) | ((c.b() as u32) << 16);
    let hwnd = hwnd as *mut core::ffi::c_void;
    unsafe {
        let dark: i32 = (!c().light) as i32;
        DwmSetWindowAttribute(hwnd, 20, &dark as *const i32 as *const _, 4); // DWMWA_USE_IMMERSIVE_DARK_MODE
        let caption = colorref(c().bg);
        DwmSetWindowAttribute(hwnd, 35, &caption as *const u32 as *const _, 4); // DWMWA_CAPTION_COLOR
        let border = colorref(c().border);
        DwmSetWindowAttribute(hwnd, 34, &border as *const u32 as *const _, 4); // DWMWA_BORDER_COLOR
    }
}

enum Page {
    Welcome(Box<welcome::Welcome>),
    Home,
    Edit(Box<editor::Editor>),
    Settings(Box<settings::Settings>),
}

pub enum Nav {
    Stay,
    Home,
}

struct App {
    page: Page,
    cfg: Config,
    monitors: Vec<Display>,
    snapshot: Snapshot,
    snap_mtime: Option<SystemTime>,
    cfg_mtime: Option<SystemTime>,
    last_check: Instant,
    fast_until: Option<Instant>,
    logo: TextureHandle,
    icons: exe_icons::Cache,
    hwnd: isize,
    light: bool,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let cfg = Config::load();
        let light = theme::wants_light(cfg.theme);
        theme::apply(&cc.egui_ctx, light);
        let hwnd = window_handle(cc);
        style_title_bar(hwnd);
        let icon = crate::icon::app_icon(128);
        let logo = cc.egui_ctx.load_texture(
            "logo",
            egui::ColorImage::from_rgba_unmultiplied([128, 128], &icon.rgba8()),
            egui::TextureOptions::LINEAR,
        );
        // si algo quedó sin restaurar, el servicio lo arregla sin esperar al próximo inicio de sesión,
        // y respeta las apps que sigan abiertas
        // si pidió quedarse en la bandeja también tiene que estar, aunque no arranque con Windows
        // las pantallas de ejemplo no deben dejar un servicio en marcha
        let demo = cfg!(debug_assertions) && std::env::var_os("HDRSHIFT_DEMO").is_some();
        if !demo && (FileLedger::exists() || cfg.needs_tray()) && !ipc::service_running() {
            ipc::spawn_self("--service");
        }
        #[cfg_attr(not(debug_assertions), allow(unused_mut))]
        let mut page = if Config::exists() { Page::Home } else { Page::Welcome(Box::new(welcome::Welcome::new())) };
        // Solo en desarrollo: abrir una pantalla concreta con datos de ejemplo para revisarla.
        #[cfg(debug_assertions)]
        if let Some(p) = demo::page(hwnd) {
            page = p;
        }
        let mut app = Self {
            page,
            cfg,
            monitors: Vec::new(),
            snapshot: Snapshot::default(),
            snap_mtime: None,
            cfg_mtime: None,
            last_check: Instant::now() - Duration::from_secs(10),
            fast_until: None,
            logo,
            icons: exe_icons::Cache::default(),
            hwnd,
            light,
        };
        app.poll(&cc.egui_ctx);
        app
    }

    /// Relee los monitores, el estado del servicio y el tema de Windows.
    fn poll(&mut self, ctx: &egui::Context) {
        #[cfg(debug_assertions)]
        if demo::fill(self) {
            return;
        }
        let fast = self.fast_until.is_some_and(|t| t > Instant::now());
        if self.last_check.elapsed() < if fast { POLL_FAST } else { POLL } {
            return;
        }
        self.last_check = Instant::now();
        match SystemDisplays.list() {
            Ok(list) => self.monitors = list,
            Err(code) => log::warn(&format!("no se pudieron leer los monitores: código {code}")),
        }
        let mtime = std::fs::metadata(paths::status_file()).and_then(|m| m.modified()).ok();
        if mtime != self.snap_mtime {
            self.snap_mtime = mtime;
            self.snapshot = Snapshot::load().unwrap_or_default();
        }
        let saved = std::fs::metadata(paths::config_file()).and_then(|m| m.modified()).ok();
        if saved != self.cfg_mtime && !matches!(self.page, Page::Edit(_)) {
            self.cfg_mtime = saved;
            self.cfg = Config::load();
        }
        let light = theme::wants_light(self.cfg.theme);
        if light != self.light {
            self.light = light;
            theme::set_theme(ctx, light);
            style_title_bar(self.hwnd);
        }
    }

    /// Da una orden al servicio y mira el resultado enseguida.
    fn order(&mut self, command: &Command) {
        self.fast_until = Some(Instant::now() + FAST_FOR);
        if !ipc::command(command) {
            log::error("no se pudo dar la orden");
        }
    }

    fn holds_of<'a>(&'a self, monitor: &'a str) -> impl Iterator<Item = &'a Hold> {
        self.snapshot.holds.iter().filter(move |h| h.monitors.contains(monitor))
    }

    fn running(&self, id: &str) -> bool {
        self.snapshot.holds.iter().any(|h| matches!(&h.kind, HoldKind::App(a) if a == id))
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.poll(&ctx);
        #[cfg(debug_assertions)]
        demo::capture(&ctx);
        let mut next: Option<Page> = None;
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(c().bg).inner_margin(egui::Margin::symmetric(20, 16)))
            .show(ui, |ui| match &mut self.page {
                Page::Home => next = home(ui, self),
                Page::Welcome(w) => {
                    if let Nav::Home = w.show(ui, &self.logo) {
                        next = Some(Page::Home);
                    }
                }
                Page::Edit(e) => {
                    if let Nav::Home = e.show(ui, &self.monitors, &mut self.cfg, &mut self.icons) {
                        next = Some(Page::Home);
                    }
                }
                Page::Settings(s) => {
                    if let Nav::Home = s.show(ui, &self.logo) {
                        next = Some(Page::Home);
                    }
                    // que el tema nuevo se vea al momento
                    if std::mem::take(&mut s.changed) {
                        self.last_check = Instant::now() - Duration::from_secs(10);
                    }
                }
            });
        if let Some(p) = next {
            if matches!(p, Page::Home) {
                self.cfg = Config::load();
                self.last_check = Instant::now() - Duration::from_secs(10);
            }
            self.page = p;
        }
        ctx.request_repaint_after(if self.fast_until.is_some_and(|t| t > Instant::now()) { POLL_FAST } else { POLL });
    }
}

fn header(ui: &mut egui::Ui, logo: &TextureHandle, title: &str, subtitle: &str) {
    ui.horizontal(|ui| {
        ui.add(egui::Image::new(logo).fit_to_exact_size(vec2(38.0, 38.0)));
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.label(egui::RichText::new(title).font(theme::bold(21.0)));
            ui.label(egui::RichText::new(subtitle).size(12.5).color(c().muted));
        });
    });
}

/// Duración legible de una sesión.
fn elapsed(started: u64) -> String {
    let s = (hdrshift_core::now_secs() as u64).saturating_sub(started);
    match s {
        0..=59 => tr("ahora mismo", "just now").into(),
        60..=3599 => format!("{} min", s / 60),
        _ => format!("{} h {} min", s / 3600, (s % 3600) / 60),
    }
}

fn home(ui: &mut egui::Ui, app: &mut App) -> Option<Page> {
    let mut next = None;
    ui.horizontal(|ui| {
        let active = app.snapshot.holds.iter().filter(|h| matches!(h.kind, HoldKind::App(_))).count();
        let n = app.cfg.apps.len();
        let sub = match (active, n) {
            (0, 0) => tr("Sin apps todavía", "No apps yet").to_string(),
            (0, 1) => tr("1 app", "1 app").to_string(),
            (0, _) => trf!("{n} apps", "{n} apps"),
            (1, _) => tr("1 app con HDR ahora", "1 app using HDR now").to_string(),
            (_, _) => trf!("{active} apps con HDR ahora", "{active} apps using HDR now"),
        };
        header(ui, &app.logo, "HDRShift", &sub);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if widgets::icon_button(ui, Glyph::Settings, tr("Ajustes", "Settings")).clicked() {
                next = Some(Page::Settings(Box::new(settings::Settings::new(app.hwnd))));
            }
        });
    });
    ui.add_space(8.0);

    if app.monitors.iter().all(|d| !d.supports_hdr) {
        no_hdr_card(ui, app.monitors.is_empty());
    }
    let monitors = app.monitors.clone();
    for d in monitors.iter().filter(|d| d.supports_hdr) {
        hero(ui, app, d);
    }
    if !app.snapshot.pending.is_empty() {
        widgets::card(ui, |ui| {
            ui.label(
                egui::RichText::new(tr("HDR por restaurar", "HDR left to restore"))
                    .font(theme::semibold(14.5))
                    .color(c().warn),
            );
            widgets::muted(
                ui,
                tr(
                    "Windows no dejó apagarlo o el monitor no está disponible. HDRShift lo seguirá intentando. Si ese monitor ya no existe, olvídalo.",
                    "Windows didn't allow it or the monitor isn't available. HDRShift will keep trying. If that monitor is gone, forget it.",
                ),
            );
            if widgets::secondary_button(ui, tr("Olvidar", "Forget"), ui.available_width()).clicked() {
                app.order(&Command::Forget);
            }
        });
    }

    widgets::section_title(ui, tr("Tus apps", "Your apps"));
    let footer = 56.0;
    let apps = app.cfg.apps.clone();
    egui::ScrollArea::vertical()
        .id_salt("home")
        .max_height(ui.available_height() - footer)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if apps.is_empty() {
                empty_state(ui);
            }
            // las que tienen sesión van primero
            let mut ordered: Vec<&AppEntry> = apps.iter().collect();
            ordered.sort_by_key(|a| !app.running(&a.id));
            for entry in ordered {
                match app_card(ui, app, entry) {
                    CardAction::Edit => {
                        next = Some(Page::Edit(Box::new(editor::Editor::new(Some(entry), &app.monitors, app.hwnd))))
                    }
                    CardAction::Open => app.order(&Command::Launch(entry.id.clone())),
                    CardAction::End => app.order(&Command::End(entry.id.clone())),
                    CardAction::Auto(on) => set_app_auto(app, &entry.id, on),
                    CardAction::None => {}
                }
            }
        });

    ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
        if widgets::primary_button(ui, tr("+  Añadir app", "+  Add app"), ui.available_width(), true).clicked() {
            next = Some(Page::Edit(Box::new(editor::Editor::new(None, &app.monitors, app.hwnd))));
        }
    });
    next
}

fn no_hdr_card(ui: &mut egui::Ui, no_monitors: bool) {
    widgets::card(ui, |ui| {
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(vec2(34.0, 34.0), Sense::hover());
            widgets::glyph(ui.painter(), rect, Glyph::Monitor, c().muted);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 1.0;
                let title = if no_monitors {
                    tr("Sin monitores", "No monitors")
                } else {
                    tr("Ningún monitor con HDR", "No HDR monitor")
                };
                ui.label(egui::RichText::new(title).font(theme::semibold(15.0)));
                widgets::muted(
                    ui,
                    tr(
                        "Comprueba el cable y el modo HDR de tu monitor.",
                        "Check the cable and your monitor's HDR mode.",
                    ),
                );
            });
        });
    });
}

/// Tarjeta de un monitor: muestra el estado real y tiene el interruptor manual.
fn hero(ui: &mut egui::Ui, app: &mut App, d: &Display) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 118.0), Sense::hover());
    let id = egui::Id::unique(("hero", &d.id));
    let t = ui.ctx().animate_bool_with_time(id, d.hdr_on, 0.35);
    let p = ui.painter();
    p.rect(rect, CornerRadius::same(20), c().surface, Stroke::new(1.0, c().border), StrokeKind::Inside);
    if t > 0.0 {
        let base = c().surface;
        widgets::gradient_rect(p, rect, 20.0, |x| theme::lerp(base, theme::brand_gradient(x), t));
    }
    let ink = theme::lerp(c().text, Color32::WHITE, t);
    let soft = theme::lerp(c().muted, theme::alpha_on(Color32::WHITE, 0.85), t);
    let disc = pos2(rect.left() + 58.0, rect.center().y);
    widgets::range_disc(p, disc, 21.0, ink, t);

    let holders: Vec<&Hold> = app.holds_of(&d.id).collect();
    let by_app = holders.iter().find(|h| matches!(h.kind, HoldKind::App(_)));
    let manual = holders.iter().find(|h| matches!(h.kind, HoldKind::Manual(_)));
    let (title, detail) = match (d.hdr_on, by_app, manual) {
        (false, ..) => (
            tr("HDR apagado", "HDR off").to_string(),
            tr("Actívalo aquí o al abrir una app", "Turn it on here or by opening an app").to_string(),
        ),
        (true, Some(h), _) => {
            let name = &h.label;
            let since = elapsed(h.started_at);
            (
                tr("HDR encendido", "HDR on").to_string(),
                trf!("Lo mantiene {name} · {since}", "Kept on by {name} · {since}"),
            )
        }
        (true, None, Some(_)) => (
            tr("HDR encendido", "HDR on").to_string(),
            tr("A mano, hasta que lo apagues", "By hand, until you turn it off").to_string(),
        ),
        (true, None, None) => {
            (tr("HDR encendido", "HDR on").to_string(), tr("Encendido en Windows", "Turned on in Windows").to_string())
        }
    };
    let x = rect.left() + 104.0;
    let name_width = rect.width() - 104.0 - 76.0;
    p.text(pos2(x, rect.top() + 36.0), Align2::LEFT_CENTER, title, theme::bold(19.0), ink);
    let name = widgets::one_line(ui, d.name.clone(), theme::semibold(12.5), soft, name_width);
    p.galley(pos2(x, rect.top() + 50.0), name, soft);
    let line = widgets::one_line(ui, detail, egui::FontId::proportional(12.5), soft, name_width);
    p.galley(pos2(x, rect.top() + 69.0), line, soft);

    let switch = Rect::from_center_size(pos2(rect.right() - 46.0, rect.center().y), vec2(52.0, 30.0));
    let locked = by_app.is_some();
    let resp = widgets::toggle_at(ui, switch, id.with("switch"), d.hdr_on, !locked, t > 0.5);
    if locked {
        resp.on_hover_text(tr("Lo mantiene una app abierta", "An open app is keeping it on"));
    } else if resp.clicked() {
        let on = !d.hdr_on;
        app.order(&Command::Manual { monitor: Some(d.id.clone()), on: Some(on) });
    }
    ui.add_space(2.0);
}

fn empty_state(ui: &mut egui::Ui) {
    ui.add_space(18.0);
    ui.vertical_centered(|ui| {
        let (rect, _) = ui.allocate_exact_size(vec2(80.0, 80.0), Sense::hover());
        widgets::range_disc(ui.painter(), rect.center(), 24.0, theme::alpha(BRAND_A, 0.9), 1.0);
        ui.add_space(8.0);
        ui.label(egui::RichText::new(tr("Añade tu primera app", "Add your first app")).font(theme::semibold(17.0)));
        widgets::muted(
            ui,
            tr(
                "Elige un juego o reproductor y crea su acceso directo:",
                "Pick a game or player and create its shortcut:",
            ),
        );
        widgets::muted(
            ui,
            tr(
                "el HDR se encenderá al abrirlo y volverá como estaba al cerrarlo.",
                "HDR turns on when you open it and goes back when you close it.",
            ),
        );
    });
}

enum CardAction {
    None,
    Edit,
    Open,
    End,
    Auto(bool),
}

fn app_card(ui: &mut egui::Ui, app: &mut App, entry: &AppEntry) -> CardAction {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(width, 76.0), Sense::hover());
    let id = egui::Id::unique(("app_card", &entry.id));
    let resp = ui.interact(rect, id, Sense::click());
    let hover = ui.ctx().animate_bool_with_time(id, resp.hovered(), 0.12);
    let running = app.running(&entry.id);
    let exists = std::path::Path::new(&entry.exe).exists();
    let border =
        if running { theme::alpha(BRAND_A, 0.7) } else { theme::lerp(c().border, theme::alpha(BRAND_A, 0.5), hover) };
    ui.painter().rect(
        rect,
        CornerRadius::same(16),
        theme::lerp(c().surface, c().surface_hi, hover),
        Stroke::new(1.0, border),
        StrokeKind::Inside,
    );

    let icon_rect = Rect::from_center_size(pos2(rect.left() + 38.0, rect.center().y), vec2(42.0, 42.0));
    let ctx = ui.ctx().clone();
    match app.icons.get(&ctx, &entry.exe) {
        Some(tex) => {
            let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
            ui.painter().image(tex.id(), icon_rect, uv, Color32::WHITE);
        }
        None => {
            ui.painter().rect_filled(icon_rect, CornerRadius::same(10), c().surface_2);
            let letter = entry.name.chars().next().map(|l| l.to_uppercase().to_string()).unwrap_or_default();
            ui.painter().text(icon_rect.center(), Align2::CENTER_CENTER, letter, theme::bold(18.0), c().muted);
        }
    }

    let x = rect.left() + 72.0;
    let text_width = rect.width() - 72.0 - 112.0;
    let name = widgets::one_line(ui, entry.name.clone(), theme::semibold(15.5), c().text, text_width);
    let name_height = name.size().y;
    ui.painter().galley(pos2(x, rect.center().y - name_height - 1.0), name, c().text);
    let chip_y = rect.center().y + 12.0;
    let monitors: Vec<&Display> = app
        .monitors
        .iter()
        .filter(|d| entry.monitors.iter().any(|m| resolve(&app.monitors, m).is_some_and(|x| x.id == d.id)))
        .collect();
    let mut action = CardAction::None;
    let mut auto_area = Rect::NOTHING;
    if !exists && !running {
        widgets::chip_at(ui, pos2(x, chip_y), tr("Programa no encontrado", "Program not found"), c().warn);
    } else if monitors.is_empty() && !running {
        widgets::chip_at(ui, pos2(x, chip_y), tr("Sin monitor elegido", "No monitor chosen"), c().warn);
    } else {
        let switch = Rect::from_center_size(pos2(x + 16.0, chip_y), vec2(32.0, 18.0));
        let tip = tr(
            "Enciende el HDR justo después de que se abra, la abras como la abras. HDRShift arranca con Windows y se queda en la bandeja. Si un juego necesita el HDR antes de arrancar, usa el botón o su acceso directo.",
            "Turns HDR on right after it opens, however you open it. HDRShift starts with Windows and stays in the tray. If a game needs HDR before it starts, use the button or its shortcut.",
        );
        let toggle = widgets::toggle_at(ui, switch, id.with("auto"), entry.auto, true, false);
        if toggle.on_hover_text(tip).clicked() {
            action = CardAction::Auto(!entry.auto);
        }
        let label = tr("Encender solo", "Turn on by itself");
        let font = egui::FontId::proportional(12.5);
        let text = ui.painter().text(pos2(x + 40.0, chip_y), Align2::LEFT_CENTER, label, font, c().muted);
        // pulsar el texto también lo cambia: abrir el editor ahí sería una sorpresa
        let text_click = ui.interact(text, id.with("auto-text"), Sense::click());
        if text_click.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            action = CardAction::Auto(!entry.auto);
        }
        auto_area = switch.union(text);
        if running {
            widgets::chip_at(ui, pos2(text.right() + 8.0, chip_y), tr("HDR activo", "HDR on"), c().accent);
        }
    }

    let button = Rect::from_center_size(pos2(rect.right() - 36.0, rect.center().y), vec2(40.0, 40.0));
    if exists || running {
        let tip = if running {
            tr("Terminar la sesión de HDR", "End the HDR session")
        } else {
            tr("Abrir con HDR", "Open with HDR")
        };
        if widgets::play_button(ui, button, id.with("play"), running, tip).clicked() {
            action = if running { CardAction::End } else { CardAction::Open };
        }
    }
    let pencil = Rect::from_center_size(pos2(button.left() - 25.0, rect.center().y), vec2(34.0, 34.0));
    if widgets::icon_button_at(ui, pencil, id.with("edit"), Glyph::Edit, tr("Editar", "Edit")).clicked() {
        action = CardAction::Edit;
    }
    let on_button = |pos: Pos2| button.contains(pos) || pencil.contains(pos) || auto_area.contains(pos);
    if resp.clicked()
        && matches!(action, CardAction::None)
        && !on_button(resp.interact_pointer_pos().unwrap_or_default())
    {
        action = CardAction::Edit;
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand).on_hover_text(tr("Editar", "Edit"));
    ui.add_space(2.0);
    action
}

/// Interruptor "Encender solo" de una app.
fn set_app_auto(app: &mut App, id: &str, on: bool) {
    let mut cfg = app.cfg.clone();
    if let Some(entry) = cfg.apps.iter_mut().find(|a| a.id == id) {
        entry.auto = on;
    }
    if save_config(&cfg) {
        app.cfg = cfg;
    }
}

/// Guarda la configuración y avisa al servicio si está en marcha.
pub fn save_config(cfg: &Config) -> bool {
    match cfg.save() {
        Ok(()) => {
            i18n::set(cfg.language);
            ipc::send(&Command::Reload);
            crate::autostart::apply(cfg.starts_with_windows());
            if cfg.needs_tray() && !ipc::service_running() {
                ipc::spawn_self("--service");
            }
            true
        }
        Err(e) => {
            log::error(&format!("no se pudo guardar config.json: {e}"));
            false
        }
    }
}

/// Fila "atrás + título" de las páginas interiores. Devuelve `true` si se pulsó atrás.
pub fn page_header(ui: &mut egui::Ui, title: &str) -> bool {
    let mut back = false;
    ui.horizontal(|ui| {
        back = widgets::icon_button(ui, Glyph::Back, tr("Volver", "Back")).clicked();
        ui.label(egui::RichText::new(title).font(theme::bold(20.0)));
    });
    ui.add_space(4.0);
    back
}
