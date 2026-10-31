//! Abre una app como lo haría Windows, para que funcionen también las que piden administrador.

use std::ptr::null;

use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, HANDLE};
use windows_sys::Win32::System::Threading::GetProcessId;
use windows_sys::Win32::UI::Shell::{SEE_MASK_FLAG_NO_UI, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW};
use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Devuelve el proceso, que puede ser nulo si Windows lo reutilizó, o el código de error de Windows,
/// también si el usuario rechaza la ventana de permisos. Los errores no muestran ventanas: el que
/// llama decide cómo avisar.
fn execute(exe: &str, args: &str, folder: &str) -> Result<HANDLE, u32> {
    let (exe, args, folder) = (wide(exe), wide(args), wide(folder));
    let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
    info.cbSize = size_of::<SHELLEXECUTEINFOW>() as u32;
    info.fMask = SEE_MASK_NOCLOSEPROCESS | SEE_MASK_FLAG_NO_UI;
    info.lpFile = exe.as_ptr();
    info.lpParameters = if args.len() > 1 { args.as_ptr() } else { null() };
    info.lpDirectory = folder.as_ptr();
    info.nShow = SW_SHOWNORMAL;
    if unsafe { ShellExecuteExW(&mut info) } == 0 {
        return Err(unsafe { GetLastError() });
    }
    Ok(info.hProcess)
}

/// Lanza `exe` en su carpeta. Puede tardar: la ventana de permisos espera al usuario.
pub fn open(exe: &str, args: &str) -> Result<u32, u32> {
    let folder = std::path::Path::new(exe).parent().map(|p| p.display().to_string()).unwrap_or_default();
    let process = execute(exe, args, &folder)?;
    if process.is_null() {
        return Ok(0);
    }
    unsafe {
        let pid = GetProcessId(process);
        CloseHandle(process);
        Ok(pid)
    }
}
