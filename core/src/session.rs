//! Quién necesita el HDR y cuándo devolverlo a como estaba.
//!
//! Hay un solo `Engine`, en el servicio. Cada app abierta o interruptor manual es una `Hold` sobre
//! sus monitores. El HDR que encendió HDRShift se apaga cuando se va la última `Hold` que lo usa, y
//! el que ya estaba encendido no se toca nunca.
//!
//! `Ledger::turned_on` es la lista de deudas: se guarda **antes** de encender un monitor y una
//! entrada solo se borra tras comprobar que el HDR está realmente apagado. Así ningún fallo, cierre
//! brusco o monitor desconectado puede hacer que se olvide lo que hay que restaurar.

use std::collections::BTreeSet;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::display::{Display, Displays, resolve};
use crate::log;

/// Código usado cuando Windows aceptó el cambio pero el estado leído después no coincide.
pub const UNVERIFIED: u32 = u32::MAX;
/// Código usado cuando no se pudo guardar la deuda: sin eso no se toca el HDR.
pub const NOT_SAVED: u32 = u32::MAX - 1;

const VERIFY_TRIES: u32 = 10;
const VERIFY_GAP: Duration = Duration::from_millis(if cfg!(test) { 0 } else { 100 });

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HoldKind {
    /// Una app, por su id de configuración.
    App(String),
    /// El interruptor manual de un monitor, por su id.
    Manual(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Hold {
    pub kind: HoldKind,
    pub label: String,
    pub monitors: BTreeSet<String>,
    pub started_at: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Ledger {
    /// Monitores que encendió HDRShift y que debe apagar.
    pub turned_on: BTreeSet<String>,
    pub holds: Vec<Hold>,
}

/// Dónde se guarda el `Ledger`. Es un rasgo para poder probar sin disco ni registro.
pub trait LedgerStore {
    fn load(&self) -> Option<Ledger>;
    fn save(&mut self, ledger: &Ledger) -> Result<(), String>;
}

/// Qué pasó con un monitor al pedir HDR.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MonitorResult {
    TurnedOn,
    /// Ya estaba encendido: sigue así al terminar si no fue HDRShift quien lo encendió.
    AlreadyOn,
    NoHdr,
    NotFound,
    /// Código de error de Windows, o `UNVERIFIED`.
    Failed(u32),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonitorOutcome {
    pub name: String,
    pub result: MonitorResult,
}

pub struct Engine<D: Displays, S: LedgerStore> {
    displays: D,
    store: S,
    ledger: Ledger,
}

impl<D: Displays, S: LedgerStore> Engine<D, S> {
    pub fn new(displays: D, store: S) -> Self {
        let ledger = store.load().unwrap_or_default();
        Self { displays, store, ledger }
    }

    pub fn holds(&self) -> &[Hold] {
        &self.ledger.holds
    }

    pub fn has_hold(&self, kind: &HoldKind) -> bool {
        self.ledger.holds.iter().any(|h| h.kind == *kind)
    }

    /// Monitores que HDRShift debe apagar y nadie necesita ya.
    pub fn pending(&self) -> Vec<String> {
        self.ledger.turned_on.iter().filter(|id| !self.needed(id)).cloned().collect()
    }

    pub fn is_idle(&self) -> bool {
        self.ledger.holds.is_empty() && self.ledger.turned_on.is_empty()
    }

    fn needed(&self, id: &str) -> bool {
        self.ledger.holds.iter().any(|h| h.monitors.contains(id))
    }

    fn persist(&mut self) -> bool {
        let saved = self.store.save(&self.ledger);
        if let Err(e) = &saved {
            log::warn(&format!("no se pudo guardar el estado del HDR: {e}"));
        }
        saved.is_ok()
    }

    /// Cambia el HDR y comprueba que quedó como se pidió. Solo cuenta el estado leído, nunca la
    /// respuesta: con la pantalla apagada o la sesión cerrándose, Windows contesta "acceso denegado"
    /// y aun así aplica el cambio (medido en un ASUS XG27AQDMES con la build 26300).
    fn switch(&self, display: &Display, on: bool) -> Result<(), u32> {
        let answer = self.displays.set_hdr(display, on);
        for attempt in 0..VERIFY_TRIES {
            if attempt > 0 {
                std::thread::sleep(VERIFY_GAP);
            }
            if let Ok(list) = self.displays.list()
                && list.iter().find(|d| d.id == display.id).is_some_and(|d| d.hdr_on == on)
            {
                return Ok(());
            }
        }
        Err(answer.err().unwrap_or(UNVERIFIED))
    }

    /// Al arrancar el servicio: las `Hold` guardadas son de un proceso que ya no existe. Se conservan
    /// las que `keep` confirme (su app sigue abierta) y se restaura el resto.
    pub fn recover(&mut self, keep: impl Fn(&Hold) -> bool) {
        let before = self.ledger.holds.len();
        self.ledger.holds.retain(|h| keep(h));
        if before > self.ledger.holds.len() || !self.ledger.turned_on.is_empty() {
            log::warn("quedó HDR por restaurar de una sesión que no se cerró bien");
        }
        self.settle();
    }

    /// Enciende el HDR en `wanted` para `kind`. Pedirlo otra vez para lo mismo no cambia nada: el
    /// estado original no se pierde aunque se abra dos veces la misma app.
    pub fn hold(&mut self, kind: HoldKind, label: &str, wanted: &[String]) -> Vec<MonitorOutcome> {
        let list = match self.displays.list() {
            Ok(list) => list,
            Err(code) => {
                log::error(&format!("no se pudieron leer los monitores: código {code}"));
                self.add_hold(kind, label, BTreeSet::new());
                let failed = |_: &String| MonitorOutcome { name: String::new(), result: MonitorResult::Failed(code) };
                return wanted.iter().map(failed).collect();
            }
        };
        let mut outcomes = Vec::with_capacity(wanted.len());
        let mut monitors = BTreeSet::new();
        for id in wanted {
            let Some(display) = resolve(&list, id) else {
                log::warn(&format!("{label}: monitor no encontrado"));
                let result = MonitorResult::NotFound;
                outcomes.push(MonitorOutcome { name: String::new(), result });
                continue;
            };
            let result = self.turn_on(display);
            if matches!(result, MonitorResult::TurnedOn | MonitorResult::AlreadyOn) {
                monitors.insert(display.id.clone());
            }
            outcomes.push(MonitorOutcome { name: display.name.clone(), result });
        }
        self.add_hold(kind, label, monitors);
        self.settle();
        outcomes
    }

    fn turn_on(&mut self, display: &Display) -> MonitorResult {
        if !display.supports_hdr {
            return MonitorResult::NoHdr;
        }
        if display.hdr_on {
            log::info(&format!("el HDR ya estaba encendido en {}", display.name));
            return MonitorResult::AlreadyOn;
        }
        // Primero la deuda, después el cambio: si el proceso muere en medio, la recuperación sabe qué hacer.
        self.ledger.turned_on.insert(display.id.clone());
        if !self.persist() {
            self.ledger.turned_on.remove(&display.id);
            return MonitorResult::Failed(NOT_SAVED);
        }
        match self.switch(display, true) {
            Ok(()) => {
                log::info(&format!("HDR encendido en {}", display.name));
                MonitorResult::TurnedOn
            }
            Err(code) => {
                // la deuda se queda: `settle` la borra si sigue apagado y lo apaga si no
                log::error(&format!("no se pudo encender el HDR en {}: código {code}", display.name));
                MonitorResult::Failed(code)
            }
        }
    }

    fn add_hold(&mut self, kind: HoldKind, label: &str, monitors: BTreeSet<String>) {
        match self.ledger.holds.iter_mut().find(|h| h.kind == kind) {
            Some(h) => h.monitors.extend(monitors),
            None => self.ledger.holds.push(Hold {
                kind,
                label: label.to_string(),
                monitors,
                started_at: crate::now_secs() as u64,
            }),
        }
        self.persist();
    }

    /// Quita una `Hold` y apaga lo que ya nadie necesita.
    pub fn release(&mut self, kind: &HoldKind) {
        self.ledger.holds.retain(|h| h.kind != *kind);
        self.settle();
    }

    /// Termina todo (salir, cerrar la sesión de Windows).
    pub fn release_all(&mut self) {
        self.ledger.holds.clear();
        self.settle();
    }

    /// Vuelve a intentar lo pendiente.
    pub fn retry(&mut self) {
        if !self.pending().is_empty() {
            self.settle();
        }
    }

    /// El usuario apaga a mano un HDR que no encendió HDRShift.
    pub fn force_off(&mut self, id: &str) -> Result<(), u32> {
        let list = self.displays.list()?;
        let Some(display) = list.iter().find(|d| d.id == id) else { return Ok(()) };
        if display.hdr_on {
            self.switch(display, false)?;
        }
        self.ledger.turned_on.remove(id);
        self.persist();
        Ok(())
    }

    /// El usuario renuncia a lo pendiente, por ejemplo porque ese monitor ya no existe.
    pub fn forget_pending(&mut self) {
        for id in self.pending() {
            self.ledger.turned_on.remove(&id);
        }
        self.persist();
    }

    /// Apaga lo que HDRShift encendió y nadie necesita. Lo que no se pueda sigue en la deuda.
    fn settle(&mut self) {
        let pending = self.pending();
        if pending.is_empty() {
            self.persist();
            return;
        }
        let list = match self.displays.list() {
            Ok(list) => list,
            Err(code) => {
                log::warn(&format!("no se pudo restaurar el HDR, se reintentará: código {code}"));
                self.persist();
                return;
            }
        };
        for id in pending {
            // Por modelo se encuentra un monitor cuyo cable cambió de puerto, pero solo sirve para apagar:
            // si el de la deuda sigue sin conectar, otro igual apagado no prueba nada, y el que usa otra
            // sesión no se toca.
            let exact = list.iter().find(|d| d.id == id);
            let same_model = || resolve(&list, &id).filter(|d| d.hdr_on && !self.needed(&d.id));
            let Some(display) = exact.or_else(same_model) else {
                log::warn("no se pudo restaurar el HDR: el monitor no está disponible, se reintentará");
                continue;
            };
            let done = !display.hdr_on
                || self
                    .switch(display, false)
                    .inspect_err(|code| {
                        log::warn(&format!(
                            "no se pudo apagar el HDR en {}, se reintentará: código {code}",
                            display.name
                        ));
                    })
                    .is_ok();
            if done {
                log::info(&format!("HDR restaurado en {}", display.name));
                self.ledger.turned_on.remove(&id);
            }
        }
        self.persist();
    }
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    use super::*;

    const A: &str = "\\\\?\\DISPLAY#AUSAA11#1#{x}";
    const B: &str = "\\\\?\\DISPLAY#LGD0001#2#{x}";
    /// Dos monitores iguales.
    const TWIN_1: &str = "\\\\?\\DISPLAY#TWIN001#1#{x}";
    const TWIN_2: &str = "\\\\?\\DISPLAY#TWIN001#2#{x}";

    #[derive(Default)]
    struct World {
        on: RefCell<BTreeSet<String>>,
        hidden: RefCell<BTreeSet<String>>,
        no_hdr: RefCell<BTreeSet<String>>,
        fail_set: Cell<Option<u32>>,
        fail_list: Cell<Option<u32>>,
        fail_save: Cell<bool>,
        /// Windows acepta el cambio pero no lo aplica.
        ignore_set: Cell<bool>,
        /// Windows contesta con error pero aplica el cambio, como con la pantalla apagada.
        applied_with_error: Cell<Option<u32>>,
        saved: RefCell<Option<Ledger>>,
        /// Copia de la deuda guardada en el momento de cada cambio de HDR.
        saved_at_switch: RefCell<Vec<Ledger>>,
    }

    #[derive(Clone)]
    struct Fake(Rc<World>);

    impl Displays for Fake {
        fn list(&self) -> Result<Vec<Display>, u32> {
            if let Some(code) = self.0.fail_list.get() {
                return Err(code);
            }
            let all = [A, B, TWIN_1, TWIN_2];
            let shown = all.iter().filter(|id| !self.0.hidden.borrow().contains(**id));
            Ok(shown
                .map(|id| {
                    let supports = !self.0.no_hdr.borrow().contains(*id);
                    Display::new(id, "Prueba", supports, self.0.on.borrow().contains(*id))
                })
                .collect())
        }

        fn set_hdr(&self, display: &Display, on: bool) -> Result<(), u32> {
            self.0.saved_at_switch.borrow_mut().push(self.0.saved.borrow().clone().unwrap_or_default());
            if let Some(code) = self.0.fail_set.get() {
                return Err(code);
            }
            if self.0.ignore_set.get() {
                return Ok(());
            }
            if on {
                self.0.on.borrow_mut().insert(display.id.clone());
            } else {
                self.0.on.borrow_mut().remove(&display.id);
            }
            self.0.applied_with_error.get().map_or(Ok(()), Err)
        }
    }

    impl LedgerStore for Fake {
        fn load(&self) -> Option<Ledger> {
            self.0.saved.borrow().clone()
        }

        fn save(&mut self, ledger: &Ledger) -> Result<(), String> {
            if self.0.fail_save.get() {
                return Err("disco lleno".into());
            }
            *self.0.saved.borrow_mut() = Some(ledger.clone());
            Ok(())
        }
    }

    fn world() -> Fake {
        Fake(Rc::new(World::default()))
    }

    fn engine(w: &Fake) -> Engine<Fake, Fake> {
        Engine::new(w.clone(), w.clone())
    }

    fn app(id: &str) -> HoldKind {
        HoldKind::App(id.into())
    }

    fn on(w: &Fake, id: &str) -> bool {
        w.0.on.borrow().contains(id)
    }

    fn ids(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn turns_on_and_restores() {
        let w = world();
        let mut e = engine(&w);
        let report = e.hold(app("juego"), "Juego", &ids(&[A]));
        assert_eq!(report[0].result, MonitorResult::TurnedOn);
        assert!(on(&w, A));
        e.release(&app("juego"));
        assert!(!on(&w, A));
        assert!(e.is_idle());
    }

    /// Dos apps a la vez: la primera en cerrarse no le quita el HDR a la otra.
    #[test]
    fn hdr_stays_until_the_last_app_closes() {
        let w = world();
        let mut e = engine(&w);
        e.hold(app("a"), "A", &ids(&[A]));
        e.hold(app("b"), "B", &ids(&[A]));
        e.release(&app("a"));
        assert!(on(&w, A), "la segunda app sigue abierta");
        e.release(&app("b"));
        assert!(!on(&w, A));
    }

    /// Otro programa encendió el HDR y lo apagó al cerrarse la app. Si ella vuelve hay que encenderlo,
    /// y esta vez es HDRShift quien debe apagarlo.
    #[test]
    fn asking_again_recovers_the_hdr_another_program_turned_off() {
        let w = world();
        w.0.on.borrow_mut().insert(A.to_string());
        let mut e = engine(&w);
        assert_eq!(e.hold(app("juego"), "Juego", &ids(&[A]))[0].result, MonitorResult::AlreadyOn);
        w.0.on.borrow_mut().remove(A);
        assert_eq!(e.hold(app("juego"), "Juego", &ids(&[A]))[0].result, MonitorResult::TurnedOn);
        assert!(on(&w, A));
        e.release(&app("juego"));
        assert!(!on(&w, A));
        assert!(e.is_idle());
    }

    /// El caso real del registro: abrir otra vez la misma app no convierte "encendido" en el estado original.
    #[test]
    fn opening_the_same_app_again_keeps_the_true_original() {
        let w = world();
        let mut e = engine(&w);
        e.hold(app("a"), "A", &ids(&[A]));
        let second = e.hold(app("a"), "A", &ids(&[A]));
        assert_eq!(second[0].result, MonitorResult::AlreadyOn);
        assert_eq!(e.holds().len(), 1);
        e.release(&app("a"));
        assert!(!on(&w, A), "debe volver a apagado");
    }

    #[test]
    fn hdr_that_was_already_on_is_left_on() {
        let w = world();
        w.0.on.borrow_mut().insert(A.into());
        let mut e = engine(&w);
        let report = e.hold(app("a"), "A", &ids(&[A]));
        assert_eq!(report[0].result, MonitorResult::AlreadyOn);
        e.release(&app("a"));
        assert!(on(&w, A), "HDRShift no lo encendió: no debe apagarlo");
        assert!(e.is_idle());
    }

    /// Falla al restaurar, como al cerrar la sesión de Windows: la deuda no se pierde.
    #[test]
    fn a_failed_restore_stays_pending_and_is_retried() {
        let w = world();
        let mut e = engine(&w);
        e.hold(app("a"), "A", &ids(&[A]));
        w.0.fail_set.set(Some(5));
        e.release(&app("a"));
        assert_eq!(e.pending(), ids(&[A]));
        assert_eq!(w.0.saved.borrow().as_ref().unwrap().turned_on.len(), 1, "debe seguir en disco");
        e.retry();
        assert_eq!(e.pending(), ids(&[A]));
        w.0.fail_set.set(None);
        e.retry();
        assert!(e.pending().is_empty());
        assert!(!on(&w, A));
    }

    /// Lo medido con la pantalla apagada: "acceso denegado" y el cambio aplicado. Si se creyera la
    /// respuesta, el HDR quedaría encendido sin que nadie recuerde apagarlo.
    #[test]
    fn an_error_answer_does_not_matter_if_the_change_was_applied() {
        let w = world();
        w.0.applied_with_error.set(Some(5));
        let mut e = engine(&w);
        let report = e.hold(app("a"), "A", &ids(&[A]));
        assert_eq!(report[0].result, MonitorResult::TurnedOn);
        assert!(on(&w, A));
        e.release(&app("a"));
        assert!(!on(&w, A));
        assert!(e.is_idle());
    }

    /// Si encender falla de verdad pero el HDR acabó encendido, no puede quedar así sin dueño.
    #[test]
    fn a_failed_activation_that_left_hdr_on_is_cleaned_up() {
        let w = world();
        let mut e = engine(&w);
        // el primer intento no se comprueba como encendido y el estado cambia después
        w.0.ignore_set.set(true);
        let report = e.hold(app("a"), "A", &ids(&[A]));
        assert_eq!(report[0].result, MonitorResult::Failed(UNVERIFIED));
        assert!(e.ledger.turned_on.is_empty(), "seguía apagado: no hay nada que deber");
        assert!(!on(&w, A));
    }

    /// Windows dice que sí pero el estado no cambió: no se cuenta como restaurado.
    #[test]
    fn a_change_that_does_not_stick_is_not_believed() {
        let w = world();
        let mut e = engine(&w);
        e.hold(app("a"), "A", &ids(&[A]));
        w.0.ignore_set.set(true);
        e.release(&app("a"));
        assert_eq!(e.pending(), ids(&[A]));
    }

    #[test]
    fn a_monitor_missing_at_restore_is_restored_when_it_returns() {
        let w = world();
        let mut e = engine(&w);
        e.hold(app("a"), "A", &ids(&[A]));
        w.0.hidden.borrow_mut().insert(A.into());
        e.release(&app("a"));
        assert_eq!(e.pending(), ids(&[A]));
        w.0.hidden.borrow_mut().clear();
        e.retry();
        assert!(e.pending().is_empty());
        assert!(!on(&w, A));
    }

    /// La deuda se apuntó con la ruta vieja y el cable cambió de puerto antes de pagarla.
    #[test]
    fn a_debt_is_paid_even_if_the_monitor_changed_port_meanwhile() {
        let w = world();
        let old_path = "\\\\?\\DISPLAY#AUSAA11#9#{x}";
        w.0.on.borrow_mut().insert(A.into());
        let mut ledger = Ledger::default();
        ledger.turned_on.insert(old_path.to_string());
        *w.0.saved.borrow_mut() = Some(ledger);
        let mut e = engine(&w);
        e.recover(|_| false);
        assert!(!on(&w, A));
        assert!(e.is_idle());
    }

    /// Debía apagar el primero de dos monitores iguales y solo está conectado el segundo, que otra
    /// app está usando: no se le quita el HDR.
    #[test]
    fn a_debt_never_turns_off_an_identical_monitor_in_use() {
        let w = world();
        let mut e = engine(&w);
        e.hold(app("a"), "A", &ids(&[TWIN_1]));
        e.hold(app("b"), "B", &ids(&[TWIN_2]));
        w.0.hidden.borrow_mut().insert(TWIN_1.into());
        e.release(&app("a"));
        assert!(on(&w, TWIN_2), "la otra app sigue abierta");
        assert_eq!(e.pending(), ids(&[TWIN_1]));
    }

    /// Un monitor igual apagado no demuestra que se pagó la deuda del que no está.
    #[test]
    fn an_identical_monitor_turned_off_does_not_pay_the_debt() {
        let w = world();
        let mut e = engine(&w);
        e.hold(app("a"), "A", &ids(&[TWIN_1]));
        w.0.hidden.borrow_mut().insert(TWIN_1.into());
        e.release(&app("a"));
        assert_eq!(e.pending(), ids(&[TWIN_1]));
        w.0.hidden.borrow_mut().clear();
        e.retry();
        assert!(!on(&w, TWIN_1));
        assert!(e.is_idle());
    }

    #[test]
    fn the_user_can_forget_a_monitor_that_no_longer_exists() {
        let w = world();
        let mut e = engine(&w);
        e.hold(app("a"), "A", &ids(&[A]));
        w.0.hidden.borrow_mut().insert(A.into());
        e.release(&app("a"));
        e.forget_pending();
        assert!(e.is_idle());
        assert!(w.0.saved.borrow().as_ref().unwrap().turned_on.is_empty());
    }

    /// Sin poder escribir la deuda no se enciende nada: nadie recordaría apagarlo.
    #[test]
    fn hdr_is_not_touched_if_the_debt_cannot_be_saved() {
        let w = world();
        w.0.fail_save.set(true);
        let mut e = engine(&w);
        let report = e.hold(app("a"), "A", &ids(&[A]));
        assert_eq!(report[0].result, MonitorResult::Failed(NOT_SAVED));
        assert!(!on(&w, A));
    }

    #[test]
    fn listing_failure_at_restore_keeps_the_debt() {
        let w = world();
        let mut e = engine(&w);
        e.hold(app("a"), "A", &ids(&[A]));
        w.0.fail_list.set(Some(6));
        e.release(&app("a"));
        assert_eq!(e.pending(), ids(&[A]));
    }

    /// El proceso muere con el HDR encendido; lo siguiente que ocurre es otra app. Debe restaurar
    /// primero, para que la app nueva no tome "encendido" como estado original.
    #[test]
    fn a_session_after_a_crash_keeps_the_true_original() {
        let w = world();
        engine(&w).hold(app("a"), "A", &ids(&[A]));
        let mut next = engine(&w);
        next.recover(|_| false);
        assert!(!on(&w, A));
        next.hold(app("b"), "B", &ids(&[A]));
        next.release(&app("b"));
        assert!(!on(&w, A));
        assert!(next.is_idle());
    }

    #[test]
    fn an_app_still_open_after_a_crash_keeps_its_hdr() {
        let w = world();
        engine(&w).hold(app("a"), "A", &ids(&[A]));
        let mut next = engine(&w);
        next.recover(|h| h.kind == HoldKind::App("a".into()));
        assert!(on(&w, A));
        next.release(&app("a"));
        assert!(!on(&w, A));
    }

    /// La deuda queda escrita antes de tocar el HDR: si el proceso muere justo ahí, no se olvida.
    #[test]
    fn the_debt_is_saved_before_changing_hdr() {
        let w = world();
        engine(&w).hold(app("a"), "A", &ids(&[A]));
        assert!(w.0.saved_at_switch.borrow()[0].turned_on.contains(A));
    }

    #[test]
    fn a_failed_activation_is_reported_and_leaves_no_debt() {
        let w = world();
        w.0.fail_set.set(Some(5));
        let mut e = engine(&w);
        let report = e.hold(app("a"), "A", &ids(&[A]));
        assert_eq!(report[0].result, MonitorResult::Failed(5));
        assert!(e.pending().is_empty());
        assert!(e.ledger.turned_on.is_empty());
    }

    #[test]
    fn monitors_without_hdr_or_missing_are_reported() {
        let w = world();
        w.0.no_hdr.borrow_mut().insert(A.into());
        w.0.hidden.borrow_mut().insert(B.into());
        let mut e = engine(&w);
        let report = e.hold(app("a"), "A", &ids(&[A, B]));
        assert_eq!(report[0].result, MonitorResult::NoHdr);
        assert_eq!(report[1].result, MonitorResult::NotFound);
    }

    #[test]
    fn a_monitor_moved_to_another_port_still_works() {
        let w = world();
        let mut e = engine(&w);
        let old_path = "\\\\?\\DISPLAY#AUSAA11#9#{x}";
        let report = e.hold(app("a"), "A", &ids(&[old_path]));
        assert_eq!(report[0].result, MonitorResult::TurnedOn);
        assert!(on(&w, A));
    }

    #[test]
    fn if_the_user_already_turned_it_off_there_is_nothing_to_do() {
        let w = world();
        let mut e = engine(&w);
        e.hold(app("a"), "A", &ids(&[A]));
        w.0.on.borrow_mut().clear();
        w.0.fail_set.set(Some(5));
        e.release(&app("a"));
        assert!(e.is_idle());
    }

    #[test]
    fn the_manual_switch_outlives_an_app() {
        let w = world();
        let mut e = engine(&w);
        e.hold(HoldKind::Manual(A.into()), "Manual", &ids(&[A]));
        e.hold(app("a"), "A", &ids(&[A]));
        e.release(&app("a"));
        assert!(on(&w, A));
        e.release(&HoldKind::Manual(A.into()));
        assert!(!on(&w, A));
    }

    #[test]
    fn the_user_can_turn_off_hdr_it_did_not_turn_on() {
        let w = world();
        w.0.on.borrow_mut().insert(A.into());
        let mut e = engine(&w);
        assert_eq!(e.force_off(A), Ok(()));
        assert!(!on(&w, A));
    }

    #[test]
    fn release_all_restores_every_monitor() {
        let w = world();
        let mut e = engine(&w);
        e.hold(app("a"), "A", &ids(&[A]));
        e.hold(app("b"), "B", &ids(&[B]));
        e.release_all();
        assert!(!on(&w, A) && !on(&w, B));
        assert!(e.is_idle());
    }

    /// Sigue vivo el registro de la `Hold` si no se pudo leer ningún monitor: la app debe abrirse igual.
    #[test]
    fn an_app_opens_even_if_the_monitors_cannot_be_read() {
        let w = world();
        w.0.fail_list.set(Some(5));
        let mut e = engine(&w);
        let report = e.hold(app("a"), "A", &ids(&[A]));
        assert_eq!(report[0].result, MonitorResult::Failed(5));
        assert!(e.has_hold(&app("a")));
    }
}
