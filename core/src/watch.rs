//! Saber si una app sigue abierta.
//!
//! El proceso que se lanza no basta: muchos lanzadores abren el juego y se cierran. Por eso cuenta
//! cualquier proceso del exe exacto, de su misma carpeta o con el nombre que indicó el usuario. Se
//! compara la ruta completa: un `Launcher.exe` de otro programa no es esta app.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use windows_sys::Win32::Foundation::{CloseHandle, HWND, INVALID_HANDLE_VALUE, LPARAM};
use windows_sys::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows_sys::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GW_OWNER, GetWindow, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible,
};
use windows_sys::core::BOOL;

/// Margen para el traspaso de lanzador a juego: si no queda nada abierto, se espera esto antes de
/// dar la app por cerrada.
pub const EXIT_GRACE: Duration = Duration::from_secs(5);
/// Tiempo que se espera a que aparezca algún proceso de la app tras lanzarla.
pub const START_GRACE: Duration = Duration::from_secs(25);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Proc {
    pub pid: u32,
    /// Nombre del exe en minúsculas.
    pub name: String,
}

/// Procesos en ejecución.
pub fn snapshot() -> Vec<Proc> {
    let mut out = Vec::with_capacity(256);
    unsafe {
        let h = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if h == INVALID_HANDLE_VALUE {
            return out;
        }
        let mut e: PROCESSENTRY32W = std::mem::zeroed();
        e.dwSize = size_of::<PROCESSENTRY32W>() as u32;
        let mut ok = Process32FirstW(h, &mut e);
        while ok != 0 {
            let len = e.szExeFile.iter().position(|c| *c == 0).unwrap_or(e.szExeFile.len());
            let name = String::from_utf16_lossy(&e.szExeFile[..len]).to_lowercase();
            out.push(Proc { pid: e.th32ProcessID, name });
            ok = Process32NextW(h, &mut e);
        }
        CloseHandle(h);
    }
    out
}

/// Ruta completa en minúsculas de un proceso, si Windows deja leerla.
pub fn image_path(pid: u32) -> Option<String> {
    real_path(pid).map(|p| p.to_lowercase())
}

/// Un programa abierto, para añadirlo sin tener que buscar su exe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenApp {
    /// Título de su ventana.
    pub title: String,
    pub exe: String,
}

unsafe extern "system" fn collect_window(hwnd: HWND, list: LPARAM) -> BOOL {
    unsafe {
        let list = &mut *(list as *mut Vec<(u32, String)>);
        if IsWindowVisible(hwnd) != 0 && GetWindow(hwnd, GW_OWNER).is_null() {
            let mut buf = [0u16; 256];
            let len = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
            let mut pid = 0;
            GetWindowThreadProcessId(hwnd, &mut pid);
            if len > 0 && pid != 0 {
                list.push((pid, String::from_utf16_lossy(&buf[..len as usize])));
            }
        }
        1
    }
}

/// Programas con una ventana visible, sin Windows ni el propio HDRShift.
pub fn open_apps() -> Vec<OpenApp> {
    let mut windows: Vec<(u32, String)> = Vec::new();
    unsafe { EnumWindows(Some(collect_window), &mut windows as *mut _ as LPARAM) };
    let found = windows.into_iter().filter_map(|(pid, title)| Some(OpenApp { title, exe: real_path(pid)? })).collect();
    let own = std::env::current_exe().map(|p| p.display().to_string()).unwrap_or_default();
    choose_open_apps(found, &own, &std::env::var("WINDIR").unwrap_or_default())
}

fn choose_open_apps(found: Vec<OpenApp>, own_exe: &str, windows_dir: &str) -> Vec<OpenApp> {
    let windows_dir = format!("{}\\", normalize(windows_dir));
    let mut seen = HashSet::new();
    let mut out: Vec<OpenApp> = found
        .into_iter()
        .filter(|a| {
            let exe = normalize(&a.exe);
            exe != normalize(own_exe) && !exe.starts_with(&windows_dir) && seen.insert(exe)
        })
        .collect();
    out.sort_by_key(|a| a.title.to_lowercase());
    out
}

/// Ruta completa de un proceso, tal como está escrita.
fn real_path(pid: u32) -> Option<String> {
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            return None;
        }
        let mut buf = [0u16; 1024];
        let mut len = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(h, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len);
        CloseHandle(h);
        (ok != 0).then(|| String::from_utf16_lossy(&buf[..len as usize]))
    }
}

