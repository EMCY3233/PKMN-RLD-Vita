//! Mapa de controles de la Vita (keymap.ini): lectura, guía en pantalla y cambio de botones desde
//! el propio juego. Sin dependencias de la Vita para poder probar el menú en PC.

use crate::{game::Game, render::OverlayPage};
use std::collections::HashMap;

pub const DEFAULT_KEYMAP: &str = include_str!("../../../../vita/keymap.ini");

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Btn {
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

pub fn btn_name(n: &str) -> Option<Btn> {
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
pub fn vk_name(n: &str) -> Option<u8> {
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
        "BACKSLASH" => 220,
        // acciones del runner (no son teclas)
        "GUIA" => ACT_GUIDE,
        "BARRA" => ACT_BAR,
        s if s.len() == 1 && s.as_bytes()[0].is_ascii_alphanumeric() => s.as_bytes()[0],
        s if s.starts_with('F') && s[1..].parse::<u8>().map_or(false, |f| (1..=12).contains(&f)) => {
            0x6F + s[1..].parse::<u8>().unwrap()
        },
        _ => return None,
    })
}

/// Códigos internos para acciones del runner en [combos]
pub const ACT_GUIDE: u8 = 0xF0;
pub const ACT_BAR: u8 = 0xF1;

pub struct Keymap {
    /// para la guía en pantalla: (sección, botones, teclas, descripción del comentario)
    pub guide: Vec<(String, String, String, String)>,
    pub base: HashMap<Btn, u8>,
    pub layer_l: HashMap<Btn, u8>,
    pub layer_r: HashMap<Btn, u8>,
    pub layer_select: HashMap<Btn, u8>,
    /// panel trasero: 6 zonas (3 columnas × 2 filas) y las mismas con L mantenido
    pub rear: [Option<u8>; 6],
    pub rear_l: [Option<u8>; 6],
    /// combinaciones: botones que hay que mantener a la vez → teclas que se pulsan juntas
    pub combos: Vec<(Vec<Btn>, Vec<u8>)>,
}

pub fn parse(text: &str) -> Keymap {
    let mut km = Keymap {
        guide: Vec::new(),
        base: HashMap::new(),
        layer_l: HashMap::new(),
        layer_r: HashMap::new(),
        layer_select: HashMap::new(),
        // panel trasero desactivado por defecto (roces accidentales); las cifras van en la barra en pantalla
        rear: [None; 6],
        rear_l: [None; 6],
        combos: Vec::new(),
    };
    let mut section = String::new();
    for raw in text.lines() {
        let line = raw.split(';').next().unwrap_or("").trim();
        if line.starts_with('[') {
            section = line.trim_matches(|c| c == '[' || c == ']').to_ascii_lowercase();
            continue
        }
        let Some((k, v)) = line.split_once('=') else { continue };
        let comment = raw.split_once(';').map(|(_, c)| c.trim().to_string()).unwrap_or_default();
        if !matches!(section.as_str(), "pantalla") && !k.trim().eq_ignore_ascii_case("LSTICK") {
            km.guide.push((section.clone(), k.trim().to_string(), v.trim().to_string(), comment));
        }
        let (k, v) = (k.trim().to_ascii_uppercase(), v.trim());
        match section.as_str() {
            "base" | "capa_l" | "capa_r" | "capa_select" => {
                if let (Some(b), Some(vk)) = (btn_name(&k), vk_name(v)) {
                    match section.as_str() {
                        "base" => km.base.insert(b, vk),
                        "capa_l" => km.layer_l.insert(b, vk),
                        "capa_select" => km.layer_select.insert(b, vk),
                        _ => km.layer_r.insert(b, vk),
                    };
                }
            },
            "combos" => {
                let btns: Option<Vec<Btn>> = k.split('+').map(|b| btn_name(b.trim())).collect();
                let keys: Option<Vec<u8>> = v.split('+').map(vk_name).collect();
                if let (Some(b), Some(k)) = (btns, keys) {
                    if b.len() >= 2 && !k.is_empty() {
                        km.combos.push((b, k));
                    }
                }
            },
            "tactil" => {
                // REAR_1..REAR_6 (arriba izq→der, abajo izq→der) y REAR_L1..REAR_L6 con L mantenido
                let (l, n) = if let Some(n) = k.strip_prefix("REAR_L") {
                    (true, n)
                } else if let Some(n) = k.strip_prefix("REAR_") {
                    (false, n)
                } else {
                    continue
                };
                let Ok(i) = n.parse::<usize>() else { continue };
                if (1..=6).contains(&i) {
                    let vk = if v.eq_ignore_ascii_case("NADA") { None } else { vk_name(v) };
                    if l { km.rear_l[i - 1] = vk } else { km.rear[i - 1] = vk }
                }
            },
            _ => (),
        }
    }
    km
}

