//! Recuperación al iniciar sesión: mientras quede HDR por restaurar hay una entrada en `RunOnce` que
//! lanza `HDRShift.exe --recover`. Windows la borra al ejecutarla, así que no queda nada residente.

const RUN_ONCE: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\RunOnce";
const VALUE: &str = "HDRShiftRestore";

pub fn arm() {
    if cfg!(test) {
        return;
    }
    let Ok(exe) = std::env::current_exe() else { return };
    if !crate::registry::set_user_string(RUN_ONCE, VALUE, &format!("\"{}\" --recover", exe.display())) {
        crate::log::warn("no se pudo programar la recuperación de inicio de sesión");
    }
}

pub fn disarm() {
    if !cfg!(test) {
        crate::registry::delete_user_value(RUN_ONCE, VALUE);
    }
}
