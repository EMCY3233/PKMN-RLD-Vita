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