/// Nombre de botón para la guía
pub fn btn_label(b: &str) -> String {
    b.split('+')
        .map(|p| match p.trim().to_ascii_uppercase().as_str() {
            "CROSS" => "Cruz",
            "CIRCLE" => "Círculo",
            "SQUARE" => "Cuadrado",
            "TRIANGLE" => "Triángulo",
            "DPAD_UP" => "Arriba",
            "DPAD_DOWN" => "Abajo",
            "DPAD_LEFT" => "Izquierda",
            "DPAD_RIGHT" => "Derecha",
            "START" => "START",
            "SELECT" => "SELECT",
            "L" => "L",
            "R" => "R",
            "FRONT_TOUCH" => "Pantalla táctil",
            "TECLADO_VIRTUAL" => "Teclado del sistema",
            other => if other.starts_with("REAR_") { "Panel trasero" } else { "?" },
        }
        .to_string())
        .collect::<Vec<_>>()
        .join(" + ")
}


/// Secciones cuyas entradas se pueden cambiar desde el menú
fn editable(sec: &str, btn: &str) -> bool {
    matches!(sec, "base" | "capa_l" | "capa_r" | "capa_select" | "combos") && !btn.eq_ignore_ascii_case("LSTICK")
}

fn keys_text(keys: &str) -> String {
    match keys.to_ascii_uppercase().as_str() {
        "GUIA" => "guía de controles".to_string(),
        "BARRA" => "barra de números".to_string(),
        "MOUSE" => "ratón".to_string(),
        "NADA" | "" => "(nada)".to_string(),
        _ => format!("tecla {}", keys),
    }
}

/// Acción de cada tecla en el juego, sacada del mapa original (no del archivo del usuario, cuyos
/// comentarios dejan de corresponder al cambiar teclas) y completada con las teclas sin botón
pub fn key_action(key: &str) -> String {
    use std::sync::OnceLock;
    static TABLE: OnceLock<HashMap<String, String>> = OnceLock::new();
    let t = TABLE.get_or_init(|| {
        let mut m = HashMap::new();
        for (sec, _, keys, d) in parse(DEFAULT_KEYMAP).guide {
            if sec != "tactil" && !d.is_empty() {
                m.entry(keys.to_ascii_uppercase()).or_insert(d);
            }
        }
        for (k, d) in [
            ("NADA", "Sin función"),
            ("GUIA", "Abrir esta guía"),
            ("BARRA", "Mostrar/ocultar la barra de números"),
            ("F8", "Móvil (key_menu_movil)"),
            ("ESCAPE", "Salir (key_global_salir)"),
            ("1", "Pokémon 1 en combate"),
            ("2", "Pokémon 2 en combate"),
            ("3", "Pokémon 3 en combate"),
            ("4", "Pokémon 4 en combate"),
            ("5", "Pokémon 5 en combate / MO"),
            ("6", "Pokémon 6 en combate / MO"),
            ("7", "MO"),
            ("8", "Habilidades externas"),
            ("9", "Xpod"),
            ("0", "Comandos (key_menu_comandos)"),
            ("L", "Áreas"),
            ("BACKSLASH", "Ver Pokémon"),
            ("F4", "Pantalla completa (sin uso en Vita)"),
            ("F7", "Escala de la ventana"),
            ("F9", "Captura de pantalla"),
            ("F10", "Música sí/no"),
            ("UP", "Arriba"),
            ("DOWN", "Abajo"),
            ("LEFT", "Izquierda"),
            ("RIGHT", "Derecha"),
        ] {
            m.entry(k.to_string()).or_insert_with(|| d.to_string());
        }
        m
    });
    t.get(&key.trim().to_ascii_uppercase()).cloned().unwrap_or_default()
}

