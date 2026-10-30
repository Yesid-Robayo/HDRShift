//! Configuración en `%APPDATA%\HDRShift\config.json`. La escribe la ventana y la lee el servicio
//! (que la recarga cuando la ventana le avisa).

use serde::{Deserialize, Serialize};

use crate::i18n::Language;

const MAX_APPS: usize = 512;
const MAX_NAME: usize = 48;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ThemeChoice {
    /// El mismo que Windows.
    #[default]
    System,
    Dark,
    Light,
}

/// Una app que necesita HDR.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct AppEntry {
    /// Identificador fijo: los accesos directos lo usan, así renombrar la app no los rompe.
    pub id: String,
    pub name: String,
    pub exe: String,
    pub args: String,
    /// Otro proceso que cuenta como esta app, para los lanzadores que abren el juego en otra carpeta.
    pub watch: String,
    /// Monitores donde encender el HDR (`Display::id`).
    pub monitors: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub apps: Vec<AppEntry>,
    pub language: Language,
    pub theme: ThemeChoice,
    /// Seguir en la bandeja sin sesiones activas. Apagado, no queda ningún proceso al terminar.
    pub stay_in_tray: bool,
    /// Encender el HDR al detectar una app abierta, aunque no se haya usado el acceso directo.
    pub auto_detect: bool,
    pub start_with_windows: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            apps: Vec::new(),
            language: Language::Spanish,
            theme: ThemeChoice::System,
            stay_in_tray: false,
            auto_detect: false,
            start_with_windows: false,
        }
    }
}

impl Config {
    pub fn exists() -> bool {
        crate::paths::config_file().exists()
    }

    pub fn load() -> Self {
        Self::try_load().unwrap_or_default()
    }

    /// `None` si el archivo existe pero no se puede leer. Se deja una copia aparte, porque el
    /// próximo guardado lo sobrescribiría y con él se perderían las apps.
    pub fn try_load() -> Option<Self> {
        let path = crate::paths::config_file();
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Some(Self::default()),
            Err(e) => {
                crate::log::warn(&format!("no se pudo leer config.json: {e}"));
                return None;
            }
        };
        match serde_json::from_str::<Config>(&text) {
            Ok(c) => Some(c.sanitized()),
            Err(e) => {
                crate::log::warn(&format!("config.json ilegible, se guarda una copia: {e}"));
                let _ = std::fs::copy(&path, path.with_extension("bad.json"));
                None
            }
        }
    }

    pub fn save(&self) -> std::io::Result<()> {
        let json = serde_json::to_vec_pretty(&self.clone().sanitized()).map_err(std::io::Error::other)?;
        crate::paths::write_atomic(&crate::paths::config_file(), &json)
    }

    pub fn find(&self, id: &str) -> Option<&AppEntry> {
        self.apps.iter().find(|a| a.id == id)
    }

    /// Deja cada valor en un rango sensato (el archivo lo puede editar cualquiera).
    pub fn sanitized(mut self) -> Self {
        self.apps.truncate(MAX_APPS);
        let mut seen = std::collections::HashSet::new();
        for (i, a) in self.apps.iter_mut().enumerate() {
            a.name = a.name.chars().filter(|c| !c.is_control()).take(MAX_NAME).collect();
            // basado en la posición: tiene que salir igual en la ventana y en el servicio
            if a.id.is_empty() || seen.contains(&a.id) {
                a.id = format!("app{i}");
            }
            seen.insert(a.id.clone());
        }
        // las dos opciones necesitan un proceso que siga vivo
        self.stay_in_tray |= self.auto_detect || self.start_with_windows;
        self
    }
}

/// Identificador corto y único.
pub fn new_id() -> String {
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    format!("{:x}", (t as u64) ^ (std::process::id() as u64).rotate_left(40))
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    /// Las pruebas comparten el mismo `config.json`.
    static FILE: Mutex<()> = Mutex::new(());

    fn clean() {
        let _ = std::fs::remove_file(crate::paths::config_file());
        let _ = std::fs::remove_file(crate::paths::config_file().with_extension("bad.json"));
    }

    #[test]
    fn saves_and_loads() {
        let _g = FILE.lock().unwrap();
        clean();
        let cfg = Config {
            apps: vec![AppEntry {
                id: "a1".into(),
                name: "Juego".into(),
                exe: "C:\\j.exe".into(),
                ..Default::default()
            }],
            theme: ThemeChoice::Light,
            ..Default::default()
        };
        cfg.save().unwrap();
        assert_eq!(Config::load(), cfg);
        clean();
    }

    /// Un archivo dañado no debe acabar borrando las apps del usuario.
    #[test]
    fn a_damaged_file_is_kept_aside() {
        let _g = FILE.lock().unwrap();
        clean();
        std::fs::write(crate::paths::config_file(), "{ esto no es json").unwrap();
        assert_eq!(Config::try_load(), None);
        let copy = crate::paths::config_file().with_extension("bad.json");
        assert_eq!(std::fs::read_to_string(copy).unwrap(), "{ esto no es json");
        clean();
    }

    #[test]
    fn a_missing_file_is_the_default() {
        let _g = FILE.lock().unwrap();
        clean();
        assert_eq!(Config::try_load(), Some(Config::default()));
    }

    #[test]
    fn nothing_stays_resident_by_default() {
        let c = Config::default();
        assert!(!c.stay_in_tray && !c.auto_detect && !c.start_with_windows);
    }

    #[test]
    fn options_that_need_the_tray_turn_it_on() {
        assert!(Config { auto_detect: true, ..Default::default() }.sanitized().stay_in_tray);
        assert!(Config { start_with_windows: true, ..Default::default() }.sanitized().stay_in_tray);
    }

    #[test]
    fn repeated_ids_are_fixed_the_same_way_every_time() {
        let app = |id: &str| AppEntry { id: id.into(), ..Default::default() };
        let c = Config { apps: vec![app("x"), app("x"), app("")], ..Default::default() };
        let ids: Vec<String> = c.sanitized().apps.into_iter().map(|a| a.id).collect();
        assert_eq!(ids, ["x", "app1", "app2"]);
    }

    #[test]
    fn names_are_cleaned_and_ids_filled() {
        let name = format!("a\u{7}{}", "b".repeat(100));
        let c = Config { apps: vec![AppEntry { name, ..Default::default() }], ..Default::default() }.sanitized();
        assert_eq!(c.apps[0].name.chars().count(), 48);
        assert!(!c.apps[0].name.contains('\u{7}'));
        assert!(!c.apps[0].id.is_empty());
    }
}
