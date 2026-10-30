//! Lectura y escritura de valores del registro. Escribir solo se hace en `HKEY_CURRENT_USER`, así
//! que nunca hacen falta permisos de administrador.

use std::ptr::{null, null_mut};

use windows_sys::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE, REG_SZ, RRF_RT_REG_DWORD,
    RRF_RT_REG_SZ, RegCloseKey, RegCreateKeyExW, RegDeleteKeyValueW, RegGetValueW, RegSetValueExW,
};

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn read_string(root: HKEY, key: &str, name: &str) -> Option<String> {
    let (key, name) = (wide(key), wide(name));
    let mut buf = [0u16; 512];
    let mut size = size_of_val(&buf) as u32;
    let r = unsafe {
        RegGetValueW(
            root,
            key.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_SZ,
            null_mut(),
            buf.as_mut_ptr() as *mut _,
            &mut size,
        )
    };
    if r != 0 {
        return None;
    }
    let len = buf.iter().position(|c| *c == 0).unwrap_or(buf.len());
    Some(String::from_utf16_lossy(&buf[..len]))
}

/// Cadena de `HKEY_LOCAL_MACHINE`.
pub fn machine_string(key: &str, name: &str) -> Option<String> {
    read_string(HKEY_LOCAL_MACHINE, key, name)
}

/// Cadena de `HKEY_CURRENT_USER`.
pub fn user_string(key: &str, name: &str) -> Option<String> {
    read_string(HKEY_CURRENT_USER, key, name)
}

/// Número de `HKEY_CURRENT_USER`.
pub fn user_dword(key: &str, name: &str) -> Option<u32> {
    let (key, name) = (wide(key), wide(name));
    let mut value = 0u32;
    let mut size = size_of::<u32>() as u32;
    let r = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_DWORD,
            null_mut(),
            &mut value as *mut u32 as *mut _,
            &mut size,
        )
    };
    (r == 0).then_some(value)
}

/// Guarda una cadena en `HKEY_CURRENT_USER`.
pub fn set_user_string(key: &str, name: &str, value: &str) -> bool {
    let (key, name, value) = (wide(key), wide(name), wide(value));
    unsafe {
        let mut h: HKEY = null_mut();
        if RegCreateKeyExW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            0,
            null(),
            REG_OPTION_NON_VOLATILE,
            KEY_SET_VALUE,
            null(),
            &mut h,
            null_mut(),
        ) != 0
        {
            return false;
        }
        let r = RegSetValueExW(h, name.as_ptr(), 0, REG_SZ, value.as_ptr() as *const u8, (value.len() * 2) as u32);
        RegCloseKey(h);
        r == 0
    }
}

pub fn delete_user_value(key: &str, name: &str) {
    let (key, name) = (wide(key), wide(name));
    unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), name.as_ptr()) };
}