impl Keymap {
    /// Líneas de la guía y, para cada línea, la entrada (índice en `guide`) que representa
    pub fn guide_lines(&self) -> (Vec<String>, Vec<Option<usize>>) {
        let mut out = Vec::new();
        let mut map = Vec::new();
        let mut last = String::new();
        for (i, (sec, btn, keys, desc)) in self.guide.iter().enumerate() {
            if btn.to_ascii_uppercase().starts_with("REAR_") {
                continue
            }
            if *sec != last {
                if !out.is_empty() {
                    out.push(String::new());
                    map.push(None);
                }
                out.push(match sec.as_str() {
                    "base" => "BOTONES".to_string(),
                    "capa_l" => "MANTENIENDO L".to_string(),
                    "capa_r" => "MANTENIENDO R".to_string(),
                    "capa_select" => "MANTENIENDO SELECT".to_string(),
                    "combos" => "COMBINACIONES (pulsar a la vez)".to_string(),
                    "tactil" => "PANTALLA TÁCTIL".to_string(),
                    other => other.to_uppercase(),
                });
                map.push(None);
                last = sec.clone();
            }
            let action = if editable(sec, btn) { key_action(keys) } else { String::new() };
            let desc = if action.is_empty() { desc.clone() } else { action };
            let line = format!("  {:<28} {:<20} {}", btn_label(btn), keys_text(keys), desc);
            out.push(line.chars().take(92).collect());
            map.push(if editable(sec, btn) { Some(i) } else { None });
        }
        out.push(String::new());
        map.push(None);
        out.push("BARRA DE NÚMEROS (L+R+Cuadrado o L+R+SELECT): 1-6 a la izquierda, 7 8 9 0 \\ L a la derecha".into());
        map.push(None);
        (out, map)
    }

    /// Teclas que se pueden asignar, con lo que hace cada una en el juego (según los comentarios)
    pub fn key_choices(&self) -> Vec<(String, String)> {
        let mut names: Vec<String> = vec!["NADA".into(), "GUIA".into(), "BARRA".into()];
        for n in ["UP", "DOWN", "LEFT", "RIGHT", "ENTER", "SPACE", "SHIFT", "CTRL", "ESCAPE", "BACKSPACE", "TAB",
            "PAGEUP", "PAGEDOWN", "HOME", "END", "BACKSLASH", "CTRL+P"]
        {
            names.push(n.into());
        }
        for c in ('A'..='Z').chain('0'..='9') {
            names.push(c.to_string());
        }
        for f in 1..=12 {
            names.push(format!("F{}", f));
        }
        names.into_iter().map(|n| { let d = key_action(&n); (n, d) }).collect()
    }
}

