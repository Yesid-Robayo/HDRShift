//! Servicio: el único dueño del HDR. Tiene el icono de bandeja y un hilo que atiende las órdenes y
//! vigila las apps. No carga nada gráfico pesado (la ventana es otro proceso), y si el usuario no
//! pidió quedarse en la bandeja, sale solo en cuanto no queda nada de lo que ocuparse.

use std::cell::{OnceCell, RefCell};
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use hdrshift_core::command::Command;
use hdrshift_core::config::{AppEntry, Config};
use hdrshift_core::display::{Display, Displays, SystemDisplays};
use hdrshift_core::i18n::{self, tr};
use hdrshift_core::ledger::FileLedger;
use hdrshift_core::session::{Engine, HoldKind, MonitorOutcome, MonitorResult, NOT_SAVED};
use hdrshift_core::status::Snapshot;
use hdrshift_core::watch::{self, Proc, Target, Tracker};
use hdrshift_core::{launch, log, registry, trf};
use windows_sys::Win32::Foundation::{CloseHandle, HWND, LPARAM, LRESULT, WAIT_ABANDONED, WAIT_OBJECT_0, WPARAM};
use windows_sys::Win32::System::Com::{COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoInitializeEx};
use windows_sys::Win32::System::DataExchange::COPYDATASTRUCT;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Threading::{CreateMutexW, WaitForSingleObject};
use windows_sys::Win32::UI::Shell::NIN_SELECT;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW, MSG, PostMessageW, PostQuitMessage,
    RegisterClassExW, RegisterWindowMessageW, TranslateMessage, WM_CONTEXTMENU, WM_COPYDATA, WM_DESTROY,
    WM_DISPLAYCHANGE, WM_ENDSESSION, WM_LBUTTONUP, WM_RBUTTONUP, WM_SETTINGCHANGE, WNDCLASSEXW, WS_EX_TOOLWINDOW,
    WS_OVERLAPPED,
};

use crate::icon::TrayState;
use crate::ipc::{self, COPY_COMMAND, MAX_COMMAND_BYTES, MSG_QUIT, MSG_SNAPSHOT, MSG_TRAY, wide};
use crate::tray::{self, Tray};

/// Cada cuánto se miran los procesos mientras hay algo que vigilar.
const TICK: Duration = Duration::from_secs(2);
/// Lo mismo si una app marcada puede abrirse en cualquier momento: el juego no espera a HDRShift, y
/// cuanto antes se vea, más probable es que el HDR llegue antes de que él lo compruebe.
const DETECT_TICK: Duration = Duration::from_millis(500);
/// Espera entre comprobaciones sin nada que hacer, en la bandeja.
const IDLE_WAIT: Duration = Duration::from_secs(60);
const PENDING_WAIT: Duration = Duration::from_secs(5);
const RETRY_GAP: Duration = Duration::from_secs(30);
/// Margen para que el monitor cambie de señal antes de abrir la app: muchas leen el HDR al arrancar.
const HDR_SETTLE: Duration = Duration::from_millis(1500);
/// Sin nada que hacer y sin quedarse en la bandeja, el proceso sale tras este margen.
const EXIT_AFTER: Duration = Duration::from_secs(2);
const SESSION_END_WAIT: Duration = Duration::from_secs(2);
const SERVICE_TRIES: u32 = 180;
const SERVICE_GAP: Duration = Duration::from_millis(500);
/// Tiempo total que un acceso directo intenta dar su orden.
const COMMAND_TRIES: u32 = 40;
const COMMAND_GAP: Duration = Duration::from_millis(100);

/// Selección del icono con el teclado (NIN_SELECT | NINF_KEY).
const NIN_KEYSELECT: u32 = NIN_SELECT | 1;
/// El usuario canceló la ventana de permisos.
const ERROR_CANCELLED: u32 = 1223;

const PERSONALIZE: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize";

enum Msg {
    Command(Command),
    /// Terminó el intento de abrir una app.
    Opened {
        app: String,
        result: Result<u32, u32>,
    },
    DisplayChanged,
    /// Windows cierra la sesión; responder cuando se haya hecho lo posible.
    SessionEnd(Sender<()>),
}

