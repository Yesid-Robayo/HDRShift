//! Ventanas de Windows para elegir un archivo. Corren en su propio hilo para que la ventana de la
//! app siga pintándose mientras están abiertas.

use std::sync::mpsc::{self, Receiver};

use windows_sys::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
use windows_sys::Win32::UI::Controls::Dialogs::{
    GetOpenFileNameW, GetSaveFileNameW, OFN_FILEMUSTEXIST, OFN_HIDEREADONLY, OFN_NOCHANGEDIR, OFN_OVERWRITEPROMPT,
    OFN_PATHMUSTEXIST, OPENFILENAMEW,
};

use crate::ipc::wide;

const PATH_CHARS: usize = 4096;

/// Filtro de Windows: pares de texto separados por nulos y un nulo final.
fn filter(label: &str, pattern: &str) -> Vec<u16> {
    format!("{label}\0{pattern}\0\0").encode_utf16().collect()
}

struct Request {
    owner: isize,
    filter: Vec<u16>,
    title: String,
    folder: String,
    file: String,
    extension: Option<&'static str>,
    save: bool,
}

fn show(r: Request) -> Option<String> {
    let mut buffer = vec![0u16; PATH_CHARS];
    let name: Vec<u16> = r.file.encode_utf16().take(PATH_CHARS - 1).collect();
    buffer[..name.len()].copy_from_slice(&name);
    let (title, folder) = (wide(&r.title), wide(&r.folder));
    let extension = r.extension.map(wide);
    let mut o: OPENFILENAMEW = unsafe { std::mem::zeroed() };
    o.lStructSize = size_of::<OPENFILENAMEW>() as u32;
    o.hwndOwner = r.owner as *mut _;
    o.lpstrFilter = r.filter.as_ptr();
    o.lpstrFile = buffer.as_mut_ptr();
    o.nMaxFile = PATH_CHARS as u32;
    o.lpstrTitle = title.as_ptr();
    o.lpstrInitialDir = if r.folder.is_empty() { std::ptr::null() } else { folder.as_ptr() };
    o.lpstrDefExt = extension.as_ref().map_or(std::ptr::null(), |e| e.as_ptr());
    o.Flags = OFN_NOCHANGEDIR
        | OFN_HIDEREADONLY
        | if r.save { OFN_OVERWRITEPROMPT } else { OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST };
    let ok = unsafe {
        CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32);
        if r.save { GetSaveFileNameW(&mut o) } else { GetOpenFileNameW(&mut o) }
    };
    if ok == 0 {
        return None;
    }
    let len = buffer.iter().position(|c| *c == 0).unwrap_or(buffer.len());
    Some(String::from_utf16_lossy(&buffer[..len]))
}

fn in_background(request: Request) -> Receiver<Option<String>> {
    let (tx, rx) = mpsc::channel();
    let _ = std::thread::Builder::new().name("archivo".into()).spawn(move || {
        let _ = tx.send(show(request));
    });
    rx
}

pub fn pick_exe(owner: isize, title: &str, label: &str, folder: String) -> Receiver<Option<String>> {
    in_background(Request {
        owner,
        filter: filter(label, "*.exe"),
        title: title.to_string(),
        folder,
        file: String::new(),
        extension: None,
        save: false,
    })
}

pub fn save_shortcut(owner: isize, title: &str, label: &str, folder: String, file: String) -> Receiver<Option<String>> {
    in_background(Request {
        owner,
        filter: filter(label, "*.lnk"),
        title: title.to_string(),
        folder,
        file,
        extension: Some("lnk"),
        save: true,
    })
}
