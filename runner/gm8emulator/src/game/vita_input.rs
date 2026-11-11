//! Controles de PS Vita → teclas y ratón de PC (lo que el juego espera).
//!
//! El mapa se lee de `ux0:data/reloaded/keymap.ini` (mismo formato que vita/keymap.ini del
//! repositorio); si no existe se usa el mapa por defecto de abajo, idéntico a ese archivo.
//! - Capa base: cada botón es una tecla.
//! - Mantener L o R: los botones cambian a la capa L/R. L pulsado solo (sin usar la capa) = SHIFT
//!   breve; R mantenido solo = CTRL mientras se mantenga (acelerar combates).
//! - Stick izquierdo = cruceta. SELECT+START = ESCAPE.
//! - Pantalla táctil = ratón (clic izquierdo). Panel trasero: 4 cuadrantes = teclas 1-4.

use super::keymap::*;
use std::collections::HashMap;
use vitasdk_sys::*;


const ALL: [(Btn, u32); 12] = [
    (Btn::Up, SCE_CTRL_UP),
    (Btn::Down, SCE_CTRL_DOWN),
    (Btn::Left, SCE_CTRL_LEFT),
    (Btn::Right, SCE_CTRL_RIGHT),
    (Btn::Cross, SCE_CTRL_CROSS),
    (Btn::Circle, SCE_CTRL_CIRCLE),
    (Btn::Square, SCE_CTRL_SQUARE),
    (Btn::Triangle, SCE_CTRL_TRIANGLE),
    (Btn::Start, SCE_CTRL_START),
    (Btn::Select, SCE_CTRL_SELECT),
    (Btn::L, SCE_CTRL_LTRIGGER),
    (Btn::R, SCE_CTRL_RTRIGGER),
];

pub struct VitaInput {
    km: Keymap,
    /// texto del keymap en uso y dónde se guarda (ux0:data/reloaded/keymap.ini)
    pub km_text: String,
    pub km_path: std::path::PathBuf,
    prev_buttons: u32,
    /// teclas que mantenemos pulsadas y por qué botón
    held: HashMap<Btn, u8>,
    layer_used_l: bool,
    layer_used_r: bool,
    layer_used_select: bool,
    skip_lr_tap: u32,
    select_release_in: u32,
    r_frames: u32,
    r_ctrl: bool,
    shift_release_in: u32,
    stick: [bool; 4],
    /// combinación activa (índice) y si ya se soltó alguno de sus botones
    combo: Option<usize>,
    touching: bool,
    /// toque actual sobre la barra de teclas: tecla pulsada
    overlay_key: Option<u8>,
    rear_held: Option<u8>,
    /// SELECT+TRIÁNGULO pulsado: abrir el teclado del sistema
    pub ime_request: bool,
    /// L+R+○ (por defecto): abrir la guía de controles
    pub guide_request: bool,
    /// repetición de la cruceta en páginas
    page_repeat: u32,
    pub ime: Option<Ime>,
    /// tecla ficticia que hay que soltar en el siguiente sondeo (tras cerrar el teclado)
    pub ime_release: Option<u8>,
    /// id del script GetDialogString (se abre el teclado al empezar)
    pub ime_script: Option<usize>,
}

/// Evento para el juego
pub enum Out {
    Key(u8, bool),
    MouseMove(i32, i32),
    MouseButton(bool),
}

impl VitaInput {
    pub fn new(game_dir: &std::path::Path) -> Self {
        let text = std::fs::read_to_string(game_dir.join("keymap.ini")).unwrap_or_else(|_| DEFAULT_KEYMAP.to_string());
        unsafe {
            sceCtrlSetSamplingMode(SCE_CTRL_MODE_ANALOG);
            sceTouchSetSamplingState(SCE_TOUCH_PORT_FRONT, SCE_TOUCH_SAMPLING_STATE_START);
            sceTouchSetSamplingState(SCE_TOUCH_PORT_BACK, SCE_TOUCH_SAMPLING_STATE_START);
        }
        VitaInput {
            km: parse(&text),
            km_text: text.clone(),
            km_path: game_dir.join("keymap.ini"),
            prev_buttons: 0,
            held: HashMap::new(),
            layer_used_l: false,
            layer_used_r: false,
            layer_used_select: false,
            skip_lr_tap: 0,
            select_release_in: 0,
            r_frames: 0,
            r_ctrl: false,
            shift_release_in: 0,
            stick: [false; 4],
            combo: None,
            touching: false,
            overlay_key: None,
            rear_held: None,
            ime_request: false,
            guide_request: false,
            page_repeat: 0,
            ime: None,
            ime_release: None,
            ime_script: None,
        }
    }

