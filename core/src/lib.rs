//! Motor de HDRShift: lee y cambia el HDR de los monitores, lleva la cuenta de quién lo necesita y
//! vigila las apps. No tiene interfaz gráfica; lo usan el servicio y la ventana.
//!
//! Todo cambio de HDR pasa por `session::Engine`, para que un solo dueño decida cuándo encenderlo y
//! cuándo devolverlo a como estaba.

pub mod command;
pub mod config;
pub mod display;
pub mod i18n;
pub mod launch;
pub mod ledger;
pub mod log;
pub mod paths;
pub mod recovery;
pub mod registry;
pub mod session;
pub mod shortcut;
pub mod status;
pub mod watch;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const REPO_URL: &str = "https://github.com/Yesid-Robayo/HDRShift";
pub const KOFI_URL: &str = "https://ko-fi.com/yesidrobayo";

/// Segundos desde la época Unix.
pub fn now_secs() -> f64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}
