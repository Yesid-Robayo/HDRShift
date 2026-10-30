//! Estado que publica el servicio para la ventana, en `status.json`.

use serde::{Deserialize, Serialize};

use crate::session::Hold;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Snapshot {
    pub holds: Vec<Hold>,
    /// Monitores que HDRShift aún no pudo devolver a como estaban.
    pub pending: Vec<String>,
}

impl Snapshot {
    pub fn load() -> Option<Self> {
        serde_json::from_slice(&std::fs::read(crate::paths::status_file()).ok()?).ok()
    }

    pub fn save(&self) -> std::io::Result<()> {
        let json = serde_json::to_vec_pretty(self).map_err(std::io::Error::other)?;
        crate::paths::write_atomic(&crate::paths::status_file(), &json)
    }

    pub fn clear() {
        let _ = std::fs::remove_file(crate::paths::status_file());
    }
}
