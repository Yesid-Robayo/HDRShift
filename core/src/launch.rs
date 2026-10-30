//! Abre una app como lo haría Windows, para que funcionen también las que piden administrador.

use std::ptr::null;

use windows_sys::Win32::Foundation::{CloseHandle, GetLastError};
use windows_sys::Win32::System::Threading::GetProcessId;
use windows_sys::Win32::UI::Shell::{SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW};
use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Lanza `exe` en su carpeta. Devuelve el código de error de Windows si no se abre, también cuando
/// el usuario rechaza la ventana de permisos. Puede tardar: esa ventana espera al usuario.
pub fn open(exe: &str, args: &str) -> Result<u32, u32> {
    let folder = std::path::Path::new(exe).parent().map(|p| p.display().to_string()).unwrap_or_default();
    let (exe, args, folder) = (wide(exe), wide(args), wide(&folder));
    let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
    info.cbSize = size_of::<SHELLEXECUTEINFOW>() as u32;
    info.fMask = SEE_MASK_NOCLOSEPROCESS;
    info.lpFile = exe.as_ptr();
    info.lpParameters = if args.len() > 1 { args.as_ptr() } else { null() };
    info.lpDirectory = folder.as_ptr();
    info.nShow = SW_SHOWNORMAL;
    unsafe {
        if ShellExecuteExW(&mut info) == 0 {
            return Err(GetLastError());
        }
        if info.hProcess.is_null() {
            return Ok(0);
        }
        let pid = GetProcessId(info.hProcess);
        CloseHandle(info.hProcess);
        Ok(pid)
    }
}
