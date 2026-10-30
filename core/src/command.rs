//! Órdenes que recibe el servicio de la ventana, de los accesos directos y de la bandeja.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Command {
    /// Encender el HDR y abrir una app.
    Launch(String),
    /// Interruptor manual. Sin monitor, todos los que tengan HDR; sin `on`, invierte el estado.
    Manual {
        monitor: Option<String>,
        on: Option<bool>,
    },
    /// Terminar la sesión de una app sin cerrarla.
    End(String),
    /// Renunciar a lo que queda por restaurar.
    Forget,
    Reload,
    Quit,
}

impl Command {
    pub fn encode(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }

    pub fn decode(text: &str) -> Option<Self> {
        serde_json::from_str(text).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_command_survives_the_trip() {
        for c in [
            Command::Launch("a1".into()),
            Command::Manual { monitor: Some("\\\\?\\DISPLAY#X#1".into()), on: Some(true) },
            Command::Manual { monitor: None, on: None },
            Command::End("a1".into()),
            Command::Forget,
            Command::Reload,
            Command::Quit,
        ] {
            assert_eq!(Command::decode(&c.encode()), Some(c));
        }
    }

    #[test]
    fn anything_else_is_ignored() {
        assert_eq!(Command::decode("borrar todo"), None);
        assert_eq!(Command::decode("{\"launch\":5}"), None);
    }
}