/// Cambia el valor de `boton` en la sección `sec` de un keymap.ini conservando el resto del archivo
/// (comentarios y alineación)
pub fn set_entry(text: &str, sec: &str, btn: &str, value: &str, desc: &str) -> String {
    let mut section = String::new();
    let mut out = Vec::new();
    let mut done = false;
    for raw in text.lines() {
        let code = raw.split(';').next().unwrap_or("").trim();
        if code.starts_with('[') {
            section = code.trim_matches(|c| c == '[' || c == ']').to_ascii_lowercase();
        } else if !done && section == sec {
            if let Some((k, _)) = code.split_once('=') {
                if k.trim().eq_ignore_ascii_case(btn) {
                    let (lhs, rest) = raw.split_once('=').unwrap();
                    let mut line = format!("{}= {}", lhs, value);
                    // el comentario pasa a describir la tecla nueva (lo que la guía muestra como acción)
                    let comment = if desc.is_empty() { "; (sin acción conocida)".to_string() } else { format!("; {}", desc) };
                    {
                        let pos = rest.find(';').unwrap_or(rest.trim_end().len().max(14));
                        let col = lhs.len() + 1 + pos;
                        while line.len() < col {
                            line.push(' ');
                        }
                        if !line.ends_with(' ') {
                            line.push(' ');
                        }
                        line.push_str(&comment);
                    }
                    out.push(line);
                    done = true;
                    continue
                }
            }
        }
        out.push(raw.to_string());
    }
    let mut s = out.join("\n");
    s.push('\n');
    s
}

impl Game {
    /// Guía de controles con cambio de botones. `text` = keymap.ini en uso; devuelve el texto nuevo si cambió
    /// (ya guardado en `path`).
    pub fn rld_controls_menu(&mut self, text: &str, path: &std::path::Path) -> Option<String> {
        let mut text = text.to_string();
        let mut changed = false;
        let mut sel_line: Option<usize> = None;
        let mut msg = String::new();
        loop {
            let km = parse(&text);
            let (lines, map) = km.guide_lines();
            let selectable: Vec<bool> = map.iter().map(|m| m.is_some()).collect();
            let first = selectable.iter().position(|&s| s).unwrap_or(0);
            let sel = sel_line.filter(|&l| selectable.get(l).copied().unwrap_or(false)).unwrap_or(first);
            let title = if msg.is_empty() { "Guía de controles (PS Vita)".to_string() } else { format!("Guía de controles - {}", msg) };
            let page = OverlayPage {
                title,
                scroll: sel.saturating_sub(crate::render::PAGE_LINES / 2),
                lines,
                footer: "Cruz: cambiar botón   Triangulo: valores originales   Circulo: cerrar".into(),
                selected: Some(sel),
                selectable,
                ..Default::default()
            };
            let Some((kind, line)) = self.run_menu(page) else { break };
            sel_line = Some(line);
            if kind == 't' {
                text = DEFAULT_KEYMAP.to_string();
                changed = true;
                msg = "restaurados los controles originales".into();
                let _ = std::fs::write(path, &text);
                continue
            }
            let Some(entry) = map.get(line).copied().flatten() else { continue };
            let (sec, btn, cur, _) = km.guide[entry].clone();
            // elegir tecla
            let choices = km.key_choices();
            let cur_u = cur.to_ascii_uppercase();
            let csel = choices.iter().position(|(n, _)| *n == cur_u).unwrap_or(0);
            let clines: Vec<String> =
                choices.iter().map(|(n, d)| format!("  {:<12} {}", n, d).chars().take(92).collect()).collect();
            let n = clines.len();
            let page = OverlayPage {
                title: format!("{} ({}) = ?   ahora: {}", btn_label(&btn), sec_name(&sec), cur),
                scroll: csel.saturating_sub(crate::render::PAGE_LINES / 2),
                lines: clines,
                footer: "Cruz: asignar   Circulo: volver".into(),
                selected: Some(csel),
                selectable: vec![true; n],
                ..Default::default()
            };
            if let Some(('a', k)) = self.run_menu(page) {
                let value = &choices[k].0;
                text = set_entry(&text, &sec, &btn, value, &choices[k].1);
                changed = true;
                msg = match std::fs::write(path, &text) {
                    Ok(()) => format!("{} = {} (guardado)", btn_label(&btn), value),
                    Err(e) => format!("no se pudo guardar: {}", e),
                };
            }
        }
        if changed { Some(text) } else { None }
    }
}

fn sec_name(sec: &str) -> &'static str {
    match sec {
        "base" => "solo",
        "capa_l" => "con L",
        "capa_r" => "con R",
        "capa_select" => "con SELECT",
        "combos" => "combinación",
        _ => "",
    }
}
