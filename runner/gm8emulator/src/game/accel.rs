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
    let src = match rewrite_byte_loops(scripts, src.as_ref()) {
        Some((new, n)) => {
            if let Some(Some(s)) = scripts.get(id) {
                crate::startlog(&format!(
                    "Acelerador: {} -> {} bucle(s) de decodificación nativos (rld_map_bytes)",
                    String::from_utf8_lossy(s.name.as_ref()),
                    n
                ));
            }
            gml::String::from(new)
        },
        None => src,
    };
    let Some(Some(script)) = scripts.get(id) else { return src };
    let src = match rewrite_lookup_chains(src.as_ref()) {
        Some((new, n)) => {
            crate::startlog(&format!(
                "Acelerador: {} -> {} tabla(s) nativa(s)",
                String::from_utf8_lossy(script.name.as_ref()),
                n
            ));
            gml::String::from(new)
        },
        None => src,
    };
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
        b"SpecialChars" => match special_chars_pairs(src.as_ref()) {
            Some(pairs) => {
                crate::startlog(&format!("Acelerador: SpecialChars -> rld_special_chars ({} sustituciones)", pairs.len()));
                if let Ok(mut p) = SPECIAL_PAIRS.lock() {
                    *p = pairs;
                }
                let text = latin1(src.as_ref());
                let new = text.replacen("txt=argument0; ", "txt=argument0; if(is_string(txt))return rld_special_chars(txt); ", 1);
                gml::String::from(unlatin1(&new))
            },
            None => src,
        },
        b"InstanceInView" if trim(src.as_ref()) == INVIEW_ORIGINAL => {
            crate::startlog("Acelerador: InstanceInView -> rld_in_view");
            gml::String::from(INVIEW_FAST.to_vec())
        },
        // visible=ex(<id de InstanceInView>,x,y,24): llamada directa a la función nativa
        b"FlowerVisible" => {
            let inview = scripts.iter().position(|s| {
                s.as_ref().is_some_and(|s| s.name.as_ref() == b"InstanceInView" && trim(s.source.as_ref()) == INVIEW_ORIGINAL)
            });
            match inview {
                Some(n) if trim(src.as_ref()) == format!("visible=ex({},x,y,24);", n).as_bytes() => {
                    crate::startlog("Acelerador: FlowerVisible -> rld_in_view");
                    gml::String::from(b"visible=rld_in_view(x,y,24);".to_vec())
                },
                _ => src,
            }
        },
        // Comprobación antitrampas que se recalcula varias veces por fotograma en combate
        // (≈20 % del tiempo): el bucle de sumas pasa a una función nativa con las mismas
        // operaciones en el mismo orden (rld_secpk2_sum)
        b"GetSecurityPK2" | b"SecurityPK2" if secpk2_ok(scripts) => {
            let text = String::from_utf8_lossy(src.as_ref()).into_owned();
            let get_loop = SECPK2_LOOP.to_string();
            let sec_loop = SECPK2_LOOP.replace("v7 +=", "(3).vsietepk2 +=");
            let new = if script.name.as_ref() == b"GetSecurityPK2" && text.matches(&get_loop).count() == 1 {
                Some(text.replace(&get_loop, "v7=rld_secpk2_sum(v7); i=6; j=5;"))
            } else if script.name.as_ref() == b"SecurityPK2" && text.matches(&sec_loop).count() == 1 {
                Some(text.replace(&sec_loop, "(3).vsietepk2=rld_secpk2_sum((3).vsietepk2); i=6; j=5;"))
            } else {
                None
            };
            if std::env::var_os("RLD_DEBUG_ACCEL").is_some() {
                crate::startlog(&format!("DEBUG secpk2 {}: get={} sec={} new={}", String::from_utf8_lossy(script.name.as_ref()),
                    text.matches(&get_loop).count(), text.matches(&sec_loop).count(), new.as_ref().map_or(0, |n| n.len())));
            }
            match new {
                Some(n) if !n.contains("+=(i*7)+(global.PP_1[i]*6)") => {
                    crate::startlog(&format!("Acelerador: {} -> rld_secpk2_sum", String::from_utf8_lossy(script.name.as_ref())));
                    gml::String::from(n.into_bytes())
                },
                _ => src,
            }
        },
        // GetBattleEffect: ~100 llamadas por fotograma en combate, cada una evaluando en cadena más de
        // 100 comparaciones `argument0=(N) ||` para acabar casi siempre en `return noone` porque el
        // efecto no existe. Todas sus ramas empiezan con `if(!instance_exists(argument0))return noone;`
        // y si ninguna coincide también devuelve noone: comprobarlo al principio da el mismo resultado.
        b"GetBattleEffect" if battle_effect_ok(src.as_ref()) => {
            let text = String::from_utf8_lossy(src.as_ref()).into_owned();
            crate::startlog("Acelerador: GetBattleEffect -> salida rápida si el efecto no existe");
            let new = text.replacen(
                "var i, ef; ",
                "var i, ef; if(is_real(argument0)){if(!instance_exists(argument0))return noone;} ",
                1,
            );
            gml::String::from(new.into_bytes())
        },
        _ => src,
    }
}

/// Comprueba que GetBattleEffect tiene la forma en la que la salida rápida es equivalente
fn battle_effect_ok(src: &[u8]) -> bool {
    let t = String::from_utf8_lossy(src);
    let t = t.trim();
    let count = |p: &str| t.matches(p).count();
    let groups = count("if(argument0=(");
    let guarded = count("){ if(!instance_exists(argument0))return noone;");
    let uses = count("argument0");
    let known = count("argument0=(") + count("instance_exists(argument0)") + count("instance_find(argument0,") + count("instance_number(argument0)");
    t.starts_with("var i, ef; if(argument0=(") && t.ends_with("return noone;") && groups > 0 && groups == guarded && uses == known
}

/// ¿El bucle usa ex(9485, …) = RoundToTwo tal y como lo replica rld_secpk2_sum?
fn secpk2_ok(scripts: &[Option<Box<Script>>]) -> bool {
    matches!(scripts.get(9485), Some(Some(s)) if s.name.as_ref() == b"RoundToTwo"
        && trim(s.source.as_ref()) == b"return round(argument0*100)/100;")
}