#[derive(Default)]
struct Shared {
    snapshot: Snapshot,
    /// Avisos pendientes de mostrar: (título, texto).
    notices: Vec<(String, String)>,
}

struct Service {
    tray: Tray,
    shared: Arc<Mutex<Shared>>,
    taskbar_created: u32,
}

thread_local! {
    static SERVICE: RefCell<Option<Service>> = const { RefCell::new(None) };
    /// Fuera de `SERVICE`: el icono de la bandeja puede atender mensajes mientras `SERVICE` está
    /// prestado, y una orden no puede perderse por eso.
    static TX: OnceCell<Sender<Msg>> = const { OnceCell::new() };
}

/// El servicio decidió salir: quien envíe una orden ahora recibe un rechazo y arranca otro.
static CLOSING: AtomicBool = AtomicBool::new(false);

/// Exclusión entre procesos: solo uno puede ser el dueño del HDR. Se conserva hasta salir; si el
/// dueño muere, Windows la marca como abandonada y el siguiente la recibe igual.
struct Instance;

impl Instance {
    fn acquire() -> Option<Self> {
        unsafe {
            let h = CreateMutexW(std::ptr::null(), 0, wide("Local\\HDRShift.Service").as_ptr());
            if h.is_null() {
                return None;
            }
            match WaitForSingleObject(h, 0) {
                WAIT_OBJECT_0 | WAIT_ABANDONED => Some(Self),
                _ => {
                    CloseHandle(h);
                    None
                }
            }
        }
    }
}

/// Da una orden: al servicio si existe, y si no, este proceso pasa a ser el servicio.
pub fn command(command: Command) {
    for _ in 0..COMMAND_TRIES {
        if ipc::send(&command) {
            return;
        }
        if let Some(instance) = Instance::acquire() {
            serve(instance, Some(command));
            return;
        }
        // otro proceso está arrancando o saliendo
        std::thread::sleep(COMMAND_GAP);
    }
    log::init("orden");
    log::error("no se pudo entregar la orden al servicio");
}

/// `--service`: arranca sin orden. Si ya hay uno, no hace nada. Al iniciar sesión, `--recover` puede
/// tener la exclusión un momento: se le espera para no quedar sin bandeja.
pub fn run() {
    for _ in 0..SERVICE_TRIES {
        if ipc::service_running() {
            return;
        }
        if let Some(instance) = Instance::acquire() {
            serve(instance, None);
            return;
        }
        std::thread::sleep(SERVICE_GAP);
    }
}

/// `--recover`: restaura lo que quedó pendiente y sale, sin mostrar nada. Si no puede, por ejemplo
/// porque la pantalla aún no está lista, sigue como servicio: lo reintenta y atiende los accesos directos.
pub fn recover() {
    let Some(instance) = Instance::acquire() else { return };
    if !FileLedger::exists() {
        return;
    }
    log::init("recuperación");
    let mut engine = Engine::new(SystemDisplays, FileLedger);
    engine.recover(|_| false);
    if !engine.is_idle() {
        log::warn("no se pudo restaurar el HDR al iniciar sesión, se seguirá intentando");
        serve(instance, None);
    }
}

fn taskbar_is_light() -> bool {
    registry::user_dword(PERSONALIZE, "SystemUsesLightTheme") == Some(1)
}

fn serve(_instance: Instance, first: Option<Command>) {
    log::init("servicio");
    let cfg = Config::try_load();
    // por si el exe cambió de carpeta; con un archivo ilegible no se sabe qué eligió el usuario
    if let Some(cfg) = &cfg {
        crate::autostart::apply(cfg.starts_with_windows());
    }
    let cfg = cfg.unwrap_or_default();
    i18n::set(cfg.language);
    tray::enable_dark_menus();
    let hwnd = unsafe { create_window() };
    if hwnd.is_null() {
        log::error("no se pudo crear la ventana del servicio");
        return;
    }
    let shared = Arc::new(Mutex::new(Shared::default()));
    let (tx, rx) = mpsc::channel();
    TX.with(|t| t.set(tx.clone()).ok());
    {
        let (shared, h, notify) = (shared.clone(), hwnd as isize, tx);
        std::thread::Builder::new()
            .name("sesiones".into())
            .spawn(move || worker(h, rx, notify, shared, cfg, first))
            .expect("hilo de sesiones");
    }
    let taskbar_created = unsafe { RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()) };
    let state = TrayState { active: false, warn: false, light_taskbar: taskbar_is_light() };
    let tray = Tray::new(hwnd, state);
    SERVICE.with(|s| *s.borrow_mut() = Some(Service { tray, shared, taskbar_created }));

    unsafe {
        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    SERVICE.with(|s| s.borrow_mut().take());
    Snapshot::clear();
    log::info("servicio cerrado");
}

