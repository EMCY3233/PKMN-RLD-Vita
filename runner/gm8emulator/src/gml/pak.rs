//! Lector de `reloaded.pak` (ver tools/rldpak.py): un único archivo con toda la carpeta data/.
//!
//! Prioridad: si un archivo existe en disco (partidas, configuración, capturas…) se usa ese;
//! si no, se busca en el paquete. Así el juego no nota la diferencia y las partidas guardadas
//! nunca se mezclan con los datos del juego.

use std::{
    collections::{HashMap, HashSet},
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::Path,
    sync::{Mutex, OnceLock},
};

const MAGIC: &[u8; 8] = b"RLDPAK1\0";

pub struct Pak {
    file: Mutex<File>,
    index: HashMap<String, (u64, u32)>,
    dirs: HashSet<String>,
    /// carpeta del juego normalizada (minúsculas, '/', con '/' final)
    base: String,
}

static PAK: OnceLock<Option<Pak>> = OnceLock::new();

/// Caché en memoria de archivos de solo lectura que el juego relee constantemente
/// (data/text/*: TextReader abre txt_dialog.dll, de 1,5 MB, para sacar cada línea de diálogo).
/// En PS Vita cada relectura desde la tarjeta costaba ~100 ms.
static CACHE: Mutex<Option<HashMap<String, std::sync::Arc<[u8]>>>> = Mutex::new(None);
const CACHE_MAX_FILE: usize = 4 * 1024 * 1024;

fn cacheable(k: &str) -> bool {
    k.starts_with("data/text/") && !k.ends_with(".ini")
}

/// Clave normalizada de una ruta del juego (aunque no haya paquete)
fn any_key(path: &str) -> String {
    match get() {
        Some(p) => key(p, path).unwrap_or_else(|| norm(path)),
        None => norm(path),
    }
}

/// Lectura con caché: disco primero, paquete después; guarda en memoria los textos del juego.
pub fn read_cached(path: &str) -> Option<std::sync::Arc<[u8]>> {
    let k = any_key(path);
    let use_cache = cacheable(&k);
    if use_cache {
        if let Some(c) = CACHE.lock().ok().and_then(|c| c.as_ref().and_then(|m| m.get(&k).cloned())) {
            return Some(c)
        }
    }
    let data: std::sync::Arc<[u8]> = read_any(path).ok()?.into();
    if use_cache && data.len() <= CACHE_MAX_FILE {
        if let Ok(mut c) = CACHE.lock() {
            c.get_or_insert_with(HashMap::new).insert(k, data.clone());
        }
    }
    Some(data)
}

/// Índices de inicio de línea de los textos en caché (para rld_text_line)
static LINES: Mutex<Option<HashMap<String, std::sync::Arc<Vec<u32>>>>> = Mutex::new(None);

/// Línea `line` (1 = primera) de un archivo de texto, igual que haría el GML:
/// saltar line-1 veces con file_text_readln y leer con file_text_read_string (sin \r\n).
/// Devuelve None si el archivo no existe; Some(vacío) si la línea no existe.
pub fn text_line(path: &str, line: i64) -> Option<Vec<u8>> {
    let data = read_cached(path)?;
    let k = any_key(path);
    let starts = {
        let mut g = LINES.lock().ok()?;
        let m = g.get_or_insert_with(HashMap::new);
        m.entry(k)
            .or_insert_with(|| {
                let mut v = vec![0u32];
                for (i, &b) in data.iter().enumerate() {
                    if b == 0x0a {
                        v.push(i as u32 + 1);
                    }
                }
                std::sync::Arc::new(v)
            })
            .clone()
    };
    let idx = (line.max(1) - 1) as usize;
    let Some(&start) = starts.get(idx) else { return Some(Vec::new()) };
    let start = start as usize;
    // el bucle original devuelve "???" si llega al final antes de la línea pedida
    if starts[..idx].iter().any(|&s| s as usize >= data.len()) || start >= data.len() {
        return Some(Vec::new())
    }
    let end = data[start..].iter().position(|&b| b == 0x0a).map_or(data.len(), |p| start + p);
    let mut out = data[start..end].to_vec();
    if out.last() == Some(&0x0d) {
        out.pop();
    }
    Some(out)
}