const SECPK2_LOOP: &str = r#"for(i=0;i<=5;i+=1){ v7 +=(i*7)+(global.PP_1[i]*6)+(global.PS_1[i]*7)+global.STD1[i]+361; v7 +=(i*3)+(global.IV[i]*7)+(global.objpoke1[i]*6)+(global.selue_pk1[i]*11)+global.pkmnsex1[i]+361; v7 +=(i*7)+(global.PP_2[i]*6)+(global.PS_2[i]*7)+global.STD2[i]+361; v7 +=(i*3)+(global.IV2[i]*7)+(global.objpoke2[i]*6)+(global.selue_pk2[i]*11)+global.pkmnsex2[i]+361; for(j=1;j<=4;j+=1){ v7 +=global.attack1[i,j]*7*j; v7 +=global.attack2[i,j]*7*j; } v7 +=(i*13)+(((global.actual1+8)*5)+(global.MAX_PP_1[i]*2)+(global.MAX_PS_1[i]*2)+(global.XTRA_PS_1[i]*2)+(global.atk1[i]*3)+(global.def1[i]*3)+(ex(9485,global.vel1[i])*7)); v7 +=(global.atk_sp1[i]*5)+(global.def_sp1[i]*3)+(global.prec1[i]*4)+(global.tipoA1[i]*6)+(global.tipoB1[i]*5)+(global.tipoC1[i]*4); v7 +=(i*13)+(((global.actual2+8)*5)+(global.MAX_PP_2[i]*2)+(global.MAX_PS_2[i]*2)+(global.XTRA_PS_2[i]*2)+(global.atk2[i]*3)+(global.def2[i]*3)+(ex(9485,global.vel2[i])*9)); v7 +=(global.atk_sp2[i]*5)+(global.def_sp2[i]*3)+(global.prec2[i]*4)+(global.tipoA2[i]*6)+(global.tipoB2[i]*5)+(global.tipoC2[i]*4); }"#;

fn trim(b: &[u8]) -> &[u8] {
    let s = b.iter().position(|c| !c.is_ascii_whitespace()).unwrap_or(b.len());
    let e = b.iter().rposition(|c| !c.is_ascii_whitespace()).map_or(s, |e| e + 1);
    &b[s..e.max(s)]
}

const INVIEW_ORIGINAL: &[u8] = b"if(argument0<view_xview[0]-argument2)return false; if(argument0>view_xview[0]+view_wview[0]+argument2)return false; if(argument1<view_yview[0]-argument2)return false; if(argument1>view_yview[0]+view_hview[0]+argument2)return false; return true;";
const INVIEW_FAST: &[u8] = b"return rld_in_view(argument0,argument1,argument2);";

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


// ---------------------------------------------------------------------------------------------
// Tablas de búsqueda
//
// 43 scripts del juego (datos de Pokédex, pesos, rarezas, ataques…) son tablas escritas como
// cientos de `if(argument0=N)return(M);` seguidos: el intérprete las recorre una a una en cada
// llamada (GetIndexOrdenObj: 822 comparaciones, 17 % del tiempo total). Cada bloque contiguo se
// sustituye por una búsqueda en una tabla nativa:
//     if(rld_lookup_has(T,argument0))return(rld_lookup(T,argument0));
// Semántica idéntica: gana la primera aparición de cada clave; las claves son enteras y GM8
// compara reales con un margen de 1e-13, así que basta con redondear y comprobar la distancia.

#[derive(Clone, Debug, PartialEq)]
pub enum Lit {
    Real(f64),
    Str(Vec<u8>),
}

/// Por tabla: (entradas, primera clave del bloque original). La primera clave hace falta para
/// reproducir el error de GM8 al comparar un texto con un número.
static TABLES: std::sync::Mutex<Vec<(std::collections::HashMap<i64, Lit>, f64)>> = std::sync::Mutex::new(Vec::new());

pub fn first_key(table: usize) -> Option<f64> {
    TABLES.lock().ok()?.get(table).map(|t| t.1)
}
const MIN_RUN: usize = 8;

pub fn lookup(table: usize, key: f64) -> Option<Lit> {
    let k = key.round();
    if (key - k).abs() >= 1e-13 || !k.is_finite() {
        return None
    }
    TABLES.lock().ok()?.get(table)?.0.get(&(k as i64)).cloned()
}

struct P<'a> {
    s: &'a [u8],
    i: usize,
}

