#![allow(clippy::cognitive_complexity)]
#![allow(clippy::unreadable_literal)]

macro_rules! log {
    ($logger: expr, $x: expr) => {
        if let Some(logger) = &$logger {
            logger($x.into());
        }
    };
    ($logger: expr, $format: expr, $($x: expr),*) => {
        if let Some(logger) = &$logger {
            logger(&format!(
                $format,
                $($x),*
            ));
        }
    };
    ($($x:expr,)*) => (log![$($x),*]); // leveraged from vec![]
}

pub mod asset;

/// Si es true, los píxeles de sprites y fondos se descartan al leer (se conservan medidas y
/// máscaras de colisión). Lo usa el runner en PS Vita, donde guardar ~190 MB de RGBA no cabe.
pub static SKIP_PIXELS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Carga diferida de texturas (PS Vita con gráficos): además de saltar los píxeles (SKIP_PIXELS),
/// se guardan los bloques comprimidos originales de cada sprite y fondo (~22 MB en vez de ~178 MB
/// de RGBA) para descomprimirlos solo cuando se dibujan por primera vez.
pub static LAZY_PIXELS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
pub static LAZY_SPRITE_CHUNKS: std::sync::Mutex<Vec<Box<[u8]>>> = std::sync::Mutex::new(Vec::new());
pub static LAZY_BACKGROUND_CHUNKS: std::sync::Mutex<Vec<Box<[u8]>>> = std::sync::Mutex::new(Vec::new());

thread_local! {
    /// Fuerza la lectura de píxeles en este hilo aunque SKIP_PIXELS esté activo (para decodificar
    /// bajo demanda un bloque guardado)
    static FORCE_PIXELS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

fn decode_chunk<T: asset::Asset>(chunk: &[u8]) -> Option<T> {
    use byteorder::{ReadBytesExt, LE};
    let mut data = flate2::bufread::ZlibDecoder::new(chunk);
    if data.read_u32::<LE>().ok()? == 0 {
        return None
    }
    FORCE_PIXELS.with(|f| f.set(true));
    let r = T::deserialize_exe(data, GameVersion::GameMaker8_0, false).ok();
    FORCE_PIXELS.with(|f| f.set(false));
    r
}

/// Píxeles BGRA de cada fotograma del sprite `index` (bloque guardado con LAZY_PIXELS)
pub fn lazy_sprite_frames(index: usize) -> Option<Vec<Box<[u8]>>> {
    let chunks = LAZY_SPRITE_CHUNKS.lock().ok()?;
    let s: asset::Sprite = decode_chunk(chunks.get(index)?)?;
    Some(s.frames.into_iter().map(|f| f.data).collect())
}

/// Píxeles BGRA del fondo `index` (bloque guardado con LAZY_PIXELS)
pub fn lazy_background_pixels(index: usize) -> Option<Box<[u8]>> {
    let chunks = LAZY_BACKGROUND_CHUNKS.lock().ok()?;
    let b: asset::Background = decode_chunk(chunks.get(index)?)?;
    b.data
}

/// Lee `len` bytes de píxeles, o los salta si SKIP_PIXELS está activo.
pub(crate) fn read_pixels(reader: &mut impl std::io::Read, len: usize) -> std::io::Result<Box<[u8]>> {
    use asset::ReadChunk;
    if SKIP_PIXELS.load(std::sync::atomic::Ordering::Relaxed) && !FORCE_PIXELS.with(|f| f.get()) {
        let mut buf = [0u8; 16 * 1024];
        let mut left = len;
        while left > 0 {
            let n = left.min(buf.len());
            reader.read_exact(&mut buf[..n])?;
            left -= n;
        }
        Ok(Box::new([]))
    } else {
        Ok(reader.read_chunk(len)?.into_boxed_slice())
    }
}
pub mod def;
pub mod gamedata;
pub mod reader;
pub mod rsrc;
pub mod settings;
pub mod upx;

mod colour;

use crate::asset::*;
use settings::{GameHelpDialog, Settings};

pub type AssetList<T> = Vec<Option<Box<T>>>;

pub struct GameAssets {
    pub triggers: AssetList<Trigger>,
    pub constants: Vec<Constant>,
    pub extensions: Vec<Extension>,
    pub sprites: AssetList<Sprite>,
    pub sounds: AssetList<Sound>,
    pub backgrounds: AssetList<Background>,
    pub paths: AssetList<Path>,
    pub scripts: AssetList<Script>,
    pub fonts: AssetList<Font>,
    pub timelines: AssetList<Timeline>,
    pub objects: AssetList<Object>,
    pub rooms: AssetList<Room>,
    pub included_files: Vec<IncludedFile>,
    pub version: GameVersion,

    pub dx_dll: Vec<u8>,
    pub ico_file_raw: Option<Vec<u8>>,
    pub help_dialog: GameHelpDialog,
    pub last_instance_id: i32,
    pub last_tile_id: i32,
    pub library_init_strings: Vec<PascalString>,
    pub room_order: Vec<i32>,

    pub settings: Settings,
    pub game_id: u32,
    pub guid: [u32; 4],
}

#[derive(Copy, Clone, Debug)]
pub enum GameVersion {
    GameMaker8_0,
    GameMaker8_1,
}

pub use colour::Colour;
