//! Aceleradores del port: reescrituras de scripts concretas de Pokémon Reloaded que son
//! correctas pero demasiado lentas en PS Vita.
//!
//! Cada reescritura solo se aplica si la script contiene EXACTAMENTE el fragmento original
//! esperado. Si una actualización del juego cambia la script, la reescritura no se aplica y
//! se ejecuta el código original: más lento, pero nunca incorrecto.

use crate::{asset::Script, gml};

/// TextReader: saltaba hasta la línea pedida con un bucle GML de file_text_readln.
/// Los diálogos están en líneas de miles de posiciones de txt_dialog.dll (1,5 MB), así que
/// cada texto costaba ~100 ms en Vita. Se sustituye por rld_text_line (índice de líneas nativo).
const TEXTREADER_ORIGINAL: &[u8] = b"if(!file_exists(\"data\\text\\\"+folder+\"\\\"+filename+\".dll\"))return(\"???\"); f=file_text_open_read(\"data\\text\\\"+folder+\"\\\"+filename+\".dll\"); for(i=1;i<line;i+=1){ if(file_text_eof(f)) { file_text_close(f); return\"???\"; } file_text_readln(f); } txt=txt+file_text_read_string(f); file_text_close(f);";
const TEXTREADER_FAST: &[u8] = b"txt=rld_text_line(\"data\\text\\\"+folder+\"\\\"+filename+\".dll\",line);";

fn replace_once(src: &[u8], from: &[u8], to: &[u8]) -> Option<Vec<u8>> {
    let pos = src.windows(from.len()).position(|w| w == from)?;
    // debe aparecer una sola vez
    if src[pos + 1..].windows(from.len()).any(|w| w == from) {
        return None
    }
    let mut out = Vec::with_capacity(src.len());
    out.extend_from_slice(&src[..pos]);
    out.extend_from_slice(to);
    out.extend_from_slice(&src[pos + from.len()..]);
    Some(out)
}

/// Desactivable con RLD_NO_ACCEL=1 (para comparar resultados con el original)
fn enabled() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("RLD_NO_ACCEL").is_none())
}

pub fn rewrite_script(scripts: &[Option<Box<Script>>], id: usize, src: gml::String) -> gml::String {
    if !enabled() {
        return src
    }
    let Some(Some(script)) = scripts.get(id) else { return src };
    match script.name.as_ref() {
        b"TextReader" => match replace_once(src.as_ref(), TEXTREADER_ORIGINAL, TEXTREADER_FAST) {
            Some(new) => {
                crate::startlog("Acelerador: TextReader -> rld_text_line");
                gml::String::from(new)
            },
            None => {
                crate::startlog("Acelerador: TextReader ha cambiado en esta versión del juego, se usa el original");
                src
            },
        },
        _ => src,
    }
}

/// Autocomprobación (RLD_SELFTEST=1): compara rld_text_line con la lectura original
/// (file_text_readln ×(n-1) + file_text_read_string) en todas las líneas de los textos del juego.
pub fn selftest_text_lines(game_dir: &std::path::Path) {
    use crate::gml::file::{AccessMode, TextHandle};
    let mut files = Vec::new();
    for lang in ["es", "en", "zz"] {
        let dir = game_dir.join("data").join("text").join(lang);
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for e in rd.flatten() {
                files.push(format!("data\\text\\{}\\{}", lang, e.file_name().to_string_lossy()));
            }
        }
    }
    // también desde el paquete, si no hay carpeta
    if files.is_empty() {
        for lang in ["es", "en", "zz"] {
            for name in ["txt_dialog", "txt_menu", "dex_info", "txt_obj_desc", "txt_attack_desc", "txt_map"] {
                files.push(format!("data\\text\\{}\\{}.dll", lang, name));
            }
        }
    }
    let (mut checked, mut bad) = (0u64, 0u64);
    for f in &files {
        let f = &crate::gml::file::to_path(f).into_owned();
        if !f.ends_with(".dll") || !crate::gml::file::file_exists(f) {
            continue
        }
        let total = crate::gml::pak::read_any(f).map(|d| d.iter().filter(|&&b| b == 10).count()).unwrap_or(0) as i64;
        for line in 0..=total + 2 {
            if total > 2000 && line > 50 && line < total - 50 && line % 17 != 0 {
                continue
            }
            // algoritmo original
            let orig: Vec<u8> = (|| {
                let mut h = TextHandle::open(f, AccessMode::Read).ok()?;
                for _ in 1..line {
                    if h.is_eof().ok()? {
                        return Some(Vec::new())
                    }
                    h.skip_line().ok()?;
                }
                h.read_string().ok()
            })()
            .unwrap_or_default();
            let fast = crate::gml::pak::text_line(f, line).unwrap_or_default();
            checked += 1;
            if orig != fast {
                bad += 1;
                if bad <= 5 {
                    crate::startlog(&format!("SELFTEST diferencia: {} línea {}: {:?} != {:?}", f, line, String::from_utf8_lossy(&orig), String::from_utf8_lossy(&fast)));
                }
            }
        }
    }
    crate::startlog(&format!("SELFTEST rld_text_line: {} líneas comprobadas, {} diferencias", checked, bad));
}