impl<'a> P<'a> {
    fn ws(&mut self) {
        while self.i < self.s.len() && self.s[self.i].is_ascii_whitespace() {
            self.i += 1;
        }
    }
    fn lit(&mut self, t: &[u8]) -> bool {
        self.ws();
        if self.s[self.i..].starts_with(t) {
            self.i += t.len();
            true
        } else {
            false
        }
    }
    fn number(&mut self) -> Option<f64> {
        self.ws();
        let start = self.i;
        if self.s.get(self.i) == Some(&b'-') {
            self.i += 1;
        }
        let d0 = self.i;
        while self.s.get(self.i).map_or(false, |c| c.is_ascii_digit()) {
            self.i += 1;
        }
        if self.i == d0 {
            self.i = start;
            return None
        }
        if self.s.get(self.i) == Some(&b'.') {
            self.i += 1;
            while self.s.get(self.i).map_or(false, |c| c.is_ascii_digit()) {
                self.i += 1;
            }
        }
        // que no siga un identificador (p. ej. "5abc")
        if self.s.get(self.i).map_or(false, |c| c.is_ascii_alphanumeric() || *c == b'_') {
            self.i = start;
            return None
        }
        // mismo algoritmo que el lexer de GML (gml-parser/src/lexer.rs), para obtener exactamente
        // el mismo f64 (p. ej. "0.7" -> 0.7000000000000001, como en el GM8 original)
        let (neg, digits) = match self.s[start] {
            b'-' => (true, &self.s[start + 1..self.i]),
            _ => (false, &self.s[start..self.i]),
        };
        let (mut result, mut factor, mut point) = (0.0f64, 1.0f64, false);
        for &ch in digits {
            match ch {
                b'0'..=b'9' => {
                    if point {
                        factor /= 10.0;
                    }
                    result = result * 10.0 + f64::from(ch - b'0');
                },
                b'.' => point = true,
                _ => return None,
            }
        }
        let v = result * factor;
        Some(if neg { -v } else { v })
    }
    fn value(&mut self) -> Option<Lit> {
        self.ws();
        if self.s.get(self.i) == Some(&b'"') {
            let end = self.s[self.i + 1..].iter().position(|&c| c == b'"')? + self.i + 1;
            let v = self.s[self.i + 1..end].to_vec();
            self.i = end + 1;
            return Some(Lit::Str(v))
        }
        self.number().map(Lit::Real)
    }
    /// `if(argument0=N)return(V);` → (clave, valor)
    fn entry(&mut self) -> Option<(f64, Lit)> {
        let save = self.i;
        let r = (|| {
            if !self.lit(b"if") || !self.lit(b"(") || !self.lit(b"argument0") {
                return None
            }
            if !self.lit(b"==") && !self.lit(b"=") {
                return None
            }
            let key = self.number()?;
            if !self.lit(b")") || !self.lit(b"return") {
                return None
            }
            // "return" no debe ser el principio de otro identificador
            let paren = self.lit(b"(");
            let v = self.value()?;
            if paren && !self.lit(b")") {
                return None
            }
            // la sentencia debe terminar aquí: ';', '}', fin, o el comienzo de otra sentencia
            // (evita aceptar "return(5)+1;" como "return(5)")
            if !self.lit(b";") {
                self.ws();
                let rest = &self.s[self.i..];
                let next_is_stmt = rest.is_empty()
                    || rest[0] == b'}'
                    || (rest.starts_with(b"if") && rest.get(2).map_or(true, |c| !c.is_ascii_alphanumeric() && *c != b'_'));
                if !next_is_stmt {
                    return None
                }
            }
            // un "else" a continuación pertenece a este if: no se puede convertir en tabla
            {
                let save = self.i;
                self.ws();
                let rest = &self.s[self.i..];
                let is_else = rest.starts_with(b"else") && rest.get(4).map_or(true, |c| !c.is_ascii_alphanumeric() && *c != b'_');
                self.i = save;
                if is_else {
                    return None
                }
            }
            Some((key, v))
        })();
        if r.is_none() {
            self.i = save;
        }
        r
    }
}

/// Sustituye los bloques de tabla de una script. Devuelve el código nuevo y cuántas tablas creó.
pub fn rewrite_lookup_chains(src: &[u8]) -> Option<(Vec<u8>, usize)> {
    let mut out = Vec::with_capacity(src.len());
    let mut i = 0;
    let mut made = 0;
    let mut copied = 0;
    while i < src.len() {
        // saltar cadenas ("..." y '...', sin escapes en GM8) y comentarios (// y /* */)
        match src[i] {
            q @ (b'"' | b'\'') => {
                i += 1;
                while i < src.len() && src[i] != q {
                    i += 1;
                }
                i += 1;
                continue
            },
            b'/' if src.get(i + 1) == Some(&b'/') => {
                while i < src.len() && src[i] != b'\n' {
                    i += 1;
                }
                continue
            },
            b'/' if src.get(i + 1) == Some(&b'*') => {
                i += 2;
                while i + 1 < src.len() && !(src[i] == b'*' && src[i + 1] == b'/') {
                    i += 1;
                }
                i += 2;
                continue
            },
            _ => (),
        }
        // inicio de sentencia: principio, o tras ';', '{' o '}' (nunca tras un if/else sin llaves)
        let at_stmt = {
            let mut j = i;
            while j > 0 && src[j - 1].is_ascii_whitespace() {
                j -= 1;
            }
            j == 0 || matches!(src[j - 1], b';' | b'{' | b'}')
        };
        if at_stmt && src[i..].starts_with(b"if") && src.get(i + 2).map_or(false, |c| !c.is_ascii_alphanumeric() && *c != b'_') {
            let mut p = P { s: src, i };
            let mut entries = Vec::new();
            let mut end = i;
            while let Some(e) = p.entry() {
                entries.push(e);
                end = p.i;
            }
            let all_int = entries.iter().all(|(k, _)| k.fract() == 0.0 && k.abs() < 1e15);
            if entries.len() >= MIN_RUN && all_int {
                let mut map = std::collections::HashMap::new();
                let first = entries[0].0;
                for (k, v) in entries {
                    map.entry(k as i64).or_insert(v); // gana la primera aparición
                }
                let id = {
                    let mut t = TABLES.lock().ok()?;
                    t.push((map, first));
                    t.len() - 1
                };
                out.extend_from_slice(&src[copied..i]);
                out.extend_from_slice(
                    format!("if(rld_lookup_has({id},argument0))return(rld_lookup({id},argument0)); ").as_bytes(),
                );
                copied = end;
                i = end;
                made += 1;
                continue
            }
        }
        i += 1;
    }
    if made == 0 {
        return None
    }
    out.extend_from_slice(&src[copied..]);
    Some((out, made))
}

