//! Comunicación con el servicio (otro proceso del mismo exe).
//!
//! El servicio tiene una ventana oculta de clase `SERVICE_CLASS`. Las órdenes viajan por
//! `WM_COPYDATA` como `Command` en JSON: solo pueden pedir cosas que ya sabe hacer con las apps de la
//! configuración, nunca ejecutar una ruta que venga en el mensaje.

use std::ptr::null;
use std::time::Duration;

use hdrshift_core::command::Command;
use windows_sys::Win32::System::DataExchange::COPYDATASTRUCT;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    FindWindowW, SMTO_ABORTIFHUNG, SMTO_BLOCK, SendMessageTimeoutW, WM_APP, WM_COPYDATA,
};

pub const SERVICE_CLASS: &str = "HDRShiftService";

/// Identifica los `WM_COPYDATA` que son órdenes de HDRShift.
pub const COPY_COMMAND: usize = 0x4844_5253;
/// Más que cualquier orden real.
pub const MAX_COMMAND_BYTES: usize = 4096;

/// Uso interno del servicio: el hilo de trabajo avisa de un estado nuevo.
pub const MSG_SNAPSHOT: u32 = WM_APP + 4;
/// Uso interno del servicio: el hilo de trabajo terminó.
pub const MSG_QUIT: u32 = WM_APP + 3;
/// Uso interno del servicio: eventos del icono de la bandeja.
pub const MSG_TRAY: u32 = WM_APP + 10;

/// Tiempo que se espera a que el servicio recién lanzado acepte órdenes.
const START_TRIES: u32 = 60;
const START_GAP: Duration = Duration::from_millis(50);

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn service_window() -> *mut core::ffi::c_void {
    unsafe { FindWindowW(wide(SERVICE_CLASS).as_ptr(), null()) }
}

pub fn service_running() -> bool {
    !service_window().is_null()
}

/// `true` si el servicio está en marcha y recibió la orden.
pub fn send(command: &Command) -> bool {
    let hwnd = service_window();
    if hwnd.is_null() {
        return false;
    }
    let text = command.encode();
    let data = COPYDATASTRUCT {
        dwData: COPY_COMMAND,
        cbData: text.len() as u32,
        lpData: text.as_ptr() as *mut core::ffi::c_void,
    };
    let mut result = 0usize;
    unsafe {
        SendMessageTimeoutW(
            hwnd,
            WM_COPYDATA,
            0,
            &data as *const COPYDATASTRUCT as isize,
            SMTO_ABORTIFHUNG | SMTO_BLOCK,
            2000,
            &mut result,
        ) != 0
            && result == 1
    }
}

/// Envía la orden; si el servicio no está, lo arranca y espera a que pueda recibirla.
pub fn command(command: &Command) -> bool {
    if send(command) {
        return true;
    }
    spawn_self("--service");
    for _ in 0..START_TRIES {
        std::thread::sleep(START_GAP);
        if send(command) {
            return true;
        }
    }
    hdrshift_core::log::error("el servicio no aceptó la orden");
    false
}

/// Arranca otra instancia del exe con `arg`.
pub fn spawn_self(arg: &str) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{ASFW_ANY, AllowSetForegroundWindow};
    // la bandeja le cede el primer plano: sin esto, la ventana nueva se abriría detrás de otras
    unsafe { AllowSetForegroundWindow(ASFW_ANY) };
    let Ok(exe) = std::env::current_exe() else { return };
    let mut process = std::process::Command::new(exe);
    if !arg.is_empty() {
        process.arg(arg);
    }
    if let Err(e) = process.spawn() {
        hdrshift_core::log::error(&format!("no se pudo abrir {arg}: {e}"));
    }
}

/// Abre una URL o carpeta con el programa predeterminado (solo por acción del usuario).
pub fn open_external(target: &str) {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    unsafe {
        let op = wide("open");
        let t = wide(target);
        ShellExecuteW(std::ptr::null_mut(), op.as_ptr(), t.as_ptr(), null(), null(), SW_SHOWNORMAL);
    }
}
