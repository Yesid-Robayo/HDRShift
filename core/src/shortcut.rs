//! Crea accesos directos (`.lnk`) con la interfaz COM `IShellLinkW`. `windows-sys` no trae las
//! interfaces COM, así que se llaman por su posición en la tabla: esas posiciones son parte del
//! contrato público de Windows y no cambian.

use std::ffi::c_void;
use std::path::Path;
use std::ptr::null_mut;

use windows_sys::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
};
use windows_sys::Win32::UI::Shell::ShellLink;
use windows_sys::core::GUID;

const IID_SHELL_LINK_W: GUID = GUID::from_u128(0x000214f9_0000_0000_c000_000000000046);
const IID_PERSIST_FILE: GUID = GUID::from_u128(0x0000010b_0000_0000_c000_000000000046);

// Posición en la tabla de cada método usado.
const QUERY_INTERFACE: usize = 0;
const RELEASE: usize = 2;
const SET_DESCRIPTION: usize = 7;
const SET_WORKING_DIRECTORY: usize = 9;
const SET_ARGUMENTS: usize = 11;
const SET_ICON_LOCATION: usize = 17;
const SET_PATH: usize = 20;
const PERSIST_SAVE: usize = 6;

type QueryFn = unsafe extern "system" fn(*mut c_void, *const GUID, *mut *mut c_void) -> i32;
type ReleaseFn = unsafe extern "system" fn(*mut c_void) -> u32;
type SetTextFn = unsafe extern "system" fn(*mut c_void, *const u16) -> i32;
type SetIconFn = unsafe extern "system" fn(*mut c_void, *const u16, i32) -> i32;
type SaveFn = unsafe extern "system" fn(*mut c_void, *const u16, i32) -> i32;

pub struct Link<'a> {
    pub path: &'a Path,
    pub target: &'a Path,
    pub args: &'a str,
    pub work_dir: &'a Path,
    pub description: &'a str,
    pub icon: &'a Path,
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Método `index` de la tabla del objeto COM `object`.
unsafe fn method<T: Copy>(object: *mut c_void, index: usize) -> T {
    unsafe {
        let table = *(object as *const *const *const c_void);
        std::mem::transmute_copy(&*table.add(index))
    }
}

fn check(hr: i32, what: &str) -> Result<(), String> {
    if hr < 0 { Err(format!("{what}: 0x{:08x}", hr as u32)) } else { Ok(()) }
}

/// Libera un objeto COM al salir de su ámbito.
struct Com(*mut c_void);

impl Drop for Com {
    fn drop(&mut self) {
        unsafe { method::<ReleaseFn>(self.0, RELEASE)(self.0) };
    }
}

unsafe fn fill_and_save(link: *mut c_void, data: &Link) -> Result<(), String> {
    unsafe {
        let set = |index: usize, value: &str, what: &str| {
            let text = wide(value);
            check(method::<SetTextFn>(link, index)(link, text.as_ptr()), what)
        };
        set(SET_PATH, &data.target.display().to_string(), "ruta")?;
        set(SET_ARGUMENTS, data.args, "argumentos")?;
        set(SET_WORKING_DIRECTORY, &data.work_dir.display().to_string(), "carpeta")?;
        set(SET_DESCRIPTION, data.description, "descripción")?;
        let icon = wide(&data.icon.display().to_string());
        check(method::<SetIconFn>(link, SET_ICON_LOCATION)(link, icon.as_ptr(), 0), "icono")?;

        let mut persist = null_mut();
        check(method::<QueryFn>(link, QUERY_INTERFACE)(link, &IID_PERSIST_FILE, &mut persist), "IPersistFile")?;
        let persist = Com(persist);
        let file = wide(&data.path.display().to_string());
        check(method::<SaveFn>(persist.0, PERSIST_SAVE)(persist.0, file.as_ptr(), 1), "guardar")
    }
}

pub fn create(data: &Link) -> Result<(), String> {
    unsafe {
        let init = CoInitializeEx(null_mut(), COINIT_APARTMENTTHREADED as u32);
        let result = (|| {
            let mut link = null_mut();
            check(
                CoCreateInstance(&ShellLink, null_mut(), CLSCTX_INPROC_SERVER, &IID_SHELL_LINK_W, &mut link),
                "ShellLink",
            )?;
            let link = Com(link);
            fill_and_save(link.0, data)
        })();
        if init >= 0 {
            CoUninitialize();
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Crea un acceso de verdad y comprueba lo que quedó escrito en el archivo (texto UTF-16).
    #[test]
    fn writes_a_real_shortcut() {
        let dir = std::env::temp_dir().join("hdrshift-tests");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("prueba (HDR).lnk");
        let _ = std::fs::remove_file(&path);
        let windows = std::path::PathBuf::from(std::env::var("WINDIR").unwrap());
        create(&Link {
            path: &path,
            target: &windows.join("notepad.exe"),
            args: "--launch abc123",
            work_dir: &windows,
            description: "Prueba",
            icon: &windows.join("notepad.exe"),
        })
        .unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(&bytes[..4], &[0x4c, 0, 0, 0], "cabecera de un .lnk");
        let text: Vec<u16> = bytes.as_chunks::<2>().0.iter().map(|b| u16::from_le_bytes(*b)).collect();
        let needle: Vec<u16> = "--launch abc123".encode_utf16().collect();
        let odd: Vec<u16> = bytes[1..].as_chunks::<2>().0.iter().map(|b| u16::from_le_bytes(*b)).collect();
        assert!(text.windows(needle.len()).any(|w| w == needle) || odd.windows(needle.len()).any(|w| w == needle));
        std::fs::remove_file(&path).unwrap();
    }
}