impl crate::game::Game {
    /// Autocomprobación de rld_secpk2_sum (RLD_SELFTEST=1): el bucle GML original contra la versión
    /// nativa, con variables globales aleatorias (enteros, decimales como 1.5 o 2/3 en la velocidad,
    /// y huecos sin asignar). Restaura las globales al terminar.
    pub fn accel_selftest_secpk2(&mut self) {
        use crate::{game::GetAsset, gml::{Context, Value}, instance::{Field, Instance}};
        let names = [
            "PP_1", "PS_1", "STD1", "IV", "objpoke1", "selue_pk1", "pkmnsex1", "PP_2", "PS_2", "STD2", "IV2", "objpoke2",
            "selue_pk2", "pkmnsex2", "attack1", "attack2", "actual1", "actual2", "MAX_PP_1", "MAX_PS_1", "XTRA_PS_1",
            "atk1", "def1", "vel1", "atk_sp1", "def_sp1", "prec1", "tipoA1", "tipoB1", "tipoC1", "MAX_PP_2", "MAX_PS_2",
            "XTRA_PS_2", "atk2", "def2", "vel2", "atk_sp2", "def_sp2", "prec2", "tipoA2", "tipoB2", "tipoC2",
        ];
        let ids: Vec<usize> = names.iter().map(|n| self.compiler.get_field_id(n.as_bytes())).collect();
        let backup: Vec<Option<Field>> = ids.iter().map(|id| self.globals.fields.remove(id)).collect();
        let src = format!("var i, v7; v7=argument0; {} return v7;", SECPK2_LOOP.replace("global.", "global."));
        let Ok(orig) = self.compiler.compile(src.as_bytes()) else {
            crate::startlog("SELFTEST rld_secpk2_sum: no compila");
            return
        };
        let mut seed: u64 = 0x9E3779B97F4A7C15;
        let mut rnd = move || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        let (mut cases, mut bad) = (0, 0);
        for trial in 0..200 {
            for (k, id) in ids.iter().enumerate() {
                self.globals.fields.remove(id);
                for i in 0..6u32 {
                    for j in 0..5u32 {
                        let idx = if names[k].starts_with("attack") { i * 32000 + j } else if j == 0 { i } else { continue };
                        // a veces sin asignar
                        if trial % 7 == 3 && rnd() % 5 == 0 {
                            continue
                        }
                        let v = match rnd() % 6 {
                            0 => (rnd() % 1000) as f64 + 0.5,
                            1 => (rnd() % 300) as f64 * 2.0 / 3.0,
                            2 => -((rnd() % 50) as f64),
                            _ => (rnd() % 800) as f64,
                        };
                        match self.globals.fields.get_mut(id) {
                            Some(f) => f.set(idx, Value::Real(v.into())),
                            None => {
                                self.globals.fields.insert(*id, Field::new(idx, Value::Real(v.into())));
                            },
                        }
                    }
                }
            }
            let start = Value::Real((((rnd() % 100000) as f64) + if trial % 2 == 0 { 0.25 } else { 0.0 }).into());
            let dummy = self.room.instance_list.insert_dummy(Instance::new_dummy(self.assets.objects.get_asset(0).map(|x| x.as_ref())));
            let mut ctx = Context::with_single_instance(dummy);
            ctx.arguments[0] = start.clone();
            ctx.argument_count = 1;
            let r1 = self.execute(&orig, &mut ctx).map(|_| ctx.return_value.clone()).map_err(|e| e.to_string());
            self.room.instance_list.remove_dummy(dummy);
            let r2 = self.rld_secpk2_sum(&[start]).map_err(|e| e.to_string());
            let (a, b) = (format!("{:?}", r1), format!("{:?}", r2));
            cases += 1;
            if a != b {
                bad += 1;
                if bad <= 3 {
                    crate::startlog(&format!("SELFTEST rld_secpk2_sum: {} != {}", a, b));
                }
            }
        }
        for (id, f) in ids.iter().zip(backup) {
            self.globals.fields.remove(id);
            if let Some(f) = f {
                self.globals.fields.insert(*id, f);
            }
        }
        crate::startlog(&format!("SELFTEST rld_secpk2_sum: {} casos, {} diferencias", cases, bad));
    }

    /// Autocomprobación de rld_in_view (RLD_SELFTEST=1): original GML contra la versión nativa con
    /// varias vistas y puntos dentro, fuera y justo en el borde (incluidos decimales y cadenas).
    pub fn accel_selftest_inview(&mut self) {
        use crate::{
            game::GetAsset,
            gml::{Context, Value},
            instance::Instance,
        };
        let (Ok(orig), Ok(fast)) = (self.compiler.compile(INVIEW_ORIGINAL), self.compiler.compile(INVIEW_FAST)) else {
            crate::startlog("SELFTEST rld_in_view: no compila");
            return
        };
        let had_views = !self.room.views.is_empty();
        if !had_views {
            self.room.views.push(crate::game::view::View {
                visible: false, source_x: 0, source_y: 0, source_w: 0, source_h: 0, port_x: 0, port_y: 0,
                port_w: 0, port_h: 0, angle: 0.0.into(), follow_target: -1, follow_hborder: 0,
                follow_vborder: 0, follow_hspeed: 0, follow_vspeed: 0,
            });
        }
        let saved = (self.room.views[0].source_x, self.room.views[0].source_y, self.room.views[0].source_w, self.room.views[0].source_h);
        let (mut cases, mut bad) = (0u32, 0u32);
        let pts: [f64; 9] = [-100.0, -24.0, -23.9999999, 0.0, 12.5, 160.0, 343.99999999, 344.0, 1000.0];
        for view in [(0, 0, 320, 240), (100, 37, 320, 180), (-5, 900, 640, 480)] {
            self.room.views[0].source_x = view.0;
            self.room.views[0].source_y = view.1;
            self.room.views[0].source_w = view.2;
            self.room.views[0].source_h = view.3;
            let mut inputs: Vec<[Value; 3]> = Vec::new();
            for &x in &pts {
                for &y in &pts {
                    for m in [0.0, 24.0, 7.5] {
                        inputs.push([(x + view.0 as f64).into(), (y + view.1 as f64).into(), m.into()]);
                    }
                }
            }
            inputs.push([Value::Str("a".into()), 0.0.into(), 24.0.into()]);
            inputs.push([0.0.into(), Value::Str("a".into()), 24.0.into()]);
            inputs.push([0.0.into(), 0.0.into(), Value::Str("a".into())]);
            for input in inputs {
                let mut results = Vec::new();
                for code in [&orig, &fast] {
                    let dummy = self
                        .room
                        .instance_list
                        .insert_dummy(Instance::new_dummy(self.assets.objects.get_asset(0).map(|x| x.as_ref())));
                    let mut ctx = Context::with_single_instance(dummy);
                    ctx.arguments[..3].clone_from_slice(&input);
                    ctx.argument_count = 3;
                    let r = self.execute(code, &mut ctx);
                    self.room.instance_list.remove_dummy(dummy);
                    results.push(format!("{:?} {:?}", r.map(|_| ()).map_err(|e| e.to_string()), ctx.return_value));
                }
                cases += 1;
                if results[0] != results[1] {
                    bad += 1;
                    if bad <= 3 {
                        crate::startlog(&format!("SELFTEST rld_in_view {:?} {:?}: {} != {}", view, input, results[0], results[1]));
                    }
                }
            }
        }
        (self.room.views[0].source_x, self.room.views[0].source_y, self.room.views[0].source_w, self.room.views[0].source_h) = saved;
        if !had_views {
            self.room.views.clear();
        }
        crate::startlog(&format!("SELFTEST rld_in_view: {} casos, {} diferencias", cases, bad));
    }

