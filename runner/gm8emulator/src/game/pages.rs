//! Páginas a pantalla completa que dibuja el propio runner (ver render::OVERLAY_PAGE):
//! - `show_info` / `load_info`: el manual del juego (GameInfo.rtf, GameKeys.rtf…), que en Windows
//!   abre una ventana aparte con el RTF. Aquí se convierte a texto y se muestra con desplazamiento.
//! - `show_image`: imágenes informativas (tabla de tipos, ataques, grupos huevo…).
//! - Guía de controles de la Vita (desde el juego con L+R+○).
//!
//! Mientras una página está abierta el juego espera, igual que con la ventana modal de GM8.

use crate::{
    game::Game,
    gml::{self, file},
    render::{OverlayPage, OVERLAY_PAGE, PAGE_LINES},
};

/// Ancho de línea en caracteres (pantalla de 960 px con letra de 10 px y márgenes)
const COLS: usize = 92;

static INFO_TEXT: std::sync::Mutex<Option<(String, String)>> = std::sync::Mutex::new(None);

/// RTF (como lo guarda Word/WordPad) → texto plano: párrafos, tabulaciones, acentos (\'e1, \uN)
pub fn rtf_to_text(rtf: &[u8]) -> String {
    const SKIP: &[&str] = &[
        "fonttbl", "colortbl", "stylesheet", "info", "pict", "listtable", "listoverridetable", "rsidtbl", "generator",
        "xmlnstbl", "themedata", "colorschememapping", "latentstyles", "datastore", "mmathPr", "pgdsctbl", "header",
        "headerl", "headerr", "headerf", "footer", "footerl", "footerr", "footerf", "object", "fldinst", "filetbl",
        "revtbl", "wgrffmtfilter", "fchars", "lchars", "defchp", "defpap", "pnseclvl", "xe", "tc", "bkmkstart",
        "bkmkend", "nonshppict", "shp",
    ];
    let mut out: Vec<u8> = Vec::new();
    let mut stack: Vec<(bool, usize)> = Vec::new();
    let mut skip = false;
    let mut uc: usize = 1; // caracteres de respaldo tras \uN
    let mut pending_skip_chars = 0usize;
    let mut i = 0;
    let n = rtf.len();
    let mut push_char = |out: &mut Vec<u8>, skip: bool, pending: &mut usize, c: u8| {
        if skip {
            return
        }
        if *pending > 0 {
            *pending -= 1;
            return
        }
        out.push(c);
    };
    while i < n {
        let c = rtf[i];
        match c {
            b'{' => {
                stack.push((skip, uc));
                if rtf.get(i + 1) == Some(&b'\\') && rtf.get(i + 2) == Some(&b'*') {
                    skip = true;
                }
                i += 1;
            },
            b'}' => {
                if let Some((s, u)) = stack.pop() {
                    skip = s;
                    uc = u;
                }
                i += 1;
            },
            b'\\' => {
                i += 1;
                let Some(&d) = rtf.get(i) else { break };
                if d.is_ascii_alphabetic() {
                    let s = i;
                    while i < n && rtf[i].is_ascii_alphabetic() {
                        i += 1;
                    }
                    let word = std::str::from_utf8(&rtf[s..i]).unwrap_or("");
                    let ps = i;
                    if i < n && (rtf[i] == b'-' || rtf[i].is_ascii_digit()) {
                        i += 1;
                        while i < n && rtf[i].is_ascii_digit() {
                            i += 1;
                        }
                    }
                    let param: Option<i32> = std::str::from_utf8(&rtf[ps..i]).ok().and_then(|p| p.parse().ok());
                    if i < n && rtf[i] == b' ' {
                        i += 1;
                    }
                    match word {
                        w if SKIP.contains(&w) => skip = true,
                        "par" | "line" | "sect" | "page" => {
                            if !skip {
                                out.push(b'\n');
                            }
                        },
                        "tab" | "cell" => {
                            if !skip {
                                out.extend_from_slice(b"    ");
                            }
                        },
                        "row" => {
                            if !skip {
                                out.push(b'\n');
                            }
                        },
                        "bullet" => push_char(&mut out, skip, &mut pending_skip_chars, 0x95),
                        "emdash" | "endash" => push_char(&mut out, skip, &mut pending_skip_chars, b'-'),
                        "lquote" | "rquote" => push_char(&mut out, skip, &mut pending_skip_chars, b'\''),
                        "ldblquote" | "rdblquote" => push_char(&mut out, skip, &mut pending_skip_chars, b'"'),
                        "uc" => uc = param.unwrap_or(1).max(0) as usize,
                        "u" => {
                            if !skip {
                                let cp = param.unwrap_or(63);
                                let cp = if cp < 0 { cp + 65536 } else { cp } as u32;
                                let ch = char::from_u32(cp).unwrap_or('?');
                                let mut buf = [0u8; 4];
                                let st = ch.encode_utf8(&mut buf);
                                let (b, _, _) = encoding_rs::WINDOWS_1252.encode(st);
                                out.extend_from_slice(&b);
                                pending_skip_chars = uc;
                            }
                        },
                        _ => (),
                    }
                } else {
                    match d {
                        b'\'' => {
                            let hex = rtf.get(i + 1..i + 3).and_then(|h| std::str::from_utf8(h).ok());
                            if let Some(v) = hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                                push_char(&mut out, skip, &mut pending_skip_chars, v);
                            }
                            i += 3;
                        },
                        b'~' => {
                            push_char(&mut out, skip, &mut pending_skip_chars, b' ');
                            i += 1;
                        },
                        b'*' => i += 1,
                        b'\n' | b'\r' => {
                            if !skip {
                                out.push(b'\n');
                            }
                            i += 1;
                        },
                        other => {
                            if other != b'-' && other != b'_' {
                                push_char(&mut out, skip, &mut pending_skip_chars, other);
                            }
                            i += 1;
                        },
                    }
                }
            },
            b'\r' | b'\n' => i += 1,
            other => {
                push_char(&mut out, skip, &mut pending_skip_chars, other);
                i += 1;
            },
        }
    }
    let (text, _, _) = encoding_rs::WINDOWS_1252.decode(&out);
    text.into_owned()
}