unsafe fn create_window() -> HWND {
    unsafe {
        let class = wide(ipc::SERVICE_CLASS);
        let hinst = GetModuleHandleW(std::ptr::null());
        let wc = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wndproc),
            hInstance: hinst,
            lpszClassName: class.as_ptr(),
            ..std::mem::zeroed()
        };
        RegisterClassExW(&wc);
        CreateWindowExW(
            WS_EX_TOOLWINDOW,
            class.as_ptr(),
            class.as_ptr(),
            WS_OVERLAPPED,
            0,
            0,
            0,
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            hinst,
            std::ptr::null(),
        )
    }
}

fn with_service<R>(f: impl FnOnce(&mut Service) -> R) -> Option<R> {
    SERVICE.with(|s| s.try_borrow_mut().ok().and_then(|mut g| g.as_mut().map(f)))
}

fn send(msg: Msg) -> bool {
    TX.with(|t| t.get().is_some_and(|tx| tx.send(msg).is_ok()))
}

fn tray_state(snapshot: &Snapshot) -> TrayState {
    TrayState {
        active: !snapshot.holds.is_empty(),
        warn: !snapshot.pending.is_empty(),
        light_taskbar: taskbar_is_light(),
    }
}

fn tooltip(snapshot: &Snapshot) -> String {
    if !snapshot.pending.is_empty() {
        return tr("HDRShift · HDR por restaurar", "HDRShift · HDR left to restore").into();
    }
    match snapshot.holds.first() {
        Some(h) if snapshot.holds.len() == 1 => format!("HDRShift · {}", h.label),
        Some(_) => trf!("HDRShift · {} sesiones", "HDRShift · {} sessions", snapshot.holds.len()),
        None => "HDRShift".into(),
    }
}

fn app_sessions(snapshot: &Snapshot) -> Vec<(String, String)> {
    let app = |h: &hdrshift_core::session::Hold| match &h.kind {
        HoldKind::App(id) => Some((id.clone(), h.label.clone())),
        HoldKind::Manual(_) => None,
    };
    snapshot.holds.iter().filter_map(app).collect()
}

fn show_tray_menu(hwnd: HWND) {
    let snapshot =
        with_service(|s| s.shared.lock().map(|g| g.snapshot.clone()).unwrap_or_default()).unwrap_or_default();
    let sessions = app_sessions(&snapshot);
    let names: Vec<String> = sessions.iter().map(|(_, name)| name.clone()).collect();
    let hdr_on = SystemDisplays.list().is_ok_and(|l| l.iter().any(|d| d.hdr_on));
    match tray::show_menu(hwnd, &names, hdr_on) {
        tray::CMD_OPEN => ipc::spawn_self(""),
        tray::CMD_TOGGLE => {
            send(Msg::Command(Command::Manual { monitor: None, on: None }));
        }
        tray::CMD_QUIT => {
            send(Msg::Command(Command::Quit));
        }
        cmd if cmd >= tray::CMD_END => {
            if let Some((id, _)) = sessions.get(cmd - tray::CMD_END) {
                send(Msg::Command(Command::End(id.clone())));
            }
        }
        _ => {}
    }
}