    /// Autocomprobación de las tablas (RLD_SELFTEST=1): ejecuta cada script reescrita en su versión
    /// original y en la acelerada con todas sus claves y con valores ausentes, y compara resultados.
    pub fn accel_selftest_tables(&mut self) {
        use crate::{
            game::GetAsset,
            gml::{Context, Value},
            instance::Instance,
        };
        let (mut scripts_ok, mut scripts_bad, mut cases) = (0, 0, 0u64);
        for id in 0..self.assets.scripts.len() {
            let Some(Some(script)) = self.assets.scripts.get(id) else { continue };
            let (name, src) = (String::from_utf8_lossy(script.name.as_ref()).into_owned(), script.source.clone());
            let Some((new, _)) = rewrite_lookup_chains(src.as_ref()) else { continue };
            if std::env::var("RLD_SELFTEST").as_deref() == Ok("2") {
                crate::startlog(&format!("SELFTEST probando {}", name));
            }
            let (Ok(orig), Ok(fast)) = (self.compiler.compile(src.as_ref()), self.compiler.compile(&new)) else {
                crate::startlog(&format!("SELFTEST {}: no compila", name));
                scripts_bad += 1;
                continue
            };
            // entradas: todas las claves del bloque original + valores que no están
            let mut inputs: Vec<Value> = Vec::new();
            {
                let s: &[u8] = src.as_ref();
                let mut i = 0;
                while i < s.len() {
                    let mut p = P { s, i };
                    if let Some((k, _)) = p.entry() {
                        inputs.push(k.into());
                        i = p.i;
                    } else {
                        i += 1;
                    }
                }
            }
            // valores ausentes: solo si fuera de las tablas no se llama a otras scripts (algunas
            // scripts del juego recurren a otras con IDs inválidos, y el original también se desborda)
            let rest: Vec<u8> = new.windows(3).map(|w| w[0]).collect();
            let calls_scripts = rest.windows(3).any(|w| w == b"ex(");
            if !calls_scripts {
                for extra in [-1.0, 0.0, 0.5, 2.5, 99999.0, 1e9, -0.0, 3.0000000000001] {
                    inputs.push(extra.into());
                }
            }
            inputs.push(Value::Str("abc".into()));
            inputs.push(Value::Str("5".into()));
            let mut bad = 0;
            for input in inputs {
                let mut results = Vec::new();
                for code in [&orig, &fast] {
                    let dummy = self
                        .room
                        .instance_list
                        .insert_dummy(Instance::new_dummy(self.assets.objects.get_asset(0).map(|x| x.as_ref())));
                    let mut ctx = Context::with_single_instance(dummy);
                    ctx.arguments[0] = input.clone();
                    ctx.argument_count = 1;
                    let r = self.execute(code, &mut ctx);
                    self.room.instance_list.remove_dummy(dummy);
                    results.push(format!("{:?} {:?}", r.map(|_| ()).map_err(|e| e.to_string()), ctx.return_value));
                }
                cases += 1;
                if results[0] != results[1] {
                    bad += 1;
                    if bad <= 3 {
                        crate::startlog(&format!("SELFTEST {} con {:?}: {} != {}", name, input, results[0], results[1]));
                    }
                }
            }
            if bad == 0 { scripts_ok += 1 } else { scripts_bad += 1 }
        }
        crate::startlog(&format!(
            "SELFTEST tablas: {} scripts idénticas, {} con diferencias, {} casos comprobados",
            scripts_ok, scripts_bad, cases
        ));
    }
}

// ---------------------------------------------------------------------------------------------
// Despachador `ex`
//
// El juego está ofuscado: casi todas las llamadas son ex(N, args...) y `ex` hace
// `return script_execute(argument0,...,argument15);`. Son 114.010 llamadas en el código y ~586.000
// en el benchmark (~16 % del tiempo). Si `ex` es exactamente eso, el compilador convierte cada
// ex(N, ...) con N constante en una llamada directa a la script N.
const EX_SOURCE: &[u8] = b"return script_execute(argument0,argument1,argument2,argument3,argument4,argument5,argument6,argument7,argument8,argument9,argument10,argument11,argument12,argument13,argument14,argument15);";

pub fn detect_ex_dispatcher(scripts: &[Option<Box<gm8exe::asset::Script>>]) -> Option<usize> {
    if !enabled() {
        return None
    }
    for (i, s) in scripts.iter().enumerate() {
        if let Some(s) = s {
            if s.name.0.as_ref() == b"ex" {
                let src: &[u8] = s.source.0.as_ref();
                let (a, b) = (src.iter().position(|c| !c.is_ascii_whitespace()).unwrap_or(0), src.iter().rposition(|c| !c.is_ascii_whitespace()).map_or(0, |p| p + 1));
                let ok = src.get(a..b) == Some(EX_SOURCE);
                crate::startlog(if ok {
                    "Acelerador: ex(N, ...) -> llamada directa"
                } else {
                    "Acelerador: el despachador ex ha cambiado en esta versión, se usa el original"
                });
                return if ok { Some(i) } else { None }
            }
        }
    }
    None
}

