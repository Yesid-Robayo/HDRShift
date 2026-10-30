//! Lectura y cambio del HDR con la API CCD de Windows (`user32.dll`).
//!
//! Windows 11 24H2 y posteriores separan el HDR de la gestión automática de color: ahí se usan las
//! peticiones nuevas (`GET_ADVANCED_COLOR_INFO_2`, `SET_HDR_STATE`), que no tocan esa gestión. En
//! versiones anteriores solo existe el par clásico.

use std::mem::zeroed;
use std::ptr::null_mut;

use windows_sys::Win32::Devices::Display::{
    DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME, DISPLAYCONFIG_DEVICE_INFO_HEADER, DISPLAYCONFIG_MODE_INFO,
    DISPLAYCONFIG_PATH_INFO, DISPLAYCONFIG_TARGET_DEVICE_NAME, DisplayConfigGetDeviceInfo, DisplayConfigSetDeviceInfo,
    GetDisplayConfigBufferSizes, QDC_ONLY_ACTIVE_PATHS, QueryDisplayConfig,
};
use windows_sys::Win32::Foundation::{ERROR_INSUFFICIENT_BUFFER, ERROR_SUCCESS, LUID};

use crate::i18n::tr;

const GET_ADVANCED_COLOR_INFO: i32 = 9;
const SET_ADVANCED_COLOR_STATE: i32 = 10;
const GET_ADVANCED_COLOR_INFO_2: i32 = 15;
const SET_HDR_STATE: i32 = 16;

/// La topología puede cambiar entre pedir el tamaño y leerla; la documentación pide reintentar.
const QUERY_TRIES: u32 = 4;

#[repr(C)]
struct ColorInfo {
    header: DISPLAYCONFIG_DEVICE_INFO_HEADER,
    value: u32,
    encoding: u32,
    bits: u32,
}

#[repr(C)]
struct ColorInfo2 {
    header: DISPLAYCONFIG_DEVICE_INFO_HEADER,
    value: u32,
    encoding: u32,
    bits: u32,
    active_mode: u32,
}

#[repr(C)]
struct SetState {
    header: DISPLAYCONFIG_DEVICE_INFO_HEADER,
    value: u32,
}

/// Datos con los que Windows reconoce el monitor ahora mismo. Cambian entre arranques: por eso no
/// sirven para guardarlos, y por eso `Display::id` es otra cosa.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Handle {
    adapter_low: u32,
    adapter_high: i32,
    target: u32,
    /// Windows entiende las peticiones nuevas.
    modern: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Display {
    /// Ruta del dispositivo: se mantiene entre reinicios. Es lo que se guarda en la configuración.
    pub id: String,
    pub name: String,
    pub supports_hdr: bool,
    pub hdr_on: bool,
    pub handle: Handle,
}

impl Display {
    pub fn new(id: &str, name: &str, supports_hdr: bool, hdr_on: bool) -> Self {
        Self { id: id.into(), name: name.into(), supports_hdr, hdr_on, handle: Handle::default() }
    }
}

/// Acceso a los monitores. Es un rasgo para poder probar `session` sin tocar la pantalla.
pub trait Displays {
    /// Monitores activos, o el código de error de Windows.
    fn list(&self) -> Result<Vec<Display>, u32>;
    fn set_hdr(&self, display: &Display, on: bool) -> Result<(), u32>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ColorState {
    supports_hdr: bool,
    hdr_on: bool,
}

/// Bit 4: admite HDR. Bit 5: el usuario lo tiene encendido.
fn decode_modern(value: u32) -> ColorState {
    ColorState { supports_hdr: value & (1 << 4) != 0, hdr_on: value & (1 << 5) != 0 }
}

/// Bit 0: admite color avanzado. Bit 1: está encendido. Bit 2: lo encendido es solo color amplio,
/// no HDR: sin mirarlo, un monitor en SDR con gestión de color se leería como HDR.
fn decode_classic(value: u32) -> ColorState {
    ColorState { supports_hdr: value & 1 != 0, hdr_on: value & 2 != 0 && value & 4 == 0 }
}

/// Modelo dentro de la ruta del dispositivo: `\\?\DISPLAY#AUSAA11#5&23e4...` da `AUSAA11`.
pub fn model_of(id: &str) -> Option<&str> {
    let mut parts = id.split('#');
    parts.next()?;
    parts.next().filter(|m| !m.is_empty())
}

/// Busca un monitor guardado. Si el cable se movió de puerto la ruta cambia pero el modelo no: se
/// acepta el mismo modelo solo si hay uno, para no confundir dos monitores iguales.
pub fn resolve<'a>(list: &'a [Display], wanted: &str) -> Option<&'a Display> {
    if let Some(d) = list.iter().find(|d| d.id == wanted) {
        return Some(d);
    }
    let model = model_of(wanted)?;
    let mut same = list.iter().filter(|d| model_of(&d.id) == Some(model));
    match (same.next(), same.next()) {
        (Some(d), None) => Some(d),
        _ => None,
    }
}