/// Carpetas donde conviven programas sin relación: en ellas no vale la regla de "misma carpeta".
/// Una entrada acabada en `\\` cubre también sus subcarpetas.
pub fn shared_folders() -> Vec<String> {
    let env = |name: &str| std::env::var(name).ok();
    let mut out: Vec<String> = [
        "WINDIR",
        "ProgramFiles",
        "ProgramFiles(x86)",
        "ProgramData",
        "APPDATA",
        "LOCALAPPDATA",
        "PUBLIC",
        "USERPROFILE",
    ]
    .iter()
    .filter_map(|n| env(n))
    .collect();
    if let Some(home) = env("USERPROFILE") {
        out.extend(["Desktop", "Downloads", "Documents"].iter().map(|f| format!("{home}\\{f}")));
    }
    let mut out: Vec<String> = out.iter().map(|p| normalize(p)).collect();
    // en System32 hay decenas de procesos de Windows siempre abiertos
    out.extend(env("WINDIR").map(|w| format!("{}\\", normalize(&w))));
    out
}

fn is_shared(folder: &str, shared: &[String]) -> bool {
    shared.iter().any(|s| match s.strip_suffix('\\') {
        Some(tree) => folder == tree || folder.starts_with(s.as_str()),
        None => folder == s,
    })
}

fn normalize(path: &str) -> String {
    path.replace('/', "\\").trim_end_matches('\\').to_lowercase()
}

/// Qué procesos cuentan como esta app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Target {
    exe: String,
    exe_name: String,
    /// `None` si el exe está en una carpeta compartida.
    folder: Option<String>,
    /// Nombre extra que indicó el usuario, en minúsculas y con `.exe`.
    extra: Option<String>,
}

impl Target {
    pub fn new(exe: &str, extra: &str, shared: &[String]) -> Self {
        let exe = normalize(exe);
        let (folder, exe_name) = match exe.rsplit_once('\\') {
            Some((f, n)) => (f.to_string(), n.to_string()),
            None => (String::new(), exe.clone()),
        };
        // una raíz como `c:` tampoco identifica a nadie
        let folder = (folder.len() > 3 && !is_shared(&folder, shared)).then_some(folder);
        let extra = extra.trim().to_lowercase();
        let extra = (!extra.is_empty()).then(|| if extra.ends_with(".exe") { extra } else { format!("{extra}.exe") });
        Self { exe, exe_name, folder, extra }
    }

    fn contains(&self, path: &str) -> bool {
        path == self.exe
            || self.folder.as_ref().is_some_and(|f| path.strip_prefix(f.as_str()).is_some_and(|r| r.starts_with('\\')))
    }

    /// Solo se pide la ruta de un proceso si puede cambiar la respuesta.
    fn needs_path(&self, name: &str) -> bool {
        self.folder.is_some() || name == self.exe_name
    }

    fn matches(&self, name: &str, path: Option<&str>) -> bool {
        if self.extra.as_deref() == Some(name) {
            return true;
        }
        match path {
            Some(p) => self.contains(p),
            // Windows no deja leer la ruta de procesos protegidos: queda el nombre
            None => name == self.exe_name,
        }
    }

    /// Hay algo abierto que cuenta como esta app, con las mismas reglas que `Tracker`.
    pub fn any_open(&self, procs: &[Proc], path_of: &mut impl FnMut(u32) -> Option<String>) -> bool {
        procs.iter().any(|p| {
            let path = if self.needs_path(&p.name) { path_of(p.pid) } else { None };
            self.matches(&p.name, path.as_deref())
        })
    }

    /// El exe exacto o el nombre extra están abiertos (para detectar la app sin haberla lanzado).
    pub fn running(&self, procs: &[Proc], path_of: &mut impl FnMut(u32) -> Option<String>) -> bool {
        procs.iter().any(|p| {
            self.extra.as_deref() == Some(p.name.as_str())
                || (p.name == self.exe_name && path_of(p.pid).is_none_or(|path| path == self.exe))
        })
    }
}

/// Sigue una app abierta. Recuerda qué procesos ya miró, para pedir cada ruta una sola vez.
#[derive(Debug)]
pub struct Tracker {
    target: Target,
    known: HashMap<u32, (String, bool)>,
    started: Instant,
    last_alive: Option<Instant>,
    /// Se quedó sin procesos y aún está dentro de `EXIT_GRACE`.
    closing: bool,
    /// Volvió a abrirse dentro de ese margen.
    reopened: bool,
}