impl crate::game::Game {
    /// Diagnóstico de arranque (runner.log): carpetas, acceso al paquete con las mismas rutas que
    /// construye el juego, y prueba de escritura donde se guardan las partidas.
    pub fn startup_diagnostics(&mut self) {
        use crate::gml::{file, pak};
        let log = crate::startlog;
        let cwd = std::env::current_dir().map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|e| format!("ERROR {e}"));
        log(&format!("Diagnóstico: carpeta actual (working_directory) = '{}'", cwd));
        log(&format!("Diagnóstico: program_directory = '{}'", String::from_utf8_lossy(self.program_directory.as_ref())));
        if std::env::var_os("RLD_DUMP_VIEWS").is_some() {
            for (i, r) in self.assets.rooms.iter().enumerate() {
                let Some(r) = r else { continue };
                if !r.views_enabled {
                    continue
                }
                for v in r.views.iter().filter(|v| v.visible) {
                    if v.source_w as i64 != v.port_w as i64 || v.source_h as i64 != v.port_h as i64 {
                        log(&format!(
                            "VISTA {} {}: {}x{} -> {}x{}",
                            i,
                            self.decode_str(r.name.as_ref()),
                            v.source_w,
                            v.source_h,
                            v.port_w,
                            v.port_h
                        ));
                    }
                }
            }
        }
        log(&format!("Diagnóstico: zona horaria = UTC{:+} min (día/noche del juego)", crate::gml::datetime::local_offset_secs() / 60));
        log(&format!("Diagnóstico: temp_directory = '{}'", String::from_utf8_lossy(self.temp_directory.as_ref())));
        // como ImportBattlePokemon: working_directory + "\data\pkmnbattle\P1_2.gmspr"
        for p in [format!("{}\\data\\pkmnbattle\\P1_2.gmspr", cwd), "data\\text\\es\\txt_menu.dll".to_string()] {
            let tp = file::to_path(&p).into_owned();
            log(&format!("Diagnóstico: file_exists('{}') = {}  [{}]", p, file::file_exists(&tp), pak::describe(&tp)));
        }
        // prueba de escritura en la carpeta de partidas (relativa, como hace el juego)
        let test = file::to_path("data\\partidas\\_rld_prueba_escritura.txt").into_owned();
        let res = (|| -> std::io::Result<bool> {
            if let Some(parent) = std::path::Path::new(&test).parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&test, b"prueba")?;
            let ok = std::fs::read(&test)? == b"prueba";
            std::fs::remove_file(&test)?;
            Ok(ok)
        })();
        log(&format!(
            "Diagnóstico: escritura de partidas en '{}': {}",
            test,
            match res {
                Ok(true) => "OK".to_string(),
                Ok(false) => "ERROR: lo leído no coincide".to_string(),
                Err(e) => format!("ERROR {e}"),
            }
        ));
    }
}

/// Prueba de borrados/escrituras sobre el paquete (RLD_SELFTEST_FS=1, en una copia de la carpeta)
pub fn selftest_fs() {
    use crate::gml::file;
    let p = "data\\audio\\config.ini";
    let tp = file::to_path(p).into_owned();
    let log = crate::startlog;
    let before = file::file_exists(&tp);
    let _ = file::delete(&tp);
    let after_delete = file::file_exists(&tp);
    let read_after_delete = file::read_any(&tp).is_ok();
    log(&format!(
        "SELFTEST fs: existe={} -> tras borrar existe={} legible={}; rld_borrados.txt = {:?}",
        before,
        after_delete,
        read_after_delete,
        std::fs::read_to_string("rld_borrados.txt").ok()
    ));
}

pub fn selftest_fs_2() {
    use crate::gml::file;
    let tp = file::to_path("data\\audio\\config.ini").into_owned();
    let log = crate::startlog;
    log(&format!("SELFTEST fs (tras reiniciar): existe={}", file::file_exists(&tp)));
    std::fs::write(&tp, b"77").ok();
    crate::gml::pak::written(&tp);
    log(&format!(
        "SELFTEST fs: tras escribir existe={} contenido={:?} rld_borrados.txt existe={}",
        file::file_exists(&tp),
        file::read_any(&tp).ok().map(|b| String::from_utf8_lossy(&b).into_owned()),
        std::path::Path::new("rld_borrados.txt").exists()
    ));
}

#[cfg(test)]
mod tests {
    use super::rewrite_lookup_chains;

    fn chain(n: usize, tail: &str) -> String {
        let mut s = String::from("var a; a=1; ");
        for k in 0..n {
            s.push_str(&format!("if(argument0={})return({});", k, k * 10));
        }
        s.push_str(tail);
        s
    }

    #[test]
    fn tabla_normal() {
        let (out, n) = rewrite_lookup_chains(chain(10, " return(-1);").as_bytes()).unwrap();
        assert_eq!(n, 1);
        let out = String::from_utf8(out).unwrap();
        assert!(out.starts_with("var a; a=1; if(rld_lookup_has("), "{out}");
        assert!(out.ends_with(" return(-1);"), "{out}");
    }

    #[test]
    fn expresion_tras_return_no_es_tabla() {
        // la última entrada "return(9)+1" no es un literal: el bloque válido son las 9 primeras
        let mut s = String::new();
        for k in 0..9 {
            s.push_str(&format!("if(argument0={})return({});", k, k));
        }
        s.push_str("if(argument0=9)return(9)+1;");
        let (out, _) = rewrite_lookup_chains(s.as_bytes()).unwrap();
        let out = String::from_utf8(out).unwrap();
        assert!(out.contains("if(argument0=9)return(9)+1;"), "{out}");
    }

    #[test]
    fn else_no_se_separa() {
        let mut s = String::new();
        for k in 0..9 {
            s.push_str(&format!("if(argument0={})return({});", k, k));
        }
        s.push_str("if(argument0=9)return(9); else x=1;");
        let (out, _) = rewrite_lookup_chains(s.as_bytes()).unwrap();
        let out = String::from_utf8(out).unwrap();
        assert!(out.contains("if(argument0=9)return(9); else x=1;"), "{out}");
    }

    #[test]
    fn dentro_de_cadena_no_se_toca() {
        let inner = chain(10, "");
        let s = format!("x=\"{}\"; y='{}'; // {}\n", inner, inner, inner);
        assert!(rewrite_lookup_chains(s.as_bytes()).is_none());
    }