fn utf16_text(buf: &[u16]) -> String {
    let len = buf.iter().position(|c| *c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..len]).trim().to_string()
}

fn header<T>(kind: i32, adapter: LUID, target: u32) -> DISPLAYCONFIG_DEVICE_INFO_HEADER {
    DISPLAYCONFIG_DEVICE_INFO_HEADER { r#type: kind, size: size_of::<T>() as u32, adapterId: adapter, id: target }
}

fn active_paths() -> Result<Vec<DISPLAYCONFIG_PATH_INFO>, u32> {
    for _ in 0..QUERY_TRIES {
        let (mut np, mut nm) = (0u32, 0u32);
        let r = unsafe { GetDisplayConfigBufferSizes(QDC_ONLY_ACTIVE_PATHS, &mut np, &mut nm) };
        if r != ERROR_SUCCESS {
            return Err(r);
        }
        let mut paths = vec![unsafe { zeroed::<DISPLAYCONFIG_PATH_INFO>() }; np as usize];
        let mut modes = vec![unsafe { zeroed::<DISPLAYCONFIG_MODE_INFO>() }; nm as usize];
        let r = unsafe {
            QueryDisplayConfig(
                QDC_ONLY_ACTIVE_PATHS,
                &mut np,
                paths.as_mut_ptr(),
                &mut nm,
                modes.as_mut_ptr(),
                null_mut(),
            )
        };
        match r {
            ERROR_SUCCESS => {
                paths.truncate(np as usize);
                return Ok(paths);
            }
            ERROR_INSUFFICIENT_BUFFER => continue,
            e => return Err(e),
        }
    }
    Err(ERROR_INSUFFICIENT_BUFFER)
}

fn target_names(adapter: LUID, target: u32) -> Option<(String, String)> {
    let mut p: DISPLAYCONFIG_TARGET_DEVICE_NAME = unsafe { zeroed() };
    p.header = header::<DISPLAYCONFIG_TARGET_DEVICE_NAME>(DISPLAYCONFIG_DEVICE_INFO_GET_TARGET_NAME, adapter, target);
    if unsafe { DisplayConfigGetDeviceInfo((&raw mut p).cast()) } != ERROR_SUCCESS as i32 {
        return None;
    }
    Some((utf16_text(&p.monitorFriendlyDeviceName), utf16_text(&p.monitorDevicePath)))
}

/// Estado de color y si Windows entiende las peticiones nuevas. `None` si no se pudo leer: nunca
/// debe confundirse con "apagado", porque la deuda de ese monitor se daría por pagada.
fn color_state(adapter: LUID, target: u32) -> Option<(ColorState, bool)> {
    let mut p: ColorInfo2 = unsafe { zeroed() };
    p.header = header::<ColorInfo2>(GET_ADVANCED_COLOR_INFO_2, adapter, target);
    if unsafe { DisplayConfigGetDeviceInfo((&raw mut p).cast()) } == ERROR_SUCCESS as i32 {
        return Some((decode_modern(p.value), true));
    }
    let mut p: ColorInfo = unsafe { zeroed() };
    p.header = header::<ColorInfo>(GET_ADVANCED_COLOR_INFO, adapter, target);
    if unsafe { DisplayConfigGetDeviceInfo((&raw mut p).cast()) } == ERROR_SUCCESS as i32 {
        return Some((decode_classic(p.value), false));
    }
    None
}

/// Los monitores reales, a través de Windows.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemDisplays;

impl Displays for SystemDisplays {
    fn list(&self) -> Result<Vec<Display>, u32> {
        let mut out: Vec<Display> = Vec::new();
        for path in active_paths()? {
            let (adapter, target) = (path.targetInfo.adapterId, path.targetInfo.id);
            let (name, device_path) = target_names(adapter, target).unwrap_or_default();
            let id = if device_path.is_empty() {
                format!("{:08x}{:08x}-{target:08x}", adapter.HighPart, adapter.LowPart)
            } else {
                device_path
            };
            // dos rutas al mismo monitor: pantallas duplicadas
            if out.iter().any(|d| d.id == id) {
                continue;
            }
            // sin estado, es como si ese monitor no estuviera
            let Some((state, modern)) = color_state(adapter, target) else { continue };
            let name = if name.is_empty() { format!("{} {}", tr("Pantalla", "Display"), out.len() + 1) } else { name };
            out.push(Display {
                id,
                name,
                supports_hdr: state.supports_hdr,
                hdr_on: state.hdr_on,
                handle: Handle { adapter_low: adapter.LowPart, adapter_high: adapter.HighPart, target, modern },
            });
        }
        Ok(out)
    }

    fn set_hdr(&self, display: &Display, on: bool) -> Result<(), u32> {
        let h = display.handle;
        // Con Windows moderno nunca se usa la petición clásica como plan B: si la nueva falla, la
        // clásica cambiaría la gestión automática de color del usuario en vez del HDR.
        let kind = if h.modern { SET_HDR_STATE } else { SET_ADVANCED_COLOR_STATE };
        let adapter = LUID { LowPart: h.adapter_low, HighPart: h.adapter_high };
        let p = SetState { header: header::<SetState>(kind, adapter, h.target), value: on as u32 };
        match unsafe { DisplayConfigSetDeviceInfo((&raw const p).cast()) } {
            0 => Ok(()),
            e => Err(e as u32),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PATH: &str = "\\\\?\\DISPLAY#AUSAA11#5&1a2b3c4d&0&UID1234#{e6f07b5f-ee97-4a90-b076-33f57bf4eaa7}";

    /// Valores medidos en un ASUS XG27AQDMES con Windows build 26300.
    #[test]
    fn reads_the_measured_values_of_a_real_monitor() {
        assert_eq!(decode_modern(0xd7), ColorState { supports_hdr: true, hdr_on: false });
        assert_eq!(decode_modern(0xf3), ColorState { supports_hdr: true, hdr_on: true });
    }

    /// Ese monitor en SDR con gestión de color da `0x7` en la API clásica: no es HDR.
    #[test]
    fn wide_color_alone_is_not_hdr() {
        assert_eq!(decode_classic(0x7), ColorState { supports_hdr: true, hdr_on: false });
        assert_eq!(decode_classic(0x3), ColorState { supports_hdr: true, hdr_on: true });
    }

    #[test]
    fn the_packets_have_the_sizes_windows_expects() {
        assert_eq!(size_of::<SetState>(), 24);
        assert_eq!(size_of::<ColorInfo>(), 32);
        assert_eq!(size_of::<ColorInfo2>(), 36);
        assert_eq!(size_of::<DISPLAYCONFIG_PATH_INFO>(), 72);
        assert_eq!(size_of::<DISPLAYCONFIG_MODE_INFO>(), 64);
    }

    #[test]
    fn takes_the_model_from_the_device_path() {
        assert_eq!(model_of(PATH), Some("AUSAA11"));
        assert_eq!(model_of("0000-0001"), None);
    }

    #[test]
    fn finds_a_monitor_moved_to_another_port() {
        let moved = "\\\\?\\DISPLAY#AUSAA11#7&aaaa&0&UID9#{e6f07b5f-ee97-4a90-b076-33f57bf4eaa7}";
        let list =
            [Display::new(moved, "ASUS", true, false), Display::new("\\\\?\\DISPLAY#OTHER#1#{x}", "B", true, false)];
        assert_eq!(resolve(&list, PATH).map(|d| d.id.as_str()), Some(moved));
    }

    #[test]
    fn two_identical_monitors_are_never_guessed() {
        let a = "\\\\?\\DISPLAY#AUSAA11#1#{x}";
        let b = "\\\\?\\DISPLAY#AUSAA11#2#{x}";
        let list = [Display::new(a, "A", true, false), Display::new(b, "B", true, false)];
        assert_eq!(resolve(&list, PATH), None);
        assert_eq!(resolve(&list, b).map(|d| d.name.as_str()), Some("B"));
    }
}
