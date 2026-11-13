//! Lógica de los botones de la Vita → teclas del juego, sin depender de la Vita (se prueba en PC).
//!
//! Reglas:
//! - Capa base: cada botón mantiene su tecla mientras esté pulsado.
//! - Mantener L, R o SELECT y pulsar otro botón: la tecla de esa capa.
//! - L, R y SELECT solos (sin usar su capa):
//!   - toque corto = su tecla durante unos fotogramas (L = SHIFT, SELECT = ESPACIO);
//!   - mantenidos más de HOLD_FRAMES = su tecla mientras sigan pulsados (R = CTRL para acelerar,
//!     L = SHIFT para ver los ataques; el juego las mira con keyboard_check, hay que mantenerlas).
//!   Si después se usa la capa, esa tecla se suelta antes de pulsar la de la capa.
//! - [combos]: al pulsar el último botón con los demás mantenidos; la tecla se mantiene mientras
//!   sigan todos pulsados (p. ej. el % de captura se ve mientras se mantiene P).

use super::keymap::*;
use std::collections::HashMap;

// Máscaras de botones (mismos valores que SCE_CTRL_* de VitaSDK)
pub const SELECT: u32 = 0x0001;
pub const START: u32 = 0x0008;
pub const UP: u32 = 0x0010;
pub const RIGHT: u32 = 0x0020;
pub const DOWN: u32 = 0x0040;
pub const LEFT: u32 = 0x0080;
pub const LTRIG: u32 = 0x0100;
pub const RTRIG: u32 = 0x0200;
pub const TRIANGLE: u32 = 0x1000;
pub const CIRCLE: u32 = 0x2000;
pub const CROSS: u32 = 0x4000;
pub const SQUARE: u32 = 0x8000;

pub const ALL: [(Btn, u32); 12] = [
    (Btn::Up, UP),
    (Btn::Down, DOWN),
    (Btn::Left, LEFT),
    (Btn::Right, RIGHT),
    (Btn::Cross, CROSS),
    (Btn::Circle, CIRCLE),
    (Btn::Square, SQUARE),
    (Btn::Triangle, TRIANGLE),
    (Btn::Start, START),
    (Btn::Select, SELECT),
    (Btn::L, LTRIG),
    (Btn::R, RTRIG),
];

pub fn mask_of(b: Btn) -> u32 {
    ALL.iter().find(|(x, _)| *x == b).map_or(0, |(_, m)| *m)
}

/// Fotogramas que L/R/SELECT tienen que estar mantenidos solos para que su tecla se mantenga
pub const HOLD_FRAMES: u32 = 8;
/// Duración del toque corto de L/SELECT (fotogramas)
pub const TAP_FRAMES: u32 = 3;

/// Lo que pide el pad además de teclas
#[derive(Default, Debug, PartialEq)]
pub struct Requests {
    pub guide: bool,
    pub toggle_bar: bool,
}

/// Estado de un modificador (L, R o SELECT) usado también como tecla propia
#[derive(Default, Clone, Copy)]
struct Modifier {
    frames: u32,
    /// se ha usado su capa (o una combinación) desde que se pulsó
    used: bool,
    /// su tecla está mantenida por haberlo mantenido solo
    holding: bool,
    /// toque corto en curso: fotogramas que quedan
    tap_left: u32,
}

/// Botón de acción pulsado con un modificador que podría ser el principio de una combinación:
/// su tecla de capa espera unos fotogramas por si llega el resto de la combinación
#[derive(Clone, Copy)]
struct Pending {
    btn: Btn,
    mask: u32,
    vk: Option<u8>,
    left: u32,
}

#[derive(Default)]
pub struct PadLogic {
    prev: u32,
    held: HashMap<Btn, u8>,
    combo: Option<usize>,
    mods: [Modifier; 3], // L, R, SELECT
    pending: Option<Pending>,
    /// teclas pulsadas en un toque que hay que soltar dentro de N fotogramas
    delayed_release: Vec<(u8, u32)>,
}

/// Fotogramas que se espera a que se complete una combinación (~130 ms a 30 fps)
pub const COMBO_GRACE: u32 = 4;