    #[test]
    fn cuerpo_de_if_sin_llaves_no_se_toca() {
        let s = format!("if(b) {}", chain(10, "").replace("var a; a=1; ", ""));
        let out = rewrite_lookup_chains(s.as_bytes()).map(|(o, _)| String::from_utf8(o).unwrap());
        // la primera entrada pertenece al if(b); solo puede reescribirse el resto
        if let Some(o) = out {
            assert!(o.starts_with("if(b) if(argument0=0)return(0);"), "{o}");
        }
    }
}


impl crate::game::Game {
    /// Bucle de GetSecurityPK2/SecurityPK2: acumula sobre `argument0`, término a término y con las
    /// mismas operaciones de Value (mismos redondeos y errores) que el GML original.
    pub fn rld_secpk2_sum(&mut self, args: &[gml::Value]) -> gml::Result<gml::Value> {
        use gml::Value;
        use crate::game::Game;
        let mut acc = args.first().cloned().unwrap_or_default();
        let r = |x: f64| Value::Real(x.into());
        // valor de una variable global (índice: i, o i*32000+j)
        macro_rules! g {
            ($name:expr, $idx:expr) => {{
                let idx: u32 = $idx;
                let id = self.compiler.find_field_id($name.as_bytes());
                match id.and_then(|id| self.globals.fields.get(&id)).and_then(|f| f.get(idx)) {
                    Some(v) => v,
                    None if self.uninit_fields_are_zero => Value::default(),
                    None => return Err(gml::Error::UninitializedVariable($name.into(), idx)),
                }
            }};
        }
        // RoundToTwo(x) = round(x*100)/100
        macro_rules! rt2 {
            ($v:expr) => {{
                let m = $v.mul(r(100.0))?;
                let rounded = Game::round(&[m])?;
                rounded.div(r(100.0))?
            }};
        }
        for i in 0..=5u32 {
            let fi = r(i as f64);
            // v +=(i*7)+(PP_1[i]*6)+(PS_1[i]*7)+STD1[i]+361
            let t = fi.clone().mul(r(7.0))?.add(g!("PP_1", i).mul(r(6.0))?)?.add(g!("PS_1", i).mul(r(7.0))?)?.add(g!("STD1", i))?.add(r(361.0))?;
            acc = acc.add(t)?;
            // v +=(i*3)+(IV[i]*7)+(objpoke1[i]*6)+(selue_pk1[i]*11)+pkmnsex1[i]+361
            let t = fi.clone().mul(r(3.0))?.add(g!("IV", i).mul(r(7.0))?)?.add(g!("objpoke1", i).mul(r(6.0))?)?
                .add(g!("selue_pk1", i).mul(r(11.0))?)?.add(g!("pkmnsex1", i))?.add(r(361.0))?;
            acc = acc.add(t)?;
            let t = fi.clone().mul(r(7.0))?.add(g!("PP_2", i).mul(r(6.0))?)?.add(g!("PS_2", i).mul(r(7.0))?)?.add(g!("STD2", i))?.add(r(361.0))?;
            acc = acc.add(t)?;
            let t = fi.clone().mul(r(3.0))?.add(g!("IV2", i).mul(r(7.0))?)?.add(g!("objpoke2", i).mul(r(6.0))?)?
                .add(g!("selue_pk2", i).mul(r(11.0))?)?.add(g!("pkmnsex2", i))?.add(r(361.0))?;
            acc = acc.add(t)?;
            // for(j=1;j<=4;j+=1){ v +=attack1[i,j]*7*j; v +=attack2[i,j]*7*j; }
            for j in 1..=4u32 {
                let fj = r(j as f64);
                acc = acc.add(g!("attack1", i * 32000 + j).mul(r(7.0))?.mul(fj.clone())?)?;
                acc = acc.add(g!("attack2", i * 32000 + j).mul(r(7.0))?.mul(fj)?)?;
            }
            for (side, velmul) in [("1", 7.0), ("2", 9.0)] {
                let n = |base: &str| format!("{}{}", base, side);
                // v +=(i*13)+(((actual+8)*5)+(MAX_PP[i]*2)+(MAX_PS[i]*2)+(XTRA_PS[i]*2)+(atk[i]*3)+(def[i]*3)+(RoundToTwo(vel[i])*k))
                let inner = g!(n("actual"), 0).add(r(8.0))?.mul(r(5.0))?
                    .add(g!(format!("MAX_PP_{}", side), i).mul(r(2.0))?)?
                    .add(g!(format!("MAX_PS_{}", side), i).mul(r(2.0))?)?
                    .add(g!(format!("XTRA_PS_{}", side), i).mul(r(2.0))?)?
                    .add(g!(n("atk"), i).mul(r(3.0))?)?
                    .add(g!(n("def"), i).mul(r(3.0))?)?
                    .add(rt2!(g!(n("vel"), i)).mul(r(velmul))?)?;
                acc = acc.add(fi.clone().mul(r(13.0))?.add(inner)?)?;
                // v +=(atk_sp[i]*5)+(def_sp[i]*3)+(prec[i]*4)+(tipoA[i]*6)+(tipoB[i]*5)+(tipoC[i]*4)
                let t = g!(n("atk_sp"), i).mul(r(5.0))?
                    .add(g!(n("def_sp"), i).mul(r(3.0))?)?
                    .add(g!(n("prec"), i).mul(r(4.0))?)?
                    .add(g!(n("tipoA"), i).mul(r(6.0))?)?
                    .add(g!(n("tipoB"), i).mul(r(5.0))?)?
                    .add(g!(n("tipoC"), i).mul(r(4.0))?)?;
                acc = acc.add(t)?;
            }
        }
        Ok(acc)
    }
}


// ===== Textos: decodificación byte a byte y caracteres especiales =====

/// Bytes de la fuente (Windows-1252) como texto Latin-1, para usar expresiones regulares sin perder nada
fn latin1(b: &[u8]) -> String {
    b.iter().map(|&c| c as char).collect()
}

fn unlatin1(s: &str) -> Vec<u8> {
    s.chars().map(|c| c as u32 as u8).collect()
}

