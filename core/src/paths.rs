//! Rutas de datos: todo vive en `%APPDATA%\HDRShift`, salvo que exista una carpeta `HDRShiftData`
//! junto al exe (modo portable) o se defina `HDRSHIFT_DATA_DIR` (útil para pruebas).

use std::io::Write;
use std::path::PathBuf;

/// Todo lo que HDRShift puede dejar en su carpeta de datos.
const OWN_FILES: [&str; 10] = [
    "config.json",
    "config.bad.json",
    "ledger.json",
    "ledger.bad.json",
    "status.json",
    "hdrshift.log",
    "hdrshift.old.log",
    "config.tmp",
    "ledger.tmp",
    "status.tmp",
];

/// Dónde estarían los datos en modo portable.
pub fn portable_dir() -> Option<PathBuf> {
    Some(std::env::current_exe().ok()?.parent()?.join("HDRShiftData"))
}

pub fn is_portable() -> bool {
    portable_dir().is_some_and(|d| d == data_dir())
}

/// Carpeta de datos normal, aunque ahora se use la portable.
fn standard_dir() -> Option<PathBuf> {
    Some(PathBuf::from(std::env::var_os("APPDATA")?).join("HDRShift"))
}

pub fn data_dir() -> PathBuf {
    // las pruebas automáticas nunca deben tocar los datos reales del usuario
    #[cfg(test)]
    let dir = std::env::temp_dir().join("hdrshift-tests");
    #[cfg(not(test))]
    let dir = match std::env::var_os("HDRSHIFT_DATA_DIR") {
        Some(d) => PathBuf::from(d),
        None => portable_dir()
            .filter(|d| d.is_dir())
            .or_else(standard_dir)
            .unwrap_or_else(|| std::env::temp_dir().join("HDRShift")),
    };
    let _ = std::fs::create_dir_all(&dir);
    dir
}

pub fn config_file() -> PathBuf {
    data_dir().join("config.json")
}

pub fn status_file() -> PathBuf {
    data_dir().join("status.json")
}

pub fn ledger_file() -> PathBuf {
    data_dir().join("ledger.json")
}

pub fn log_file() -> PathBuf {
    data_dir().join("hdrshift.log")
}

/// Escribe un archivo de forma atómica (temporal + renombrar), para que otro proceso nunca
/// lea un JSON a medias. El temporal se fuerza al disco antes de renombrar: si no, un corte de
/// luz podría dejar el archivo nuevo vacío.
pub fn write_atomic(path: &std::path::Path, data: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    let mut file = std::fs::File::create(&tmp)?;
    file.write_all(data)?;
    file.sync_all()?;
    drop(file);
    std::fs::rename(&tmp, path)
}

/// Borra los datos de la app. Solo sus propios archivos y sin forzar la carpeta: si esa carpeta la
/// eligió alguien con `HDRSHIFT_DATA_DIR`, lo que no sea de HDRShift se queda donde estaba.
pub fn remove_data() {
    for dir in [Some(data_dir()), standard_dir()].into_iter().flatten() {
        for name in OWN_FILES {
            let _ = std::fs::remove_file(dir.join(name));
        }
        let _ = std::fs::remove_dir(dir);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn tests_never_use_the_real_data_folder() {
        assert!(super::data_dir().starts_with(std::env::temp_dir()));
    }
}