/// Orden recibida de otro proceso. Solo se acepta si es una `Command` válida.
unsafe fn receive_command(lparam: LPARAM) -> bool {
    if lparam == 0 || CLOSING.load(Ordering::SeqCst) {
        return false;
    }
    let data = unsafe { &*(lparam as *const COPYDATASTRUCT) };
    if data.dwData != COPY_COMMAND || data.cbData as usize > MAX_COMMAND_BYTES || data.lpData.is_null() {
        return false;
    }
    let bytes = unsafe { std::slice::from_raw_parts(data.lpData as *const u8, data.cbData as usize) };
    let command = std::str::from_utf8(bytes).ok().and_then(Command::decode);
    command.is_some_and(|c| send(Msg::Command(c)))
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        MSG_TRAY => {
            match (lparam as u32) & 0xffff {
                NIN_SELECT | NIN_KEYSELECT | WM_LBUTTONUP => ipc::spawn_self(""),
                WM_CONTEXTMENU | WM_RBUTTONUP => show_tray_menu(hwnd),
                _ => {}
            }
            0
        }
        WM_COPYDATA => unsafe { receive_command(lparam) as LRESULT },
        MSG_SNAPSHOT => {
            with_service(|s| {
                let (snap, notices) = match s.shared.lock() {
                    Ok(mut g) => (g.snapshot.clone(), std::mem::take(&mut g.notices)),
                    Err(_) => return,
                };
                s.tray.update(tray_state(&snap), &tooltip(&snap));
                for (title, text) in notices {
                    s.tray.notify(&title, &text);
                }
            });
            0
        }
        MSG_QUIT => {
            unsafe { DestroyWindow(hwnd) };
            0
        }
        WM_DISPLAYCHANGE => {
            send(Msg::DisplayChanged);
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        WM_SETTINGCHANGE => {
            // el tema de la barra de tareas pudo cambiar
            with_service(|s| {
                let snap = s.shared.lock().map(|g| g.snapshot.clone()).unwrap_or_default();
                s.tray.update(tray_state(&snap), &tooltip(&snap));
            });
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        WM_ENDSESSION => {
            if wparam != 0 {
                let (ack, done) = mpsc::channel();
                send(Msg::SessionEnd(ack));
                let _ = done.recv_timeout(SESSION_END_WAIT);
            }
            0
        }
        WM_DESTROY => {
            unsafe { PostQuitMessage(0) };
            0
        }
        _ => {
            if with_service(|s| s.taskbar_created == msg && msg != 0).unwrap_or(false) {
                // el Explorador se reinició: volver a poner el icono
                with_service(|s| s.tray.add());
                return 0;
            }
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
    }
}

/// Una app con el HDR encendido.
struct Session {
    app: String,
    /// `None` hasta que se abre por primera vez.
    tracker: Option<Tracker>,
    /// Hay un intento de abrirla en curso. No tiene plazo: puede esperar a la ventana de permisos.
    opening: bool,
}

struct Worker {
    hwnd: isize,
    engine: Engine<SystemDisplays, FileLedger>,
    cfg: Config,
    sessions: Vec<Session>,
    /// Apps cuya sesión terminó el usuario sin cerrarlas: no se vuelven a detectar hasta que se cierren.
    dismissed: HashSet<String>,
    /// Apps con un fallo ya avisado en su sesión.
    warned: HashSet<String>,
    shared_folders: Vec<String>,
    tx: Sender<Msg>,
    shared: Arc<Mutex<Shared>>,
    published: Option<Snapshot>,
    last_retry: Instant,
    last_busy: Instant,
    quitting: bool,
    /// Nombre de este exe: ni la ventana ni el servicio cuentan como una app de su misma carpeta.
    own_name: String,
}

/// Nombre en minúsculas de este exe.
fn own_exe_name() -> String {
    let exe = std::env::current_exe().ok();
    exe.and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_lowercase())).unwrap_or_default()
}