/// Corta el texto en líneas de `COLS` caracteres (por palabras) y quita líneas vacías repetidas
pub fn wrap(text: &str) -> Vec<String> {
    let mut lines = Vec::new();
    for para in text.split('\n') {
        let para = para.trim_end();
        if para.is_empty() {
            if lines.last().map_or(false, |l: &String| !l.is_empty()) {
                lines.push(String::new());
            }
            continue
        }
        let mut cur = String::new();
        for word in para.split(' ') {
            if cur.chars().count() + word.chars().count() + 1 > COLS && !cur.is_empty() {
                lines.push(std::mem::take(&mut cur));
            }
            if !cur.is_empty() {
                cur.push(' ');
            }
            cur.push_str(word);
            while cur.chars().count() > COLS {
                let head: String = cur.chars().take(COLS).collect();
                let tail: String = cur.chars().skip(COLS).collect();
                lines.push(head);
                cur = tail;
            }
        }
        lines.push(cur);
    }
    while lines.last().map_or(false, |l| l.is_empty()) {
        lines.pop();
    }
    lines
}

fn page_open() -> bool {
    OVERLAY_PAGE.lock().map_or(false, |p| p.is_some())
}

/// Mueve la página abierta: +n / -n líneas
pub fn page_scroll(delta: i32) {
    if let Ok(mut p) = OVERLAY_PAGE.lock() {
        if let Some(p) = p.as_mut() {
            let max = p.lines.len().saturating_sub(PAGE_LINES) as i32;
            p.scroll = (p.scroll as i32 + delta).clamp(0, max) as usize;
        }
    }
}

pub fn page_close() {
    if let Ok(mut p) = OVERLAY_PAGE.lock() {
        *p = None;
    }
}