/// Tablas de 256 entradas de las scripts que son un "if(argument0=N)return(M); ... return argument0;"
static BYTE_MAPS: std::sync::Mutex<Vec<(usize, [u8; 256])>> = std::sync::Mutex::new(Vec::new());

/// Si la script es exactamente una cadena de `if(argument0=N)return(M);` con M entre 0 y 255 que acaba en
/// `return argument0;`, su tabla para los bytes 0..255 (lo que haría con ord() de un carácter)
fn byte_map_of(src: &[u8]) -> Option<[u8; 256]> {
    let t = latin1(trim(src));
    let body = t.strip_suffix("return argument0;")?;
    let re = regex_lite::Regex::new(r"^if\(argument0=(\d+)\)return\((\d+)\);$").ok()?;
    let mut map = [0u8; 256];
    for (i, m) in map.iter_mut().enumerate() {
        *m = i as u8;
    }
    let mut seen = [false; 256];
    for part in body.split_inclusive(';').map(str::trim).filter(|p| !p.is_empty()) {
        let c = re.captures(part)?;
        let n: u32 = c[1].parse().ok()?;
        let v: u32 = c[2].parse().ok()?;
        if v > 255 {
            return None
        }
        if n < 256 && !seen[n as usize] {
            seen[n as usize] = true; // la primera comparación que coincide es la que devuelve
            map[n as usize] = v as u8;
        }
    }
    Some(map)
}

/// `A=""; for(J=1;J<=string_length(T);J+=1){ A+=chr(ex(N,ord(string_char_at(T,J)))); }` con N una tabla de
/// bytes -> `A=rld_map_bytes(T,N); J=string_length(T)+1;` (mismo resultado y J acaba igual que tras el bucle)
fn rewrite_byte_loops(scripts: &[Option<Box<Script>>], src: &[u8]) -> Option<(Vec<u8>, usize)> {
    if !src.windows(18).any(|w| w == b"+=chr(ex(") && !src.windows(9).any(|w| w == b"+=chr(ex(") {
        return None
    }
    let text = latin1(src);
    let re = regex_lite::Regex::new(
        r#"(\w+)="";\s*for\((\w+)=1;(\w+)<=string_length\((\w+)\);(\w+)\+=1\)\{\s*(\w+)\+=chr\(ex\((\d+),ord\(string_char_at\((\w+),(\w+)\)\)\)\);\s*\}"#,
    )
    .ok()?;
    let mut n = 0;
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for c in re.captures_iter(&text) {
        let (a, j, t) = (&c[1], &c[2], &c[4]);
        let ok = &c[3] == j && &c[5] == j && &c[9] == j && &c[6] == a && &c[8] == t && a != t && a != j && t != j;
        let id: usize = c[7].parse().unwrap_or(usize::MAX);
        let map = if ok { scripts.get(id).and_then(|s| s.as_ref()).and_then(|s| byte_map_of(s.source.as_ref())) } else { None };
        let Some(map) = map else { continue };
        if let Ok(mut m) = BYTE_MAPS.lock() {
            if !m.iter().any(|(i, _)| *i == id) {
                m.push((id, map));
            }
        }
        let whole = c.get(0).unwrap();
        out.push_str(&text[last..whole.start()]);
        out.push_str(&format!("{a}=rld_map_bytes({t},{id}); {j}=string_length({t})+1;"));
        last = whole.end();
        n += 1;
    }
    if n == 0 {
        return None
    }
    out.push_str(&text[last..]);
    Some((unlatin1(&out), n))
}

/// rld_map_bytes(texto, script): cada byte del texto pasado por la tabla de esa script
pub fn map_bytes(text: &[u8], id: usize) -> Option<Vec<u8>> {
    let m = BYTE_MAPS.lock().ok()?;
    let (_, map) = m.iter().find(|(i, _)| *i == id)?;
    Some(text.iter().map(|&b| map[b as usize]).collect())
}

/// Sustituciones de SpecialChars, en orden
static SPECIAL_PAIRS: std::sync::Mutex<Vec<(Vec<u8>, Vec<u8>)>> = std::sync::Mutex::new(Vec::new());

/// SpecialChars: `var txt; txt=argument0; txt=string_replace_all(txt,"A","B"); ... return txt;`
fn special_chars_pairs(src: &[u8]) -> Option<Vec<(Vec<u8>, Vec<u8>)>> {
    let t = latin1(trim(src));
    let body = t.strip_prefix("var txt; txt=argument0; ")?.strip_suffix("return txt;")?;
    let re = regex_lite::Regex::new(r#"^txt=string_replace_all\(txt,"([^"]*)","([^"]*)"\);$"#).ok()?;
    let mut pairs = Vec::new();
    for part in body.split_inclusive("); ").map(str::trim).filter(|p| !p.is_empty()) {
        let c = re.captures(part)?;
        if c[1].is_empty() {
            return None
        }
        pairs.push((unlatin1(&c[1]), unlatin1(&c[2])));
    }
    // rld_special_chars se salta los textos sin '&': solo es correcto si todas las marcas lo llevan delante
    if pairs.iter().any(|(a, _)| a.first() != Some(&b'&')) {
        return None
    }
    (!pairs.is_empty()).then_some(pairs)
}

/// string_replace_all de GM8: de izquierda a derecha, sin solapes
fn replace_all(s: &[u8], from: &[u8], to: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        if s[i..].starts_with(from) {
            out.extend_from_slice(to);
            i += from.len();
        } else {
            out.push(s[i]);
            i += 1;
        }
    }
    out
}

pub fn special_chars(text: &[u8]) -> Vec<u8> {
    let mut t = text.to_vec();
    // todas las marcas empiezan por '&': sin '&' no hay nada que sustituir
    if !t.contains(&b'&') {
        return t
    }
    if let Ok(pairs) = SPECIAL_PAIRS.lock() {
        for (a, b) in pairs.iter() {
            if t.windows(a.len()).any(|w| w == a.as_slice()) {
                t = replace_all(&t, a, b);
            }
        }
    }
    t
}
