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
            self.lit(b";");
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
        // inicio de sentencia: principio, o tras ';', '{' o '}' (nunca tras un if/else sin llaves)
        let at_stmt = {
            let mut j = i;
            while j > 0 && src[j - 1].is_ascii_whitespace() {
                j -= 1;
            }
            j == 0 || matches!(src[j - 1], b';' | b'{' | b'}')
        };
        if at_stmt && src[i..].starts_with(b"if") {
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
