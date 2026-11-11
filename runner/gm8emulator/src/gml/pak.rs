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
    /// Archivos del paquete que el juego ha borrado (persistente en rld_borrados.txt): para el juego
    /// dejan de existir, igual que si hubiera borrado el archivo real en un PC.
    deleted: Mutex<HashSet<String>>,
    deleted_file: std::path::PathBuf,
    file: Mutex<File>,
    index: HashMap<String, (u64, u32)>,
    dirs: HashSet<String>,
    /// carpeta del juego normalizada (minúsculas, '/', con '/' final)
    base: String,
    /// Archivos del paquete que además existen sueltos en disco (partidas, configuración o lo que el
    /// juego haya escrito): esos se leen del disco. Para todo lo demás se va directo al paquete sin
    /// intentar abrir el archivo suelto (en la Vita cada intento fallido en la tarjeta cuesta tiempo).
    on_disk: Mutex<HashSet<String>>,
}

/// Recorre la carpeta del juego y apunta los archivos sueltos que también están en el paquete
fn scan_disk(game_dir: &Path, index: &HashMap<String, (u64, u32)>) -> HashSet<String> {
    let mut out = HashSet::new();
    let mut stack = vec![game_dir.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            let path = e.path();
            let Ok(ft) = e.file_type() else { continue };
            if ft.is_dir() {
                let name = e.file_name().to_string_lossy().to_ascii_lowercase();
                if name != "temp" && !name.starts_with("gm_ttt_") {
                    stack.push(path);
                }
            } else if let Ok(rel) = path.strip_prefix(game_dir) {
                let k = norm(&rel.to_string_lossy());
                if index.contains_key(&k) {
                    out.insert(k);
                }
            }
        }
    }
    out
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
    // Vita: "ux0:/data/..." y "ux0:data/..." son la misma ruta
    if let Some(p) = s.find(":/") {
        if p > 0 && s[..p].bytes().all(|c| c.is_ascii_alphanumeric()) {
            s.remove(p + 1);
        }
    }
    // "./" intermedios
    while s.contains("/./") {
        s = s.replace("/./", "/");
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
    let deleted_file = game_dir.join("rld_borrados.txt");
    let deleted = std::fs::read_to_string(&deleted_file)
        .map(|s| s.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect())
        .unwrap_or_default();
    let on_disk = Mutex::new(scan_disk(game_dir, &index));
    Ok(Pak { deleted: Mutex::new(deleted), deleted_file, file: Mutex::new(f), index, dirs, base, on_disk })
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

fn is_deleted(p: &Pak, k: &str) -> bool {
    p.deleted.lock().map(|d| d.contains(k)).unwrap_or(false)
}

fn save_deleted(p: &Pak, d: &HashSet<String>) {
    let mut v: Vec<&String> = d.iter().collect();
    v.sort();
    let text: String = v.iter().map(|k| format!("{k}\n")).collect();
    if text.is_empty() {
        let _ = std::fs::remove_file(&p.deleted_file);
    } else {
        let _ = std::fs::write(&p.deleted_file, text);
    }
}

pub fn exists(path: &str) -> bool {
    get().and_then(|p| key(p, path).map(|k| p.index.contains_key(&k) && !is_deleted(p, &k))).unwrap_or(false)
}

/// El juego ha borrado `path`: si estaba en el paquete, deja de existir para él.
pub fn mark_deleted(path: &str) {
    invalidate(path);
    if let Some(p) = get() {
        if let Some(k) = key(p, path) {
            if p.index.contains_key(&k) {
                if let Ok(mut d) = p.deleted.lock() {
                    if d.insert(k) {
                        save_deleted(p, &d);
                    }
                }
            }
        }
    }
}

/// El juego va a escribir `path` en disco: olvidar cachés y un posible borrado anterior.
pub fn written(path: &str) {
    invalidate(path);
    if let Some(p) = get() {
        if let Some(k) = key(p, path) {
            if p.index.contains_key(&k) {
                if let Ok(mut d) = p.on_disk.lock() {
                    d.insert(k.clone());
                }
            }
            if let Ok(mut d) = p.deleted.lock() {
                if d.remove(&k) {
                    save_deleted(p, &d);
                }
            }
        }
    }
}

pub fn dir_exists(path: &str) -> bool {
    get().and_then(|p| key(p, path).map(|k| p.dirs.contains(&k))).unwrap_or(false)
}

pub fn read(path: &str) -> Option<Vec<u8>> {
    let pak = get()?;
    let k = key(pak, path)?;
    if is_deleted(pak, &k) {
        return None
    }
    let &(off, size) = pak.index.get(&k)?;
    let mut f = pak.file.lock().ok()?;
    f.seek(SeekFrom::Start(off)).ok()?;
    let mut buf = vec![0u8; size as usize];
    f.read_exact(&mut buf).ok()?;
    Some(buf)
}

/// Diagnóstico: ¿hay paquete? y clave a la que se traduciría una ruta
pub fn describe(path: &str) -> String {
    match get() {
        None => "sin reloaded.pak".into(),
        Some(p) => format!(
            "base del paquete '{}', ruta '{}' -> clave {:?}, en paquete: {}",
            p.base,
            path,
            key(p, path),
            exists(path)
        ),
    }
}

/// ¿Está `path` en el paquete y no suelto en disco? (entonces se lee del paquete directamente)
pub fn packed_only(path: &str) -> bool {
    get()
        .and_then(|p| {
            key(p, path).map(|k| {
                p.index.contains_key(&k) && !is_deleted(p, &k) && !p.on_disk.lock().map(|d| d.contains(&k)).unwrap_or(true)
            })
        })
        .unwrap_or(false)
}

/// Lee un archivo del disco o, si no existe, del paquete.
pub fn read_any(path: &str) -> std::io::Result<Vec<u8>> {
    // archivo del paquete que no está suelto en disco: directo al paquete
    if let Some(p) = get() {
        if let Some(k) = key(p, path) {
            if p.index.contains_key(&k) && !p.on_disk.lock().map(|d| d.contains(&k)).unwrap_or(true) {
                if let Some(b) = read(path) {
                    return Ok(b)
                }
            }
        }
    }
    match std::fs::read(path) {
        Ok(b) => Ok(b),
        Err(e) => read(path).ok_or(e),
    }
}

#[cfg(test)]
mod tests {
    use super::norm;

    #[test]
    fn rutas_de_vita() {
        assert_eq!(norm("ux0:/data/reloaded\\data\\x.png"), "ux0:data/reloaded/data/x.png");
        assert_eq!(norm("ux0:data/reloaded/data/X.PNG"), "ux0:data/reloaded/data/x.png");
        assert_eq!(norm("ux0:data/reloaded//./data/x.png"), "ux0:data/reloaded/data/x.png");
        assert_eq!(norm("data\\text\\es\\a.dll"), "data/text/es/a.dll");
        assert_eq!(norm("/root/juego/data/a"), "/root/juego/data/a");
    }
}
