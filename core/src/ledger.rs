//! Guarda en disco el `Ledger` de `session`, y mantiene armada la recuperación de inicio de sesión
//! mientras quede HDR por restaurar.

use crate::session::{Ledger, LedgerStore};
use crate::{log, paths, recovery};

#[derive(Debug, Clone, Copy, Default)]
pub struct FileLedger;

impl FileLedger {
    /// `true` si quedó algo de una sesión anterior.
    pub fn exists() -> bool {
        paths::ledger_file().exists()
    }
}

impl LedgerStore for FileLedger {
    fn load(&self) -> Option<Ledger> {
        let path = paths::ledger_file();
        let text = std::fs::read_to_string(&path).ok()?;
        match serde_json::from_str(&text) {
            Ok(ledger) => Some(ledger),
            Err(e) => {
                log::warn(&format!("ledger.json ilegible, se guarda una copia: {e}"));
                let _ = std::fs::copy(&path, path.with_extension("bad.json"));
                None
            }
        }
    }

    fn save(&mut self, ledger: &Ledger) -> Result<(), String> {
        let path = paths::ledger_file();
        if ledger.turned_on.is_empty() {
            recovery::disarm();
        } else {
            recovery::arm();
        }
        if ledger.turned_on.is_empty() && ledger.holds.is_empty() {
            return match std::fs::remove_file(&path) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
                _ => Ok(()),
            };
        }
        let json = serde_json::to_vec_pretty(ledger).map_err(|e| e.to_string())?;
        paths::write_atomic(&path, &json).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saves_loads_and_removes_itself_when_nothing_is_owed() {
        let mut store = FileLedger;
        let mut ledger = Ledger::default();
        ledger.turned_on.insert("monitor".into());
        store.save(&ledger).unwrap();
        assert_eq!(store.load(), Some(ledger));
        store.save(&Ledger::default()).unwrap();
        assert_eq!(store.load(), None);
    }
}