impl Tracker {
    pub fn new(target: Target, now: Instant) -> Self {
        Self { target, known: HashMap::new(), started: now, last_alive: None, closing: false, reopened: false }
    }

    /// Para una app que ya se vio abierta: no hay que esperar a que aparezca.
    pub fn already_open(target: Target, now: Instant) -> Self {
        Self { last_alive: Some(now), ..Self::new(target, now) }
    }

    /// Se acaba de abrir otra vez: el margen para verla aparecer empieza de nuevo y el de cierre deja
    /// de contar.
    pub fn expect_again(&mut self, now: Instant) {
        self.started = now;
        self.last_alive = None;
        self.closing = false;
    }

    /// Se quedó sin procesos pero aún no cuenta como cerrada: puede volver en cualquier momento.
    pub fn is_closing(&self) -> bool {
        self.closing
    }

    /// Volvió a abrirse dentro del margen de cierre; se avisa una vez por cada vuelta. Sigue siendo
    /// la misma sesión, pero otro programa pudo apagar el HDR al ver cerrarse la app.
    pub fn take_reopened(&mut self) -> bool {
        std::mem::take(&mut self.reopened)
    }

    /// `false` cuando la app ya se cerró.
    pub fn update(&mut self, now: Instant, procs: &[Proc], path_of: &mut impl FnMut(u32) -> Option<String>) -> bool {
        let alive_pids: HashSet<u32> = procs.iter().map(|p| p.pid).collect();
        self.known.retain(|pid, _| alive_pids.contains(pid));
        let mut alive = false;
        for p in procs {
            // el mismo número con otro nombre es otro proceso: Windows reutiliza los números
            let matched = match self.known.get(&p.pid) {
                Some((name, matched)) if *name == p.name => *matched,
                _ => {
                    let path = if self.target.needs_path(&p.name) { path_of(p.pid) } else { None };
                    let matched = self.target.matches(&p.name, path.as_deref());
                    self.known.insert(p.pid, (p.name.clone(), matched));
                    matched
                }
            };
            alive |= matched;
        }
        if alive {
            self.reopened |= self.closing;
            self.closing = false;
            self.last_alive = Some(now);
            return true;
        }
        match self.last_alive {
            Some(t) => {
                self.closing = true;
                now.duration_since(t) < EXIT_GRACE
            }
            None => now.duration_since(self.started) < START_GRACE,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GAME: &str = "C:\\Games\\Mi Juego\\Launcher.exe";

    fn procs(list: &[(u32, &str)]) -> Vec<Proc> {
        list.iter().map(|(pid, name)| Proc { pid: *pid, name: name.to_string() }).collect()
    }

    fn shared() -> Vec<String> {
        vec![normalize("C:\\Users\\Ana\\Downloads"), "c:\\windows\\".to_string()]
    }

    fn step(t: &mut Tracker, now: Instant, list: &[(u32, &str, &str)]) -> bool {
        let p: Vec<Proc> = list.iter().map(|(pid, name, _)| Proc { pid: *pid, name: name.to_string() }).collect();
        let paths: HashMap<u32, String> = list.iter().map(|(pid, _, path)| (*pid, path.to_lowercase())).collect();
        t.update(now, &p, &mut |pid| paths.get(&pid).filter(|p| !p.is_empty()).cloned())
    }

    /// El lanzador abre el juego y se cierra: no debe restaurarse el HDR.
    #[test]
    fn follows_the_game_after_its_launcher_closes() {
        let start = Instant::now();
        let mut t = Tracker::new(Target::new(GAME, "", &shared()), start);
        assert!(step(&mut t, start, &[(10, "launcher.exe", GAME)]));
        let game = (11, "rdr2.exe", "C:\\Games\\Mi Juego\\RDR2.exe");
        assert!(step(&mut t, start + Duration::from_secs(4), &[game]));
        assert!(step(&mut t, start + Duration::from_secs(3600), &[game]));
    }

    /// Otro programa con un nombre genérico no mantiene el HDR encendido.
    #[test]
    fn a_process_with_the_same_name_elsewhere_is_not_the_app() {
        let start = Instant::now();
        let mut t = Tracker::new(Target::new(GAME, "", &shared()), start);
        let other = (20, "launcher.exe", "C:\\Otra\\Launcher.exe");
        assert!(step(&mut t, start, &[(10, "launcher.exe", GAME), other]));
        assert!(step(&mut t, start + Duration::from_secs(2), &[other]), "aún en el margen");
        assert!(!step(&mut t, start + Duration::from_secs(10), &[other]));
    }

    #[test]
    fn a_subfolder_with_a_similar_name_does_not_count() {
        let start = Instant::now();
        let mut t = Tracker::new(Target::new(GAME, "", &shared()), start);
        assert!(step(&mut t, start, &[(10, "launcher.exe", GAME)]));
        let near = (21, "x.exe", "C:\\Games\\Mi Juego 2\\x.exe");
        assert!(!step(&mut t, start + Duration::from_secs(10), &[near]));
    }

    /// Un exe suelto en Descargas no convierte a todo Descargas en la app.
    #[test]
    fn a_shared_folder_only_matches_the_exact_exe() {
        let exe = "C:\\Users\\Ana\\Downloads\\app.exe";
        let start = Instant::now();
        let mut t = Tracker::new(Target::new(exe, "", &shared()), start);
        assert!(step(&mut t, start, &[(10, "app.exe", exe)]));
        let other = (11, "otro.exe", "C:\\Users\\Ana\\Downloads\\otro.exe");
        assert!(!step(&mut t, start + Duration::from_secs(10), &[other]));
    }

    /// Una app de System32 no se confunde con los procesos de Windows, que nunca se cierran.
    #[test]
    fn windows_subfolders_only_match_the_exact_exe() {
        let exe = "C:\\Windows\\System32\\charmap.exe";
        let start = Instant::now();
        let mut t = Tracker::new(Target::new(exe, "", &shared()), start);
        let service = (11, "svchost.exe", "C:\\Windows\\System32\\svchost.exe");
        assert!(step(&mut t, start, &[(10, "charmap.exe", exe), service]));
        assert!(!step(&mut t, start + Duration::from_secs(10), &[service]));
    }

    #[test]
    fn the_extra_name_counts_wherever_it_is() {
        let start = Instant::now();
        let mut t = Tracker::new(Target::new(GAME, "juego-real", &shared()), start);
        assert!(step(&mut t, start, &[(30, "juego-real.exe", "D:\\Otro\\juego-real.exe")]));
        assert!(step(&mut t, start + Duration::from_secs(60), &[(30, "juego-real.exe", "D:\\Otro\\juego-real.exe")]));
    }

    #[test]
    fn a_protected_process_is_recognized_by_name() {
        let start = Instant::now();
        let mut t = Tracker::new(Target::new(GAME, "", &shared()), start);
        assert!(step(&mut t, start, &[(10, "launcher.exe", "")]));
        assert!(step(&mut t, start + Duration::from_secs(60), &[(10, "launcher.exe", "")]));
    }

    #[test]
    fn gives_up_if_the_app_never_appears() {
        let start = Instant::now();
        let mut t = Tracker::new(Target::new(GAME, "", &shared()), start);
        assert!(step(&mut t, start + Duration::from_secs(5), &[]));
        assert!(!step(&mut t, start + START_GRACE, &[]));
    }

    /// Se detectó abierta y se cerró antes de la primera comprobación: vale el margen corto.
    #[test]
    fn an_app_seen_open_uses_the_short_margin() {
        let start = Instant::now();
        let mut t = Tracker::already_open(Target::new(GAME, "", &shared()), start);
        assert!(!step(&mut t, start + EXIT_GRACE, &[]));
    }

    /// Se cierra y vuelve dentro del margen: sigue la sesión, pero debe saberse para pedir el HDR otra
    /// vez, porque mientras tanto otro programa pudo apagarlo.
    #[test]
    fn notices_an_app_that_comes_back_inside_the_margin() {
        let start = Instant::now();
        let mut t = Tracker::new(Target::new(GAME, "", &shared()), start);
        assert!(step(&mut t, start, &[(10, "launcher.exe", GAME)]));
        assert!(!t.take_reopened());
        assert!(step(&mut t, start + Duration::from_secs(2), &[]));
        assert!(t.is_closing());
        assert!(step(&mut t, start + Duration::from_secs(4), &[(11, "launcher.exe", GAME)]));
        assert!(!t.is_closing());
        assert!(t.take_reopened());
        assert!(!t.take_reopened(), "se avisa una sola vez");
    }

    /// Aparecer por primera vez no es volver: el HDR ya se pidió al lanzarla.
    #[test]
    fn the_first_appearance_is_not_a_return() {
        let start = Instant::now();
        let mut t = Tracker::new(Target::new(GAME, "", &shared()), start);
        assert!(step(&mut t, start + Duration::from_secs(5), &[]));
        assert!(!t.is_closing());
        assert!(step(&mut t, start + Duration::from_secs(10), &[(10, "launcher.exe", GAME)]));
        assert!(!t.take_reopened());
    }

    #[test]
    fn expecting_the_app_again_renews_the_margin() {
        let start = Instant::now();
        let mut t = Tracker::new(Target::new(GAME, "", &shared()), start);
        let later = start + START_GRACE;
        t.expect_again(later);
        assert!(step(&mut t, later + Duration::from_secs(10), &[]));
    }

    /// Se abre otra vez mientras se cerraba: no debe darse por cerrada antes de que aparezca.
    #[test]
    fn expecting_the_app_again_cancels_the_closing_margin() {
        let start = Instant::now();
        let mut t = Tracker::new(Target::new(GAME, "", &shared()), start);
        assert!(step(&mut t, start, &[(10, "launcher.exe", GAME)]));
        assert!(step(&mut t, start + Duration::from_secs(2), &[]));
        let again = start + Duration::from_secs(3);
        t.expect_again(again);
        assert!(!t.is_closing());
        assert!(step(&mut t, again + Duration::from_secs(10), &[]), "sigue esperando a que aparezca");
        assert!(!t.is_closing());
        assert!(!step(&mut t, again + START_GRACE, &[]));
    }

    #[test]
    fn asks_for_each_path_only_once() {
        let start = Instant::now();
        let mut t = Tracker::new(Target::new(GAME, "", &shared()), start);
        let list = procs(&[(1, "a.exe"), (2, "b.exe")]);
        let mut asked = 0;
        for _ in 0..3 {
            t.update(start, &list, &mut |_| {
                asked += 1;
                None
            });
        }
        assert_eq!(asked, 2);
    }

    #[test]
    fn a_reused_process_number_is_looked_at_again() {
        let start = Instant::now();
        let mut t = Tracker::new(Target::new(GAME, "", &shared()), start);
        assert!(step(&mut t, start, &[(10, "launcher.exe", GAME)]));
        assert!(!step(&mut t, start + Duration::from_secs(10), &[(10, "calc.exe", "C:\\calc.exe")]));
    }

    /// Tras un cierre brusco del servicio, el juego que abrió el lanzador cuenta como la app.
    #[test]
    fn a_game_of_the_same_folder_counts_after_a_crash() {
        let t = Target::new(GAME, "", &shared());
        let mut game = |_| Some("c:\\games\\mi juego\\rdr2.exe".to_string());
        assert!(t.any_open(&procs(&[(1, "rdr2.exe")]), &mut game));
        let mut other = |_| Some("c:\\otra\\rdr2.exe".to_string());
        assert!(!t.any_open(&procs(&[(1, "rdr2.exe")]), &mut other));
    }

    #[test]
    fn the_open_apps_list_has_no_windows_programs_no_repeats_and_not_itself() {
        let app = |title: &str, exe: &str| OpenApp { title: title.into(), exe: exe.into() };
        let found = vec![
            app("Zeta", "C:\\Games\\Zeta\\zeta.exe"),
            app("Otra ventana de Zeta", "C:\\Games\\Zeta\\ZETA.EXE"),
            app("Explorador", "C:\\Windows\\explorer.exe"),
            app("HDRShift", "C:\\Apps\\HDRShift.exe"),
            app("alfa", "D:\\alfa.exe"),
        ];
        let list = choose_open_apps(found, "C:\\Apps\\HDRShift.exe", "C:\\Windows");
        let titles: Vec<&str> = list.iter().map(|a| a.title.as_str()).collect();
        assert_eq!(titles, ["alfa", "Zeta"]);
    }

    /// La lista real de ventanas no debe fallar, haya las que haya.
    #[test]
    fn listing_open_apps_works() {
        for app in open_apps() {
            assert!(!app.exe.is_empty());
        }
    }

    #[test]
    fn detects_only_the_exact_exe_or_the_extra_name() {
        let t = Target::new(GAME, "", &shared());
        let mut path = |_| Some("c:\\games\\mi juego\\launcher.exe".to_string());
        assert!(t.running(&procs(&[(1, "launcher.exe")]), &mut path));
        let mut other = |_| Some("c:\\otra\\launcher.exe".to_string());
        assert!(!t.running(&procs(&[(1, "launcher.exe")]), &mut other));
        assert!(!t.running(&procs(&[(1, "rdr2.exe")]), &mut path));
    }
}
