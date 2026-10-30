//! HDRShift: HDR automático para cada app en Windows.
//!
//! Un solo exe con varios papeles:
//! - sin argumentos: la ventana, un proceso aparte que solo existe mientras está abierta.
//! - `--launch <id>`: enciende el HDR y abre una app (lo que ejecutan los accesos directos).
//! - `--toggle`: interruptor manual del HDR.
//! - `--service`: arranca el servicio de bandeja sin ninguna orden.
//! - `--recover`: devuelve el HDR a como estaba tras un cierre inesperado (lo lanza Windows).
//! - `--reopen`: abre la ventana esperando a que se cierre la anterior.
//!
//! El servicio es el único dueño del HDR. Sale solo cuando ya no hay nada que vigilar.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod autostart;
mod icon;
mod ipc;
mod service;
mod tray;
mod ui;

use hdrshift_core::command::Command;

fn main() {
    // Un pánico cierra el proceso (panic = "abort"): que al menos quede en el log para diagnosticarlo.
    std::panic::set_hook(Box::new(|info| hdrshift_core::log::error(&format!("fallo inesperado: {info}"))));
    let args: Vec<String> = std::env::args().skip(1).collect();
    let has = |flag: &str| args.iter().any(|a| a == flag);
    let launch = args.iter().position(|a| a == "--launch").and_then(|i| args.get(i + 1));
    if has("--recover") {
        service::recover();
    } else if has("--service") {
        service::run();
    } else if has("--toggle") {
        service::command(Command::Manual { monitor: None, on: None });
    } else if let Some(id) = launch {
        service::command(Command::Launch(id.clone()));
    } else {
        ui::run(has("--reopen"));
    }
}
