//! Inicio con Windows: valor `HDRShift` en `HKCU\...\CurrentVersion\Run` (sin permisos de
//! administrador). Apunta al exe actual con `--service`, para que arranque solo en la bandeja.

use hdrshift_core::registry;

const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
const VALUE: &str = "HDRShift";

pub fn apply(enabled: bool) {
    // con una carpeta de datos de prueba no se toca el inicio de Windows del usuario
    if std::env::var_os("HDRSHIFT_DATA_DIR").is_some() {
        return;
    }
    if !enabled {
        registry::delete_user_value(RUN_KEY, VALUE);
        return;
    }
    let Ok(exe) = std::env::current_exe() else { return };
    registry::set_user_string(RUN_KEY, VALUE, &format!("\"{}\" --service", exe.display()));
}