impl Game {
    /// Muestra una página y espera a que el jugador la cierre (○, ✕, START; en PC X, Z, Enter, Esc)
    pub fn run_page_modal(&mut self, page: OverlayPage) -> gml::Result<()> {
        if let Ok(mut p) = OVERLAY_PAGE.lock() {
            *p = Some(page);
        }
        // si hay benchmark no se espera (pruebas sin pantalla)
        if self.bench.is_some() && std::env::var_os("RLD_PAGE_WAIT").is_none() {
            self.renderer.present(self.window_inner_size.0, self.window_inner_size.1, self.scaling);
            page_close();
            return Ok(())
        }
        self.input.keyboard_clear_all();
        while page_open() {
            self.process_window_events();
            // teclado de PC (en la Vita lo gestiona vita_input)
            for vk in [0x58u8, 0x5A, 0x0D, 0x1B] {
                if self.input.keyboard_check_pressed(vk) {
                    page_close();
                }
            }
            if self.input.keyboard_check_pressed(0x28) {
                page_scroll(1);
            }
            if self.input.keyboard_check_pressed(0x26) {
                page_scroll(-1);
            }
            if self.input.keyboard_check_pressed(0x22) {
                page_scroll(PAGE_LINES as i32);
            }
            if self.input.keyboard_check_pressed(0x21) {
                page_scroll(-(PAGE_LINES as i32));
            }
            self.input.step();
            self.renderer.present(self.window_inner_size.0, self.window_inner_size.1, self.scaling);
            #[cfg(not(target_os = "vita"))]
            std::thread::sleep(std::time::Duration::from_millis(16));
        }
        // que la tecla de cerrar no llegue al juego
        self.input.keyboard_clear_all();
        Ok(())
    }

    /// load_info(fname): guarda el texto del manual para el siguiente show_info
    pub fn rld_load_info(&mut self, fname: &str) {
        let path = file::to_path(fname);
        let text = file::read_any(path.as_ref()).map(|b| rtf_to_text(&b)).unwrap_or_default();
        let title = std::path::Path::new(fname.replace('\\', "/").as_str())
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        if let Ok(mut t) = INFO_TEXT.lock() {
            *t = Some((title, text));
        }
    }

    /// show_info(): muestra el manual cargado con load_info (o la información del juego del .exe)
    pub fn rld_show_info(&mut self) -> gml::Result<()> {
        let (title, text) = INFO_TEXT
            .lock()
            .ok()
            .and_then(|t| t.clone())
            .unwrap_or_else(|| ("Información del juego".into(), String::new()));
        let mut lines = wrap(&text);
        if lines.is_empty() {
            lines.push("(sin información)".into());
        }
        self.run_page_modal(OverlayPage {
            title: format!("Manual: {}", title),
            lines,
            footer: "Arriba/Abajo: desplazar   L/R: pagina   Circulo: cerrar".into(),
            ..Default::default()
        })
    }

    /// show_image(fname, full, delay): muestra una imagen hasta que se pulse un botón
    pub fn rld_show_image(&mut self, fname: &str) -> gml::Result<()> {
        let path = file::to_path(fname);
        let Ok(bytes) = file::read_any(path.as_ref()) else { return Ok(()) };
        let Ok(img) = image::load_from_memory(&bytes) else { return Ok(()) };
        let rgba = img.to_rgba8();
        let (w, h) = (rgba.width() as i32, rgba.height() as i32);
        let Ok(r) = self.renderer.upload_sprite(rgba.into_raw().into_boxed_slice(), w, h, 0, 0) else { return Ok(()) };
        let name = std::path::Path::new(fname.replace('\\', "/").as_str())
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let res = self.run_page_modal(OverlayPage {
            title: name,
            lines: Vec::new(),
            footer: "Circulo: cerrar".into(),
            image: Some(r),
            ..Default::default()
        });
        self.renderer.delete_sprite(r);
        res
    }
}