pub fn is_dir(b: Btn) -> bool {
    matches!(b, Btn::Up | Btn::Down | Btn::Left | Btn::Right)
}

const MOD_BTNS: [(Btn, u32); 3] = [(Btn::L, LTRIG), (Btn::R, RTRIG), (Btn::Select, SELECT)];

impl PadLogic {
    pub fn new() -> Self {
        Self::default()
    }

    /// Suelta todo (al abrir una página del runner, cambiar de mapa…)
    pub fn release_all(&mut self, km: &Keymap, out: &mut Vec<(u8, bool)>) {
        for (_, vk) in self.held.drain() {
            out.push((vk, false));
        }
        self.pending = None;
        for (vk, _) in self.delayed_release.drain(..) {
            out.push((vk, false));
        }
        if let Some(ci) = self.combo.take() {
            for &vk in km.combos[ci].1.iter().rev() {
                if vk != ACT_GUIDE && vk != ACT_BAR {
                    out.push((vk, false));
                }
            }
        }
        for (i, (b, _)) in MOD_BTNS.iter().enumerate() {
            let m = &mut self.mods[i];
            if m.holding || m.tap_left > 0 {
                if let Some(&vk) = km.base.get(b) {
                    out.push((vk, false));
                }
            }
            *m = Modifier { used: true, ..Default::default() };
        }
    }

    /// L se ha usado para otra cosa (panel trasero con L)
    pub fn mark_l_used(&mut self, km: &Keymap, out: &mut Vec<(u8, bool)>) {
        self.use_mod(0, km, out);
    }

    /// Botones vistos sin generar teclas (los modificadores pulsados cuentan como usados)
    pub fn sync(&mut self, buttons: u32) {
        for (i, (_, mask)) in MOD_BTNS.iter().enumerate() {
            if buttons & mask != 0 {
                self.mods[i].used = true;
            }
        }
        self.prev = buttons;
    }

    /// marca el modificador como usado y suelta su tecla propia si la tenía mantenida
    fn use_mod(&mut self, i: usize, km: &Keymap, out: &mut Vec<(u8, bool)>) {
        let m = &mut self.mods[i];
        m.used = true;
        if m.holding {
            m.holding = false;
            if let Some(&vk) = km.base.get(&MOD_BTNS[i].0) {
                out.push((vk, false));
            }
        }
    }

