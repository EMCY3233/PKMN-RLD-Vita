//! Detector de tirones: qué ha costado en un fotograma lento (cargas, compilaciones, texturas).
//! Siempre activo y barato: solo mide funciones que cargan cosas, no cada llamada.

use std::{cell::RefCell, time::Instant};

thread_local! {
    static LOAD_NS: std::cell::Cell<u64> = std::cell::Cell::new(0);
    static ITEMS: RefCell<Vec<(String, f64)>> = RefCell::new(Vec::new());
    static REDRAWS: std::cell::Cell<u32> = std::cell::Cell::new(0);
}

/// screen_redraw dentro de un fotograma (bucles modales del juego: diálogos, transiciones)
pub fn count_redraw() {
    REDRAWS.with(|c| c.set(c.get() + 1));
}

pub fn take_redraws() -> u32 {
    REDRAWS.with(|c| c.replace(0))
}

/// Umbral para apuntar una operación concreta (ms)
fn item_ms() -> f64 {
    if cfg!(target_os = "vita") { 8.0 } else { 2.0 }
}

pub struct LoadTimer {
    name: &'static str,
    t: Instant,
}

impl LoadTimer {
    #[inline]
    pub fn new(name: &'static str) -> Self {
        Self { name, t: Instant::now() }
    }
}

impl Drop for LoadTimer {
    fn drop(&mut self) {
        let ns = self.t.elapsed().as_nanos() as u64;
        LOAD_NS.with(|c| c.set(c.get() + ns));
        note(self.name, ns as f64 / 1e6);
    }
}

/// Apunta una operación si pasa del umbral
pub fn note(what: &str, ms: f64) {
    if ms >= item_ms() {
        ITEMS.with(|v| {
            let mut v = v.borrow_mut();
            if v.len() < 32 {
                v.push((what.to_string(), ms));
            }
        });
    }
}

pub fn add_load_ns(ns: u64) {
    LOAD_NS.with(|c| c.set(c.get() + ns));
}

/// (ms de cargas, operaciones lentas) desde la última llamada
pub fn take() -> (f64, Vec<(String, f64)>) {
    let ns = LOAD_NS.with(|c| c.replace(0));
    let items = ITEMS.with(|v| std::mem::take(&mut *v.borrow_mut()));
    (ns as f64 / 1e6, items)
}