    /// Cambia el mapa en uso (tras editarlo desde la guía)
    pub fn reload(&mut self, text: String) {
        self.km = parse(&text);
        self.km_text = text;
        self.held.clear();
        self.combo = None;
    }

    /// Lee mandos y pantallas táctiles; `win` = tamaño de la ventana (960×544)
    pub fn poll(&mut self, win: (u32, u32)) -> Vec<Out> {
        let mut out = Vec::new();
        let mut pad: SceCtrlData = unsafe { std::mem::zeroed() };
        unsafe {
            sceCtrlPeekBufferPositive(0, &mut pad, 1);
        }
        let mut buttons = pad.buttons;
        // stick izquierdo como cruceta (zona muerta 40 %)
        let (lx, ly) = (pad.lx as i32 - 128, pad.ly as i32 - 128);
        let dz = 51;
        let st = [ly < -dz, ly > dz, lx < -dz, lx > dz];
        for (i, (b, mask)) in [(Btn::Up, SCE_CTRL_UP), (Btn::Down, SCE_CTRL_DOWN), (Btn::Left, SCE_CTRL_LEFT), (Btn::Right, SCE_CTRL_RIGHT)]
            .into_iter()
            .enumerate()
        {
            let _ = b;
            if st[i] {
                buttons |= mask;
            }
        }
        self.stick = st;
        let pressed = buttons & !self.prev_buttons;
        let released = !buttons & self.prev_buttons;
        let down = |m: u32| buttons & m != 0;

        // página del runner abierta (manual, imagen, guía): los botones la manejan a ella
        if crate::render::OVERLAY_PAGE.lock().map_or(false, |p| p.is_some()) {
            use crate::game::pages::{page_accept, page_close, page_is_menu, page_scroll};
            let lines = crate::render::PAGE_LINES as i32;
            let menu = page_is_menu();
            if menu {
                if pressed & SCE_CTRL_CROSS != 0 {
                    page_accept('a');
                } else if pressed & SCE_CTRL_TRIANGLE != 0 {
                    page_accept('t');
                } else if pressed & (SCE_CTRL_CIRCLE | SCE_CTRL_START) != 0 {
                    page_close();
                }
            } else if pressed & (SCE_CTRL_CIRCLE | SCE_CTRL_CROSS | SCE_CTRL_START) != 0 {
                page_close();
            }
            let held_ud = buttons & (SCE_CTRL_UP | SCE_CTRL_DOWN);
            if held_ud != 0 {
                self.page_repeat += 1;
            } else {
                self.page_repeat = 0;
            }
            let rep = self.page_repeat == 1 || (self.page_repeat > 12 && self.page_repeat % 3 == 0);
            if rep && buttons & SCE_CTRL_DOWN != 0 {
                page_scroll(1);
            }
            if rep && buttons & SCE_CTRL_UP != 0 {
                page_scroll(-1);
            }
            if pressed & (SCE_CTRL_RTRIGGER | SCE_CTRL_RIGHT) != 0 {
                page_scroll(lines);
            }
            if pressed & (SCE_CTRL_LTRIGGER | SCE_CTRL_LEFT) != 0 {
                page_scroll(-lines);
            }
            // soltar todo lo que estuviera pulsado para el juego
            for (_, vk) in self.held.drain() {
                out.push(Out::Key(vk, false));
            }
            self.prev_buttons = buttons;
            self.layer_used_l = true;
            self.layer_used_r = true;
            self.layer_used_select = true;
            return out
        }
        // con el teclado del sistema abierto, los botones son suyos
        if self.ime.is_some() {
            self.prev_buttons = buttons;
            return out
        }
        // L+R+SELECT = mostrar/ocultar la barra (también configurable en [combos] como BARRA)
        if down(SCE_CTRL_LTRIGGER) && down(SCE_CTRL_RTRIGGER) && pressed & SCE_CTRL_SELECT != 0 {
            use std::sync::atomic::Ordering::Relaxed;
            let v = !crate::render::OVERLAY_VISIBLE.load(Relaxed);
            crate::render::OVERLAY_VISIBLE.store(v, Relaxed);
            self.layer_used_l = true;
            self.layer_used_r = true;
            self.layer_used_select = true;
            self.prev_buttons = buttons;
            return out
        }
        // SELECT+TRIÁNGULO = teclado del sistema
        if down(SCE_CTRL_SELECT) && pressed & SCE_CTRL_TRIANGLE != 0 {
            self.ime_request = true;
            self.prev_buttons = buttons;
            return out
        }
        // combinaciones de [combos] (SELECT+START = ESC, L+R+TRIÁNGULO = CTRL+P…): se activan al
        // pulsar el último botón con los demás mantenidos y se sueltan al soltar cualquiera
        let mask_of = |b: Btn| ALL.iter().find(|(x, _)| *x == b).map_or(0, |(_, m)| *m);
        if let Some(ci) = self.combo {
            let (btns, keys) = &self.km.combos[ci];
            if !btns.iter().all(|&b| down(mask_of(b))) {
                for &vk in keys.iter().rev() {
                    if vk != ACT_GUIDE && vk != ACT_BAR {
                        out.push(Out::Key(vk, false));
                    }
                }
                self.combo = None;
            }
        }
        if self.combo.is_none() {
            // las combinaciones más largas primero (L+R+START antes que R+START)
            let mut order: Vec<usize> = (0..self.km.combos.len()).collect();
            order.sort_by_key(|&i| std::cmp::Reverse(self.km.combos[i].0.len()));
            for ci in order {
                let (btns, keys) = &self.km.combos[ci];
                if btns.iter().all(|&b| down(mask_of(b))) && btns.iter().any(|&b| pressed & mask_of(b) != 0) {
                    // soltar lo que esos botones tuvieran pulsado por separado
                    for &b in btns {
                        if let Some(vk) = self.held.remove(&b) {
                            out.push(Out::Key(vk, false));
                        }
                    }
                    for &vk in keys {
                        match vk {
                            ACT_GUIDE => self.guide_request = true,
                            ACT_BAR => {
                                use std::sync::atomic::Ordering::Relaxed;
                                let v = !crate::render::OVERLAY_VISIBLE.load(Relaxed);
                                crate::render::OVERLAY_VISIBLE.store(v, Relaxed);
                            },
                            _ => out.push(Out::Key(vk, true)),
                        }
                    }
                    self.layer_used_l = true;
                    self.layer_used_r = true;
                    self.layer_used_select = true;
                    self.combo = Some(ci);
                    break
                }
            }
        }
        let esc = self.combo.is_some();

        // L y R con SELECT mantenido: su tecla de la capa SELECT (L = F1 ayuda, R = Q misiones)
        for (b, mask) in [(Btn::L, SCE_CTRL_LTRIGGER), (Btn::R, SCE_CTRL_RTRIGGER)] {
            if released & mask != 0 {
                if let Some(vk) = self.held.remove(&b) {
                    out.push(Out::Key(vk, false));
                }
            }
            if pressed & mask != 0 && !esc && down(SCE_CTRL_SELECT) {
                if let Some(&vk) = self.km.layer_select.get(&b) {
                    self.layer_used_select = true;
                    self.held.insert(b, vk);
                    out.push(Out::Key(vk, true));
                    self.skip_lr_tap |= mask;
                }
            }
        }

        for (b, mask) in ALL {
            if b == Btn::L || b == Btn::R || b == Btn::Select {
                continue
            }
            if released & mask != 0 {
                if let Some(vk) = self.held.remove(&b) {
                    out.push(Out::Key(vk, false));
                }
            }
            if pressed & mask != 0 && !esc {
                let vk = if down(SCE_CTRL_SELECT) {
                    self.layer_used_select = true;
                    self.km.layer_select.get(&b).copied()
                } else if down(SCE_CTRL_LTRIGGER) {
                    self.layer_used_l = true;
                    self.km.layer_l.get(&b).copied()
                } else if down(SCE_CTRL_RTRIGGER) {
                    self.layer_used_r = true;
                    self.km.layer_r.get(&b).copied()
                } else {
                    self.km.base.get(&b).copied()
                };
                if let Some(vk) = vk {
                    self.held.insert(b, vk);
                    out.push(Out::Key(vk, true));
                }
            }
        }

        // SELECT: toque corto sin usar su capa = su tecla (ESPACIO) durante 2 fotogramas
        if pressed & SCE_CTRL_SELECT != 0 {
            self.layer_used_select = false;
        }
        if released & SCE_CTRL_SELECT != 0 && !self.layer_used_select {
            if let Some(&vk) = self.km.base.get(&Btn::Select) {
                out.push(Out::Key(vk, true));
                self.select_release_in = 2;
            }
        }
        if self.select_release_in > 0 {
            self.select_release_in -= 1;
            if self.select_release_in == 0 {
                if let Some(&vk) = self.km.base.get(&Btn::Select) {
                    out.push(Out::Key(vk, false));
                }
            }
        }
        // L: toque corto sin usar la capa = SHIFT durante 2 fotogramas
        if pressed & SCE_CTRL_LTRIGGER != 0 {
            self.layer_used_l = self.skip_lr_tap & SCE_CTRL_LTRIGGER != 0;
        }
        if released & SCE_CTRL_LTRIGGER != 0 && !self.layer_used_l {
            if let Some(&vk) = self.km.base.get(&Btn::L) {
                out.push(Out::Key(vk, true));
                self.shift_release_in = 2;
            }
        }
        if self.shift_release_in > 0 {
            self.shift_release_in -= 1;
            if self.shift_release_in == 0 {
                if let Some(&vk) = self.km.base.get(&Btn::L) {
                    out.push(Out::Key(vk, false));
                }
            }
        }
        // R: mantenido sin usar la capa (más de 8 fotogramas) = CTRL mientras siga pulsado
        if pressed & SCE_CTRL_RTRIGGER != 0 {
            self.layer_used_r = self.skip_lr_tap & SCE_CTRL_RTRIGGER != 0;
            self.r_frames = 0;
        }
        self.skip_lr_tap = 0;
        if down(SCE_CTRL_RTRIGGER) {
            self.r_frames += 1;
            if !self.r_ctrl && !self.layer_used_r && self.r_frames > 8 {
                if let Some(&vk) = self.km.base.get(&Btn::R) {
                    out.push(Out::Key(vk, true));
                    self.r_ctrl = true;
                }
            }
        } else if self.r_ctrl {
            if let Some(&vk) = self.km.base.get(&Btn::R) {
                out.push(Out::Key(vk, false));
            }
            self.r_ctrl = false;
        }
        self.prev_buttons = buttons;

        // táctil frontal = ratón (1920×1088 → ventana)
        let mut t: SceTouchData = unsafe { std::mem::zeroed() };
        unsafe {
            sceTouchPeek(SCE_TOUCH_PORT_FRONT, &mut t, 1);
        }
        let overlay_on = crate::render::OVERLAY_VISIBLE.load(std::sync::atomic::Ordering::Relaxed);
        if t.reportNum > 0 && (self.overlay_key.is_some() || (!self.touching && overlay_on && {
            let (sx, sy) = (t.report[0].x as i32 * 960 / 1920, t.report[0].y as i32 * 544 / 1088);
            crate::render::overlay_hit(sx, sy).is_some()
        })) {
            // toque sobre la barra de teclas: tecla, no ratón (se mantiene hasta levantar el dedo)
            if self.overlay_key.is_none() {
                let (sx, sy) = (t.report[0].x as i32 * 960 / 1920, t.report[0].y as i32 * 544 / 1088);
                if let Some(i) = crate::render::overlay_hit(sx, sy) {
                    let vk = crate::render::OVERLAY_KEYS[i];
                    out.push(Out::Key(vk, true));
                    self.overlay_key = Some(vk);
                    crate::render::OVERLAY_PRESSED.store(i as i32, std::sync::atomic::Ordering::Relaxed);
                }
            }
        } else if t.reportNum > 0 {
            // pantalla (1920×1088 en el panel) → píxeles del juego según dónde se dibujó la imagen
            let (sx, sy) = (t.report[0].x as i32 * win.0 as i32 / 1920, t.report[0].y as i32 * win.1 as i32 / 1088);
            let r = crate::render::PRESENT_RECT.lock().map(|r| *r).unwrap_or_default();
            let (gx, gy) = if r[2] > 0 && r[3] > 0 {
                (((sx - r[0]) * r[4] / r[2]).clamp(0, r[4] - 1), ((sy - r[1]) * r[5] / r[3]).clamp(0, r[5] - 1))
            } else {
                (sx, sy)
            };
            out.push(Out::MouseMove(gx, gy));
            if !self.touching {
                out.push(Out::MouseButton(true));
                self.touching = true;
            }
        } else if self.touching {
            out.push(Out::MouseButton(false));
            self.touching = false;
        }
        if t.reportNum == 0 {
            if let Some(vk) = self.overlay_key.take() {
                out.push(Out::Key(vk, false));
                crate::render::OVERLAY_PRESSED.store(-1, std::sync::atomic::Ordering::Relaxed);
            }
        }

        // panel trasero: 6 zonas (1 2 3 / 4 5 6); con L mantenido, la segunda fila de teclas
        let mut r: SceTouchData = unsafe { std::mem::zeroed() };
        unsafe {
            sceTouchPeek(SCE_TOUCH_PORT_BACK, &mut r, 1);
        }
        let quad = if r.reportNum > 0 {
            let (x, y) = (r.report[0].x as i32, r.report[0].y as i32);
            let zone = (x / 640).clamp(0, 2) as usize + 3 * (y >= 500) as usize;
            if down(SCE_CTRL_LTRIGGER) {
                self.layer_used_l = true;
                self.km.rear_l[zone]
            } else {
                self.km.rear[zone]
            }
        } else {
            None
        };
        if quad != self.rear_held {
            if let Some(vk) = self.rear_held {
                out.push(Out::Key(vk, false));
            }
            if let Some(vk) = quad {
                out.push(Out::Key(vk, true));
            }
            self.rear_held = quad;
        }
        out
    }
}