    /// Un fotograma: `buttons` = botones pulsados (con el stick ya convertido en cruceta)
    pub fn step(&mut self, buttons: u32, km: &Keymap) -> (Vec<(u8, bool)>, Requests) {
        let mut out = Vec::new();
        let mut req = Requests::default();
        let pressed = buttons & !self.prev;
        let released = !buttons & self.prev;
        let down = |m: u32| buttons & m != 0;

        // toques pendientes de soltar
        self.delayed_release.retain_mut(|(vk, n)| {
            *n -= 1;
            if *n == 0 {
                out.push((*vk, false));
                false
            } else {
                true
            }
        });

        // modificadores: inicio de pulsación
        for (i, (_, mask)) in MOD_BTNS.iter().enumerate() {
            if pressed & mask != 0 {
                let m = &mut self.mods[i];
                m.frames = 0;
                m.used = false;
            }
        }

        // combinaciones activas: se sueltan al soltar cualquiera de sus botones
        if let Some(ci) = self.combo {
            let (btns, keys) = &km.combos[ci];
            if !btns.iter().all(|&b| down(mask_of(b))) {
                for &vk in keys.iter().rev() {
                    if vk != ACT_GUIDE && vk != ACT_BAR {
                        out.push((vk, false));
                    }
                }
                self.combo = None;
            }
        }
        if self.combo.is_none() {
            // las más largas primero (L+R+START antes que R+START)
            let mut order: Vec<usize> = (0..km.combos.len()).collect();
            order.sort_by_key(|&i| std::cmp::Reverse(km.combos[i].0.len()));
            for ci in order {
                let (btns, keys) = &km.combos[ci];
                // la cruceta y el stick son independientes: nunca forman parte de combinaciones
                if btns.iter().any(|&b| is_dir(b)) {
                    continue
                }
                let fresh = btns.iter().any(|&b| pressed & mask_of(b) != 0)
                    || self.pending.map_or(false, |p| btns.contains(&p.btn));
                if btns.iter().all(|&b| down(mask_of(b))) && fresh {
                    if self.pending.map_or(false, |p| btns.contains(&p.btn)) {
                        self.pending = None; // era el principio de esta combinación
                    }
                    for &b in btns {
                        if let Some(vk) = self.held.remove(&b) {
                            out.push((vk, false));
                        }
                    }
                    for i in 0..3 {
                        if btns.contains(&MOD_BTNS[i].0) {
                            self.use_mod(i, km, &mut out);
                        }
                    }
                    for &vk in keys {
                        match vk {
                            ACT_GUIDE => req.guide = true,
                            ACT_BAR => req.toggle_bar = true,
                            _ => out.push((vk, true)),
                        }
                    }
                    self.combo = Some(ci);
                    break
                }
            }
        }
        let in_combo = self.combo.is_some();

        // botón en espera: si se suelta o se acaba la espera sin completar la combinación, su tecla
        if let Some(mut p) = self.pending.take() {
            if !down(p.mask) {
                if let Some(vk) = p.vk {
                    out.push((vk, true));
                    self.delayed_release.push((vk, TAP_FRAMES));
                }
            } else if p.left <= 1 || in_combo {
                if let (Some(vk), false) = (p.vk, in_combo) {
                    self.held.insert(p.btn, vk);
                    out.push((vk, true));
                }
            } else {
                p.left -= 1;
                self.pending = Some(p);
            }
        }

        // L y R con SELECT mantenido: su tecla de la capa SELECT (L = F1 ayuda, R = Q misiones)
        for (b, mask) in [(Btn::L, LTRIG), (Btn::R, RTRIG)] {
            if released & mask != 0 {
                if let Some(vk) = self.held.remove(&b) {
                    out.push((vk, false));
                }
            }
            if pressed & mask != 0 && !in_combo && down(SELECT) {
                if let Some(&vk) = km.layer_select.get(&b) {
                    self.use_mod(2, km, &mut out);
                    let li = if b == Btn::L { 0 } else { 1 };
                    self.mods[li].used = true; // no es un toque de L/R
                    self.held.insert(b, vk);
                    out.push((vk, true));
                }
            }
        }

        // resto de botones (capas)
        for (b, mask) in ALL {
            if matches!(b, Btn::L | Btn::R | Btn::Select) {
                continue
            }
            if released & mask != 0 {
                if let Some(vk) = self.held.remove(&b) {
                    out.push((vk, false));
                }
            }
            if released & mask != 0 && self.pending.map_or(false, |p| p.btn == b) {
                continue // ya resuelto arriba
            }
            if pressed & mask != 0 && is_dir(b) {
                // cruceta/stick: siempre su tecla, con o sin L/R/SELECT, y sin contar como uso de capa
                if let Some(&vk) = km.base.get(&b) {
                    self.held.insert(b, vk);
                    out.push((vk, true));
                }
                continue
            }
            if pressed & mask != 0 && !in_combo {
                let vk = if down(SELECT) {
                    self.use_mod(2, km, &mut out);
                    km.layer_select.get(&b).copied()
                } else if down(LTRIG) {
                    self.use_mod(0, km, &mut out);
                    km.layer_l.get(&b).copied()
                } else if down(RTRIG) {
                    self.use_mod(1, km, &mut out);
                    km.layer_r.get(&b).copied()
                } else {
                    km.base.get(&b).copied()
                };
                // ¿puede ser el principio de una combinación? (los modificadores mantenidos están todos
                // en ella y le falta algún botón): esperar unos fotogramas antes de decidir
                let mods_down: Vec<Btn> = MOD_BTNS.iter().filter(|(_, m)| down(*m)).map(|(b, _)| *b).collect();
                let could = !mods_down.is_empty()
                    && self.pending.is_none()
                    && km.combos.iter().any(|(btns, _)| {
                        btns.contains(&b)
                            && !btns.iter().any(|&x| is_dir(x))
                            && mods_down.iter().all(|m| btns.contains(m))
                            && !btns.iter().all(|&x| down(mask_of(x)))
                    });
                if could {
                    self.pending = Some(Pending { btn: b, mask, vk, left: COMBO_GRACE });
                } else if let Some(vk) = vk {
                    self.held.insert(b, vk);
                    out.push((vk, true));
                }
            }
        }

        // modificadores solos: toque corto o mantenido
        for (i, (b, mask)) in MOD_BTNS.iter().enumerate() {
            let Some(&vk) = km.base.get(b) else { continue };
            let m = &mut self.mods[i];
            if down(*mask) {
                m.frames += 1;
                if !m.used && !m.holding && m.frames > HOLD_FRAMES && !in_combo {
                    // SELECT mantenido solo no hace nada (su capa es lo habitual); L y R sí
                    if *b != Btn::Select {
                        m.holding = true;
                        if m.tap_left == 0 {
                            out.push((vk, true));
                        }
                        m.tap_left = 0;
                    }
                }
            }
            if released & mask != 0 {
                if m.holding {
                    m.holding = false;
                    out.push((vk, false));
                } else if !m.used && m.frames <= HOLD_FRAMES && *b != Btn::R {
                    // toque corto (R no: CTRL solo tiene sentido mantenido)
                    if m.tap_left == 0 {
                        out.push((vk, true));
                    }
                    m.tap_left = TAP_FRAMES;
                    continue
                }
            }
            if m.tap_left > 0 {
                m.tap_left -= 1;
                if m.tap_left == 0 && !m.holding {
                    out.push((vk, false));
                }
            }
        }

        self.prev = buttons;
        (out, req)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// Simula una secuencia de estados del pad y devuelve, por fotograma, las teclas mantenidas
    fn run(km: &Keymap, frames: &[u32]) -> (Vec<HashSet<u8>>, Requests) {
        let mut p = PadLogic::new();
        let mut keys: HashSet<u8> = HashSet::new();
        let mut hist = Vec::new();
        let mut reqs = Requests::default();
        for &f in frames {
            let (out, r) = p.step(f, km);
            for (vk, d) in out {
                if d {
                    assert!(keys.insert(vk) || true);
                } else {
                    keys.remove(&vk);
                }
            }
            reqs.guide |= r.guide;
            reqs.toggle_bar |= r.toggle_bar;
            hist.push(keys.clone());
        }
        (hist, reqs)
    }

    fn hold(mask: u32, n: usize) -> Vec<u32> {
        vec![mask; n]
    }

    /// pulsar `mods` primero (unos fotogramas), luego añadir `b` durante `n` fotogramas, soltar todo
    fn chord(mods: u32, b: u32, n: usize) -> Vec<u32> {
        let mut v = Vec::new();
        if mods != 0 {
            v.extend(hold(mods, 2));
        }
        v.extend(hold(mods | b, n));
        v.extend(hold(0, 4));
        v
    }

    fn km(text: &str) -> Keymap {
        parse(text)
    }

    /// mapa por defecto y, si RLD_TEST_KEYMAP apunta a un keymap.ini, también ese
    fn maps() -> Vec<Keymap> {
        let mut v = vec![km(DEFAULT_KEYMAP)];
        if let Some(t) = std::env::var("RLD_TEST_KEYMAP").ok().and_then(|p| std::fs::read_to_string(p).ok()) {
            v.push(km(&t));
        }
        v
    }

    #[test]
    fn todas_las_teclas_del_mapa_por_defecto() {
      for km in maps() {
        let face = [(Btn::Up, UP), (Btn::Down, DOWN), (Btn::Left, LEFT), (Btn::Right, RIGHT), (Btn::Cross, CROSS), (Btn::Circle, CIRCLE), (Btn::Square, SQUARE), (Btn::Triangle, TRIANGLE), (Btn::Start, START)];
        for (layer, modm, name) in [(&km.base, 0u32, "base"), (&km.layer_l, LTRIG, "L"), (&km.layer_r, RTRIG, "R"), (&km.layer_select, SELECT, "SELECT")] {
            for (b, m) in face {
                let Some(&vk) = layer.get(&b) else { continue };
                // si es parte de una combinación con ese modificador, la combinación manda
                let (h, _) = run(&km, &chord(modm, m, 10));
                let frames_with = h.iter().filter(|s| s.contains(&vk)).count();
                // con modificador puede esperar COMBO_GRACE fotogramas por si es una combinación
                let need = if modm == 0 { 10 } else { 10 - COMBO_GRACE as usize - 1 };
                assert!(frames_with >= need, "capa {name} {b:?} -> {vk}: solo {frames_with} fotogramas");
                // y ninguna otra tecla salvo la del propio modificador mantenido
                let others: HashSet<u8> = h.iter().flat_map(|s| s.iter().copied()).filter(|&k| k != vk).collect();
                assert!(others.is_empty(), "capa {name} {b:?}: teclas de más {others:?}");
                assert!(h.last().unwrap().is_empty(), "capa {name} {b:?}: queda pulsada");
            }
        }
      }
    }

    #[test]
    fn toques_y_mantenidos_de_l_r_select() {
        let km = km(DEFAULT_KEYMAP);
        let shift = 0x10u8;
        let ctrl = 0x11u8;
        let space = 0x20u8;
        // toque corto de L: SHIFT unos fotogramas
        let (h, _) = run(&km, &[hold(LTRIG, 3), hold(0, 6)].concat());
        assert!(h.iter().filter(|s| s.contains(&shift)).count() >= 2);
        assert!(h.last().unwrap().is_empty());
        // L mantenido: SHIFT mientras se mantiene (ver ataques en combate)
        let (h, _) = run(&km, &[hold(LTRIG, 40), hold(0, 3)].concat());
        assert!(h[20].contains(&shift) && h[39].contains(&shift) && h.last().unwrap().is_empty());
        // R mantenido: CTRL
        let (h, _) = run(&km, &[hold(RTRIG, 40), hold(0, 3)].concat());
        assert!(h[20].contains(&ctrl) && h.last().unwrap().is_empty());
        // toque de R: nada
        let (h, _) = run(&km, &[hold(RTRIG, 3), hold(0, 4)].concat());
        assert!(h.iter().all(|s| s.is_empty()));
        // toque de SELECT: ESPACIO
        let (h, _) = run(&km, &[hold(SELECT, 3), hold(0, 6)].concat());
        assert!(h.iter().filter(|s| s.contains(&space)).count() >= 2 && h.last().unwrap().is_empty());
        // L mantenido mucho y luego capa: SHIFT se suelta antes de la tecla de la capa
        let (h, _) = run(&km, &[hold(LTRIG, 20), hold(LTRIG | CROSS, 10), hold(0, 3)].concat());
        let s = *km.layer_l.get(&Btn::Cross).unwrap();
        let f = 20 + COMBO_GRACE as usize + 1;
        assert!(h[f].contains(&s) && !h[f].contains(&shift), "{:?}", h[f]);
    }

    #[test]
    fn combinaciones_mantienen_la_tecla() {
      for km in maps() {
        for (btns, keys) in &km.combos {
            let all: u32 = btns.iter().map(|&b| mask_of(b)).fold(0, |a, m| a | m);
            let last = mask_of(*btns.last().unwrap());
            let first = all & !last;
            // los primeros mantenidos 20 fotogramas (dispara SHIFT/CTRL de L/R), luego el último
            let seq = [hold(first, 20), hold(all, 15), hold(0, 4)].concat();
            let (h, req) = run(&km, &seq);
            let real: Vec<u8> = keys.iter().copied().filter(|&k| k != ACT_GUIDE && k != ACT_BAR).collect();
            for k in &real {
                let n = h[20..35].iter().filter(|s| s.contains(k)).count();
                assert!(n >= 14, "combo {btns:?} -> {k}: {n} fotogramas");
            }
            // durante la combinación no queda SHIFT ni CTRL de L/R mantenidos
            for s in &h[21..35] {
                let extra: Vec<u8> = s.iter().copied().filter(|k| !real.contains(k)).collect();
                assert!(extra.is_empty(), "combo {btns:?}: teclas de más {extra:?}");
            }
            assert!(h.last().unwrap().is_empty(), "combo {btns:?}: queda algo pulsado");
            if keys.contains(&ACT_GUIDE) {
                assert!(req.guide);
            }
            if keys.contains(&ACT_BAR) {
                assert!(req.toggle_bar);
            }
        }
      }
    }

    #[test]
    fn mapa_del_usuario_l_r_cruz_es_p() {
        let text = DEFAULT_KEYMAP.replace("L+R+CROSS    = F2", "L+R+CROSS    = P").replace("L+R+SELECT   = P", "L+R+SELECT   = NADA");
        let km = km(&text);
        let seq = [hold(LTRIG, 5), hold(LTRIG | RTRIG, 15), hold(LTRIG | RTRIG | CROSS, 30), hold(0, 3)].concat();
        let (h, _) = run(&km, &seq);
        assert!(h[25..50].iter().all(|s| s.len() == 1 && s.contains(&b'P')), "{:?}", &h[25..30]);
    }

    #[test]
    fn cruceta_independiente() {
        for km in maps() {
            for (m, name) in [(LTRIG, "L"), (RTRIG, "R"), (SELECT, "SELECT"), (LTRIG | RTRIG, "L+R")] {
                for (d, vk) in [(UP, 0x26u8), (DOWN, 0x28), (LEFT, 0x25), (RIGHT, 0x27)] {
                    let (h, _) = run(&km, &[hold(m, 2), hold(m | d, 6), hold(m, 2), hold(0, 5)].concat());
                    assert!(h[2..8].iter().all(|s| s.contains(&vk)), "{name}+{d:x}: {:?}", &h[2..8]);
                    assert!(h.last().unwrap().is_empty());
                }
            }
            // mover y pulsar acción a la vez: las dos teclas
            let z = *km.base.get(&Btn::Cross).unwrap();
            let (h, _) = run(&km, &[hold(UP, 3), hold(UP | CROSS, 4), hold(0, 3)].concat());
            assert!(h[4].contains(&0x26) && h[4].contains(&z));
        }
    }

    #[test]
    fn combinacion_en_cualquier_orden() {
        let km = km(DEFAULT_KEYMAP);
        let combo_vk = 0x71u8; // L+R+CROSS = F2 en el mapa por defecto
        let layer_s = *km.layer_l.get(&Btn::Cross).unwrap();
        // L, luego Cruz y, 2 fotogramas después, R: la combinación, sin la tecla de la capa L
        let (h, _) = run(&km, &[hold(LTRIG, 3), hold(LTRIG | CROSS, 2), hold(LTRIG | CROSS | RTRIG, 10), hold(0, 4)].concat());
        assert!(h.iter().any(|s| s.contains(&combo_vk)), "{h:?}");
        assert!(h.iter().all(|s| !s.contains(&layer_s)), "se coló la capa: {h:?}");
        // L y Cruz sin R: la tecla de la capa (tras la espera) mantenida
        let (h, _) = run(&km, &[hold(LTRIG, 3), hold(LTRIG | CROSS, 12), hold(0, 6)].concat());
        assert!(h[3 + COMBO_GRACE as usize + 1].contains(&layer_s) && h[14].contains(&layer_s), "{h:?}");
        assert!(h.last().unwrap().is_empty());
        // L y toque rápido de Cruz: toque de la tecla de la capa
        let (h, _) = run(&km, &[hold(LTRIG, 3), hold(LTRIG | CROSS, 2), hold(LTRIG, 6), hold(0, 6)].concat());
        assert!(h.iter().filter(|s| s.contains(&layer_s)).count() >= 2, "{h:?}");
        assert!(h.last().unwrap().is_empty());
        // los tres a la vez
        let (h, _) = run(&km, &[hold(LTRIG | RTRIG | CROSS, 8), hold(0, 4)].concat());
        assert!(h[3].contains(&combo_vk) && h.last().unwrap().is_empty());
    }

    #[test]
    fn select_con_l_r() {
        let km = km(DEFAULT_KEYMAP);
        let f1 = 0x70u8;
        let (h, _) = run(&km, &[hold(SELECT, 3), hold(SELECT | LTRIG, 6), hold(0, 4)].concat());
        assert!(h[4].contains(&f1) && h.iter().all(|s| !s.contains(&0x20) && !s.contains(&0x10)), "{h:?}");
    }
}
