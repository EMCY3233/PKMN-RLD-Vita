//! Controles de PS Vita → teclas y ratón de PC (lo que el juego espera).
//!
//! El mapa se lee de `ux0:data/reloaded/keymap.ini` (mismo formato que vita/keymap.ini del
//! repositorio); si no existe se usa el mapa por defecto de abajo, idéntico a ese archivo.
//! - Capa base: cada botón es una tecla.
//! - Mantener L o R: los botones cambian a la capa L/R. L pulsado solo (sin usar la capa) = SHIFT
//!   breve; R mantenido solo = CTRL mientras se mantenga (acelerar combates).
//! - Stick izquierdo = cruceta. SELECT+START = ESCAPE.
//! - Pantalla táctil = ratón (clic izquierdo). Panel trasero: 4 cuadrantes = teclas 1-4.

use std::collections::HashMap;
use vitasdk_sys::*;

const DEFAULT_KEYMAP: &str = include_str!("../../../../vita/keymap.ini");

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Btn {
    Up,
    Down,
    Left,
    Right,
    Cross,
    Circle,
    Square,
    Triangle,
    Start,
    Select,
    L,
    R,
}

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

fn btn_name(n: &str) -> Option<Btn> {
    Some(match n {
        "DPAD_UP" => Btn::Up,
        "DPAD_DOWN" => Btn::Down,
        "DPAD_LEFT" => Btn::Left,
        "DPAD_RIGHT" => Btn::Right,
        "CROSS" => Btn::Cross,
        "CIRCLE" => Btn::Circle,
        "SQUARE" => Btn::Square,
        "TRIANGLE" => Btn::Triangle,
        "START" => Btn::Start,
        "SELECT" => Btn::Select,
        "L" => Btn::L,
        "R" => Btn::R,
        _ => return None,
    })
}

/// Nombre de tecla del keymap → código virtual de Windows (el que usa GM8)
fn vk_name(n: &str) -> Option<u8> {
    let n = n.trim().to_ascii_uppercase();
    Some(match n.as_str() {
        "UP" => 0x26,
        "DOWN" => 0x28,
        "LEFT" => 0x25,
        "RIGHT" => 0x27,
        "ENTER" => 0x0D,
        "SPACE" => 0x20,
        "SHIFT" => 0x10,
        "CTRL" => 0x11,
        "ESCAPE" => 0x1B,
        "BACKSPACE" => 0x08,
        "TAB" => 0x09,
        "PAGEUP" => 0x21,
        "PAGEDOWN" => 0x22,
        "HOME" => 0x24,
        "END" => 0x23,
        s if s.len() == 1 && s.as_bytes()[0].is_ascii_alphanumeric() => s.as_bytes()[0],
        s if s.starts_with('F') && s[1..].parse::<u8>().map_or(false, |f| (1..=12).contains(&f)) => {
            0x6F + s[1..].parse::<u8>().unwrap()
        },
        _ => return None,
    })
}

struct Keymap {
    base: HashMap<Btn, u8>,
    layer_l: HashMap<Btn, u8>,
    layer_r: HashMap<Btn, u8>,
    rear: [u8; 4],
}

fn parse(text: &str) -> Keymap {
    let mut km = Keymap { base: HashMap::new(), layer_l: HashMap::new(), layer_r: HashMap::new(), rear: [b'1', b'2', b'3', b'4'] };
    let mut section = String::new();
    for line in text.lines() {
        let line = line.split(';').next().unwrap_or("").trim();
        if line.starts_with('[') {
            section = line.trim_matches(|c| c == '[' || c == ']').to_ascii_lowercase();
            continue
        }
        let Some((k, v)) = line.split_once('=') else { continue };
        let (k, v) = (k.trim().to_ascii_uppercase(), v.trim());
        match section.as_str() {
            "base" | "capa_l" | "capa_r" => {
                if let (Some(b), Some(vk)) = (btn_name(&k), vk_name(v)) {
                    match section.as_str() {
                        "base" => km.base.insert(b, vk),
                        "capa_l" => km.layer_l.insert(b, vk),
                        _ => km.layer_r.insert(b, vk),
                    };
                }
            },
            "tactil" => {
                let i = match k.as_str() {
                    "REAR_TL" => 0,
                    "REAR_TR" => 1,
                    "REAR_BL" => 2,
                    "REAR_BR" => 3,
                    _ => continue,
                };
                if let Some(vk) = vk_name(v) {
                    km.rear[i] = vk;
                }
            },
            _ => (),
        }
    }
    km
}

pub struct VitaInput {
    km: Keymap,
    prev_buttons: u32,
    /// teclas que mantenemos pulsadas y por qué botón
    held: HashMap<Btn, u8>,
    layer_used_l: bool,
    layer_used_r: bool,
    r_frames: u32,
    r_ctrl: bool,
    shift_release_in: u32,
    stick: [bool; 4],
    escape: bool,
    touching: bool,
    rear_held: Option<u8>,
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
            prev_buttons: 0,
            held: HashMap::new(),
            layer_used_l: false,
            layer_used_r: false,
            r_frames: 0,
            r_ctrl: false,
            shift_release_in: 0,
            stick: [false; 4],
            escape: false,
            touching: false,
            rear_held: None,
        }
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

        // SELECT+START = ESCAPE
        let esc = down(SCE_CTRL_SELECT) && down(SCE_CTRL_START);
        if esc != self.escape {
            out.push(Out::Key(0x1B, esc));
            self.escape = esc;
        }

        for (b, mask) in ALL {
            if b == Btn::L || b == Btn::R {
                continue
            }
            if released & mask != 0 {
                if let Some(vk) = self.held.remove(&b) {
                    out.push(Out::Key(vk, false));
                }
            }
            if pressed & mask != 0 && !esc {
                let vk = if down(SCE_CTRL_LTRIGGER) {
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

        // L: toque corto sin usar la capa = SHIFT durante 2 fotogramas
        if pressed & SCE_CTRL_LTRIGGER != 0 {
            self.layer_used_l = false;
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
            self.layer_used_r = false;
            self.r_frames = 0;
        }
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
        if t.reportNum > 0 {
            let (x, y) = (t.report[0].x as i32, t.report[0].y as i32);
            out.push(Out::MouseMove(x * win.0 as i32 / 1920, y * win.1 as i32 / 1088));
            if !self.touching {
                out.push(Out::MouseButton(true));
                self.touching = true;
            }
        } else if self.touching {
            out.push(Out::MouseButton(false));
            self.touching = false;
        }

        // panel trasero: cuadrantes = teclas 1-4
        let mut r: SceTouchData = unsafe { std::mem::zeroed() };
        unsafe {
            sceTouchPeek(SCE_TOUCH_PORT_BACK, &mut r, 1);
        }
        let quad = if r.reportNum > 0 {
            let (x, y) = (r.report[0].x as i32, r.report[0].y as i32);
            let q = (x >= 960) as usize + 2 * (y >= 500) as usize;
            Some(self.km.rear[q])
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