// ===== Teclado del sistema (IME) =====
//
// Se abre solo cuando el juego pide escribir un texto (script GetDialogString: nombre del
// personaje, apodos…) o con SELECT+TRIÁNGULO. Lo escrito sustituye a keyboard_string, igual que
// si se hubiera tecleado en un PC; el teclado en pantalla del juego sigue funcionando con la cruceta.

/// true mientras el teclado está en pantalla (vitaGL debe dibujar los diálogos del sistema)
pub static IME_ACTIVE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub struct Ime {
    title: Vec<u16>,
    initial: Vec<u16>,
    buffer: Vec<u16>,
}

impl Ime {
    /// Abre el teclado con un texto inicial; None si el sistema no lo permite ahora
    pub fn open(title: &str, initial: &str, max_len: u32) -> Option<Ime> {
        let max_len = max_len.clamp(1, 64);
        let mut ime = Ime {
            title: title.encode_utf16().chain(std::iter::once(0)).collect(),
            initial: initial.encode_utf16().take(max_len as usize).chain(std::iter::once(0)).collect(),
            buffer: vec![0u16; max_len as usize + 1],
        };
        unsafe {
            let mut p: SceImeDialogParam = std::mem::zeroed();
            // = sceImeDialogParamInit (función inline del SDK)
            p.sdkVersion = PSP2_SDK_VERSION;
            p.commonParam.magic = SCE_COMMON_DIALOG_MAGIC_NUMBER.wrapping_add(&p.commonParam as *const _ as u32);
            p.supportedLanguages = 0;
            p.languagesForced = 0;
            p.type_ = SCE_IME_TYPE_DEFAULT;
            p.option = 0;
            p.textBoxMode = SCE_IME_DIALOG_TEXTBOX_MODE_WITH_CLEAR;
            p.title = ime.title.as_ptr();
            p.maxTextLength = max_len;
            p.initialText = ime.initial.as_mut_ptr();
            p.inputTextBuffer = ime.buffer.as_mut_ptr();
            if sceImeDialogInit(&p) < 0 {
                return None
            }
        }
        IME_ACTIVE.store(true, std::sync::atomic::Ordering::Relaxed);
        Some(ime)
    }

    /// None = sigue abierto; Some(None) = cancelado; Some(Some(texto)) = aceptado
    pub fn poll(&mut self) -> Option<Option<String>> {
        unsafe {
            if sceImeDialogGetStatus() != SCE_COMMON_DIALOG_STATUS_FINISHED {
                return None
            }
            let mut r: SceImeDialogResult = std::mem::zeroed();
            sceImeDialogGetResult(&mut r);
            sceImeDialogTerm();
            IME_ACTIVE.store(false, std::sync::atomic::Ordering::Relaxed);
            if r.button == SCE_IME_DIALOG_BUTTON_ENTER as i32 {
                let end = self.buffer.iter().position(|&c| c == 0).unwrap_or(self.buffer.len());
                Some(Some(String::from_utf16_lossy(&self.buffer[..end])))
            } else {
                Some(None)
            }
        }
    }
}