/// Olvida un archivo de la caché (cuando el juego lo escribe o lo borra)
pub fn invalidate(path: &str) {
    let k = any_key(path);
    if let Ok(mut c) = CACHE.lock() {
        if let Some(m) = c.as_mut() {
            m.remove(&k);
        }
    }
    if let Ok(mut c) = LINES.lock() {
        if let Some(m) = c.as_mut() {
            m.remove(&k);
        }
    }
}

fn norm(path: &str) -> String {
    let mut s = path.replace('\\', "/").to_lowercase();
    while s.contains("//") {
        s = s.replace("//", "/");
    }
    s
}

/// Abre `<carpeta_del_juego>/reloaded.pak` si existe. Llamar una vez al arrancar.
pub fn init(game_dir: &Path) {
    let _ = PAK.set(open(game_dir).ok());
}

fn open(game_dir: &Path) -> std::io::Result<Pak> {
    let mut f = File::open(game_dir.join("reloaded.pak"))?;
    let mut hdr = [0u8; 20];
    f.read_exact(&mut hdr)?;
    if &hdr[..8] != MAGIC {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "reloaded.pak no válido"));
    }
    let count = u32::from_le_bytes(hdr[8..12].try_into().unwrap()) as usize;
    let ipos = u64::from_le_bytes(hdr[12..20].try_into().unwrap());
    f.seek(SeekFrom::Start(ipos))?;
    let mut raw = Vec::new();
    f.read_to_end(&mut raw)?;
    let mut index = HashMap::with_capacity(count);
    let mut dirs = HashSet::new();
    let mut p = 0usize;
    for _ in 0..count {
        let n = u16::from_le_bytes([raw[p], raw[p + 1]]) as usize;
        p += 2;
        let key = String::from_utf8_lossy(&raw[p..p + n]).into_owned();
        p += n;
        let off = u64::from_le_bytes(raw[p..p + 8].try_into().unwrap());
        let size = u32::from_le_bytes(raw[p + 8..p + 12].try_into().unwrap());
        p += 12;
        let mut d = key.as_str();
        while let Some(i) = d.rfind('/') {
            d = &d[..i];
            dirs.insert(d.to_string());
        }
        index.insert(key, (off, size));
    }
    let mut base = norm(&game_dir.to_string_lossy());
    if !base.ends_with('/') {
        base.push('/');
    }
    Ok(Pak { file: Mutex::new(f), index, dirs, base })
}

fn get() -> Option<&'static Pak> {
    PAK.get().and_then(|p| p.as_ref())
}

/// Ruta del juego → clave del paquete ("data/…"). None si la ruta está fuera de la carpeta del juego.
fn key(pak: &Pak, path: &str) -> Option<String> {
    let mut s = norm(path);
    if let Some(rest) = s.strip_prefix(&pak.base) {
        s = rest.to_string();
    } else if s.starts_with('/') || s.contains(':') {
        return None
    }
    while let Some(rest) = s.strip_prefix("./") {
        s = rest.to_string();
    }
    Some(s.trim_end_matches('/').to_string())
}

pub fn exists(path: &str) -> bool {
    get().and_then(|p| key(p, path).map(|k| p.index.contains_key(&k))).unwrap_or(false)
}

pub fn dir_exists(path: &str) -> bool {
    get().and_then(|p| key(p, path).map(|k| p.dirs.contains(&k))).unwrap_or(false)
}

pub fn read(path: &str) -> Option<Vec<u8>> {
    let pak = get()?;
    let &(off, size) = pak.index.get(&key(pak, path)?)?;
    let mut f = pak.file.lock().ok()?;
    f.seek(SeekFrom::Start(off)).ok()?;
    let mut buf = vec![0u8; size as usize];
    f.read_exact(&mut buf).ok()?;
    Some(buf)
}

/// Lee un archivo del disco o, si no existe, del paquete.
pub fn read_any(path: &str) -> std::io::Result<Vec<u8>> {
    match std::fs::read(path) {
        Ok(b) => Ok(b),
        Err(e) => read(path).ok_or(e),
    }
}