fn worker(
    hwnd: isize,
    rx: Receiver<Msg>,
    tx: Sender<Msg>,
    shared: Arc<Mutex<Shared>>,
    cfg: Config,
    first: Option<Command>,
) {
    let now = Instant::now();
    let mut w = Worker {
        hwnd,
        engine: Engine::new(SystemDisplays, FileLedger),
        cfg,
        sessions: Vec::new(),
        dismissed: HashSet::new(),
        warned: HashSet::new(),
        shared_folders: watch::shared_folders(),
        tx,
        shared,
        published: None,
        last_retry: now,
        last_busy: now,
        quitting: false,
        own_name: own_exe_name(),
    };
    w.recover();
    if let Some(c) = first {
        w.command(c);
    }
    w.publish();
    loop {
        match rx.recv_timeout(w.wait()) {
            Ok(msg) => w.handle(msg),
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        // al cerrar no se mira nada más: la detección volvería a encender el HDR de una app abierta
        if !w.quitting {
            w.tick();
        }
        w.publish();
        if w.should_exit() {
            // desde aquí las órdenes se rechazan; lo que ya hubiera en la cola se atiende
            CLOSING.store(true, Ordering::SeqCst);
            match rx.try_recv() {
                Ok(msg) if !w.quitting => {
                    CLOSING.store(false, Ordering::SeqCst);
                    w.handle(msg);
                    w.publish();
                }
                Ok(_) | Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => {
                    unsafe { PostMessageW(hwnd as HWND, MSG_QUIT, 0, 0) };
                    break;
                }
            }
        }
    }
}

impl Worker {
    fn handle(&mut self, msg: Msg) {
        match msg {
            Msg::Command(c) => self.command(c),
            Msg::Opened { app, result } => self.opened(&app, result),
            // apagar el HDR también provoca este mensaje, y entonces no hay nada pendiente
            Msg::DisplayChanged => self.engine.retry(),
            Msg::SessionEnd(ack) => {
                self.close_everything();
                self.quitting = true;
                self.publish();
                let _ = ack.send(());
            }
        }
    }

    /// Procesos ajenos a HDRShift.
    fn processes(&self) -> Vec<Proc> {
        let mut procs = watch::snapshot();
        procs.retain(|p| p.name != self.own_name);
        procs
    }

    fn target(&self, app: &AppEntry) -> Target {
        Target::new(&app.exe, &app.watch, &self.shared_folders)
    }

    /// Lo que quedó de un proceso anterior: las apps que sigan abiertas conservan su sesión.
    fn recover(&mut self) {
        let procs = self.processes();
        let cfg = self.cfg.clone();
        let folders = self.shared_folders.clone();
        let open = |id: &str| {
            cfg.find(id)
                .is_some_and(|a| Target::new(&a.exe, &a.watch, &folders).any_open(&procs, &mut watch::image_path))
        };
        self.engine.recover(|h| matches!(&h.kind, HoldKind::App(id) if open(id)));
        let kept: Vec<String> = self
            .engine
            .holds()
            .iter()
            .filter_map(|h| match &h.kind {
                HoldKind::App(id) => Some(id.clone()),
                HoldKind::Manual(_) => None,
            })
            .collect();
        for id in kept {
            if let Some(app) = self.cfg.find(&id).cloned() {
                let tracker = Tracker::already_open(self.target(&app), Instant::now());
                self.sessions.push(Session { app: id, tracker: Some(tracker), opening: false });
            }
        }
    }

    fn notice(&mut self, title: String, text: String) {
        if let Ok(mut g) = self.shared.lock() {
            g.notices.push((title, text));
        }
    }

    /// Enciende el HDR para una app y avisa de cómo fue. Devuelve si cambió algo.
    fn hold_app(&mut self, app: &AppEntry) -> bool {
        let outcomes = self.engine.hold(HoldKind::App(app.id.clone()), &app.name, &app.monitors);
        self.report(&app.name, &outcomes);
        self.confirm(app, &outcomes)
    }

    /// Confirma con un aviso de Windows que el HDR se encendió: si ya estaba encendido no ha cambiado
    /// nada y no se avisa. Anota si hubo fallos, para que `renew` no los repita.
    fn confirm(&mut self, app: &AppEntry, outcomes: &[MonitorOutcome]) -> bool {
        let fine = |o: &MonitorOutcome| matches!(o.result, MonitorResult::TurnedOn | MonitorResult::AlreadyOn);
        if outcomes.iter().all(fine) {
            self.warned.remove(&app.id);
        } else {
            self.warned.insert(app.id.clone());
        }
        let changed = outcomes.iter().any(|o| o.result == MonitorResult::TurnedOn);
        if changed {
            let name = &app.name;
            self.notice("HDRShift".into(), trf!("HDR activo · {name}", "HDR on · {name}"));
        }
        changed
    }

    /// Avisa de lo que no salió bien.
    fn report(&mut self, label: &str, outcomes: &[MonitorOutcome]) {
        for o in outcomes {
            let text = match o.result {
                MonitorResult::Failed(NOT_SAVED) => {
                    trf!(
                        "{label}: no se pudo escribir en la carpeta de datos, seguirá sin HDR.",
                        "{label}: couldn't write to the data folder, it will run without HDR."
                    )
                }
                MonitorResult::Failed(code) => {
                    trf!(
                        "{label}: Windows no dejó encender el HDR (código {code}).",
                        "{label}: Windows refused to turn HDR on (code {code})."
                    )
                }
                MonitorResult::NotFound => {
                    trf!(
                        "{label}: no se encontró su monitor, seguirá sin HDR.",
                        "{label}: its monitor wasn't found, it will run without HDR."
                    )
                }
                MonitorResult::NoHdr => {
                    let name = &o.name;
                    trf!("{label}: {name} no admite HDR.", "{label}: {name} doesn't support HDR.")
                }
                MonitorResult::TurnedOn | MonitorResult::AlreadyOn => continue,
            };
            self.notice("HDRShift".into(), text);
        }
    }

    fn command(&mut self, command: Command) {
        match command {
            Command::Launch(id) => self.launch(&id),
            Command::Manual { monitor, on } => self.manual(monitor.as_deref(), on),
            Command::End(id) => {
                self.dismissed.insert(id.clone());
                self.end_session(&id);
            }
            Command::Forget => self.engine.forget_pending(),
            Command::Reload => {
                // un archivo ilegible no debe vaciar la lista de apps ni quitar el inicio con Windows
                if let Some(cfg) = Config::try_load() {
                    self.cfg = cfg;
                    i18n::set(self.cfg.language);
                    crate::autostart::apply(self.cfg.starts_with_windows());
                }
            }
            Command::Quit => {
                self.close_everything();
                self.quitting = true;
            }
        }
    }

    fn close_everything(&mut self) {
        self.sessions.clear();
        self.engine.release_all();
    }

    /// Enciende el HDR y deja de esperar por la app en un hilo aparte: abrirla puede quedarse
    /// esperando a la ventana de permisos.
    fn launch(&mut self, id: &str) {
        let Some(app) = self.cfg.find(id).cloned() else {
            log::warn("orden para una app que no existe");
            self.notice(
                "HDRShift".into(),
                tr("Esa app ya no está en HDRShift.", "That app is no longer in HDRShift.").into(),
            );
            return;
        };
        if !std::path::Path::new(&app.exe).exists() {
            log::warn(&format!("{}: el exe no existe", app.name));
            let name = &app.name;
            self.notice(
                "HDRShift".into(),
                trf!("No se encontró el programa de {name}.", "The program for {name} wasn't found."),
            );
            return;
        }
        // ya se está abriendo: un doble clic no debe abrirla dos veces
        if self.sessions.iter().any(|s| s.app == app.id && s.opening) {
            return;
        }
        log::info(&format!("abriendo {}", app.name));
        self.dismissed.remove(id);
        let changed = self.hold_app(&app);
        let settle = HDR_SETTLE + Duration::from_secs(app.extra_wait.into());
        match self.sessions.iter_mut().find(|s| s.app == app.id) {
            Some(s) => s.opening = true,
            None => self.sessions.push(Session { app: app.id.clone(), tracker: None, opening: true }),
        }
        let tx = self.tx.clone();
        let spawned = std::thread::Builder::new().name("abrir".into()).spawn(move || {
            unsafe { CoInitializeEx(std::ptr::null(), (COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) as u32) };
            if changed {
                std::thread::sleep(settle);
            }
            let result = launch::open(&app.exe, &app.args);
            let _ = tx.send(Msg::Opened { app: app.id, result });
        });
        if let Err(e) = spawned {
            log::error(&format!("no se pudo crear el hilo para abrir la app: {e}"));
            self.open_failed(id);
        }
    }

    fn opened(&mut self, id: &str, result: Result<u32, u32>) {
        let name = self.cfg.find(id).map(|a| a.name.clone()).unwrap_or_default();
        match result {
            Ok(_) => {
                let Some(app) = self.cfg.find(id).cloned() else {
                    self.open_failed(id);
                    return;
                };
                let target = self.target(&app);
                let now = Instant::now();
                if let Some(s) = self.sessions.iter_mut().find(|s| s.app == id) {
                    s.opening = false;
                    match &mut s.tracker {
                        Some(t) => t.expect_again(now),
                        None => s.tracker = Some(Tracker::new(target, now)),
                    }
                }
            }
            Err(ERROR_CANCELLED) => {
                log::info(&format!("{name}: el usuario canceló"));
                self.open_failed(id);
            }
            Err(code) => {
                log::error(&format!("{name}: no se pudo abrir, código {code}"));
                self.notice(
                    "HDRShift".into(),
                    trf!("No se pudo abrir {name} (código {code}).", "Couldn't open {name} (code {code})."),
                );
                self.open_failed(id);
            }
        }
    }

    /// Un intento fallido solo termina la sesión si es la que se estaba abriendo: si la app ya estaba
    /// abierta y falló un segundo clic, sigue necesitando su HDR.
    fn open_failed(&mut self, id: &str) {
        let Some(s) = self.sessions.iter_mut().find(|s| s.app == id) else { return };
        s.opening = false;
        if s.tracker.is_none() {
            self.end_session(id);
        }
    }

    fn end_session(&mut self, id: &str) {
        self.sessions.retain(|s| s.app != id);
        self.warned.remove(id);
        self.engine.release(&HoldKind::App(id.to_string()));
    }

    fn manual(&mut self, monitor: Option<&str>, on: Option<bool>) {
        let list = match SystemDisplays.list() {
            Ok(list) => list,
            Err(code) => {
                log::error(&format!("no se pudieron leer los monitores: código {code}"));
                return;
            }
        };
        let targets: Vec<&Display> =
            list.iter().filter(|d| d.supports_hdr && monitor.is_none_or(|m| m == d.id)).collect();
        let want = on.unwrap_or_else(|| !targets.iter().any(|d| d.hdr_on));
        for d in targets {
            let kind = HoldKind::Manual(d.id.clone());
            if want {
                if !d.hdr_on {
                    let outcomes = self.engine.hold(kind.clone(), tr("Manual", "Manual"), std::slice::from_ref(&d.id));
                    self.report(&d.name, &outcomes);
                    // sin nada encendido, esta sesión mantendría vivo el servicio para nada
                    if !outcomes.iter().any(|o| o.result == MonitorResult::TurnedOn) {
                        self.engine.release(&kind);
                    }
                }
            } else if self.engine.has_hold(&kind) {
                self.engine.release(&kind);
            } else if !self.engine.holds().iter().any(|h| h.monitors.contains(&d.id))
                && let Err(code) = self.engine.force_off(&d.id)
            {
                log::warn(&format!("no se pudo apagar el HDR en {}: código {code}", d.name));
            }
        }
    }

    fn wait(&self) -> Duration {
        let waiting = |a: &AppEntry| a.auto && !self.sessions.iter().any(|s| s.app == a.id);
        let closing = self.sessions.iter().any(|s| s.tracker.as_ref().is_some_and(Tracker::is_closing));
        if closing || self.cfg.apps.iter().any(waiting) {
            DETECT_TICK
        } else if !self.sessions.is_empty() || !self.dismissed.is_empty() {
            TICK
        } else if !self.engine.pending().is_empty() {
            PENDING_WAIT
        } else if self.cfg.needs_tray() {
            IDLE_WAIT
        } else {
            EXIT_AFTER / 2
        }
    }

    fn tick(&mut self) {
        let watching = self.sessions.iter().any(|s| s.tracker.is_some());
        if watching || self.cfg.any_auto() || !self.dismissed.is_empty() {
            self.follow_apps();
        }
        if !self.engine.pending().is_empty() && self.last_retry.elapsed() >= RETRY_GAP {
            self.last_retry = Instant::now();
            self.engine.retry();
        }
    }

    fn follow_apps(&mut self) {
        let procs = self.processes();
        // sin lista no se sabe nada: no significa que las apps se hayan cerrado
        if procs.is_empty() {
            return;
        }
        let now = Instant::now();
        let mut closed = Vec::new();
        let mut reopened = Vec::new();
        for s in &mut self.sessions {
            let Some(t) = &mut s.tracker else { continue };
            let open = t.update(now, &procs, &mut watch::image_path);
            if !open && !s.opening {
                closed.push(s.app.clone());
            } else if t.take_reopened() {
                reopened.push(s.app.clone());
            }
        }
        for id in closed {
            log::info("la app se cerró");
            self.end_session(&id);
        }
        for id in reopened {
            self.renew(&id);
        }
        self.detect(&procs);
    }

    /// Una app que vuelve dentro del margen de cierre conserva su sesión, así que `detect` no la ve;
    /// otro programa pudo apagar el HDR al verla cerrarse.
    fn renew(&mut self, id: &str) {
        let Some(app) = self.cfg.find(id).cloned() else { return };
        log::info(&format!("{} volvió a abrirse", app.name));
        let outcomes = self.engine.hold(HoldKind::App(app.id.clone()), &app.name, &app.monitors);
        // el mismo fallo no se avisa otra vez en el relevo entre lanzador y juego
        if !self.warned.contains(&app.id) {
            self.report(&app.name, &outcomes);
        }
        self.confirm(&app, &outcomes);
    }

    /// Apps marcadas para encenderse solas: abiertas por cualquier camino.
    fn detect(&mut self, procs: &[Proc]) {
        let cfg = self.cfg.clone();
        self.dismissed.retain(|id| {
            cfg.find(id).is_some_and(|a| {
                Target::new(&a.exe, &a.watch, &self.shared_folders).running(procs, &mut watch::image_path)
            })
        });
        for app in cfg.apps.iter().filter(|a| a.auto) {
            let known = self.dismissed.contains(&app.id) || self.sessions.iter().any(|s| s.app == app.id);
            if known || !self.target(app).running(procs, &mut watch::image_path) {
                continue;
            }
            log::info(&format!("detectada {}", app.name));
            self.hold_app(app);
            let tracker = Tracker::already_open(self.target(app), Instant::now());
            self.sessions.push(Session { app: app.id.clone(), tracker: Some(tracker), opening: false });
        }
    }

    fn publish(&mut self) {
        let snapshot = Snapshot { holds: self.engine.holds().to_vec(), pending: self.engine.pending() };
        let changed = self.published.as_ref() != Some(&snapshot);
        if changed {
            let was_clean = self.published.as_ref().is_none_or(|s| s.pending.is_empty());
            if was_clean && !snapshot.pending.is_empty() {
                self.notice(
                    "HDRShift".into(),
                    tr(
                        "No se pudo devolver el HDR a como estaba. Se reintentará solo.",
                        "HDR couldn't be put back as it was. It will retry on its own.",
                    )
                    .into(),
                );
            }
            if let Err(e) = snapshot.save() {
                log::warn(&format!("no se pudo escribir status.json: {e}"));
            }
            self.published = Some(snapshot.clone());
        }
        let Ok(mut g) = self.shared.lock() else { return };
        let notify = changed || !g.notices.is_empty();
        g.snapshot = snapshot;
        drop(g);
        if notify {
            unsafe { PostMessageW(self.hwnd as HWND, MSG_SNAPSHOT, 0, 0) };
        }
    }

    fn should_exit(&mut self) -> bool {
        if self.quitting {
            return true;
        }
        if !self.sessions.is_empty() || !self.engine.is_idle() {
            self.last_busy = Instant::now();
            return false;
        }
        !self.cfg.needs_tray() && self.last_busy.elapsed() >= EXIT_AFTER
    }
}
