//! Modo benchmark: mide a qué velocidad ejecuta el runner la lógica del juego.
//!
//! Se activa con `--bench FOTOGRAMAS` (en PS Vita, siempre). Quita el limitador de FPS,
//! pulsa teclas automáticamente para pasar menús y diálogos, y cada segundo de juego
//! escribe en un log los FPS conseguidos, la sala y el número de instancias.

use crate::game::{Game, GetAsset};
use std::{fs::File, io::Write, time::Instant};

/// Teclas virtuales de Windows que entiende el juego
const VK_RETURN: u8 = 0x0D;
const VK_Z: u8 = b'Z';
const VK_DOWN: u8 = 0x28;
const VK_X: u8 = b'X';

pub struct Bench {
    pub max_frames: u64,
    frame: u64,
    start: Instant,
    window_start: Instant,
    window_frames: u64,
    worst_frame_ms: f64,
    last_frame: Instant,
    log: Option<File>,
    rooms_seen: Vec<i32>,
    polls: u64,
}

impl Bench {
    pub fn new(max_frames: u64, log_path: &str) -> Self {
        let now = Instant::now();
        let log = File::create(log_path).ok();
        let mut b = Self {
            max_frames,
            frame: 0,
            start: now,
            window_start: now,
            window_frames: 0,
            worst_frame_ms: 0.0,
            last_frame: now,
            log,
            rooms_seen: Vec::new(),
            polls: 0,
        };
        b.line(&format!("Benchmark del runner - plataforma: {} - {} fotogramas", std::env::consts::OS, max_frames));
        b.line("seg_real | fotogramas | FPS | peor_fotograma_ms | sala | instancias");
        b
    }

    pub fn line(&mut self, s: &str) {
        println!("{s}");
        if let Some(f) = self.log.as_mut() {
            let _ = writeln!(f, "{s}");
            let _ = f.flush();
        }
    }
}

impl Game {
    /// Pulsaciones simuladas (pasar menús, nombre, diálogos). Se llama en cada sondeo de entrada:
    /// una vez por fotograma y también dentro de los bucles modales del juego (io_handle, sleep,
    /// keyboard_wait), que de otro modo esperarían eternamente una tecla dentro del mismo fotograma.
    /// Las teclas simuladas no escriben texto, así que los campos de nombre se aceptan por defecto.
    pub fn bench_poll(&mut self) {
        let Some(b) = self.bench.as_mut() else { return };
        b.polls += 1;
        let frame = b.polls;
        let press = |g: &mut Game, vk: u8| g.pending_keys.push((vk, true));
        let release = |g: &mut Game, vk: u8| g.pending_keys.push((vk, false));
        let speed = self.room.speed.max(1) as u64;
        // Una pulsación de Z cada 1,5 s de juego; Enter cada 7 s; abajo de vez en cuando para moverse
        let z_period = speed * 3 / 2;
        let enter_period = speed * 7;
        match frame % z_period { 0 => press(self, VK_Z), 2 => release(self, VK_Z), _ => () }
        match frame % enter_period { 5 => press(self, VK_RETURN), 7 => release(self, VK_RETURN), _ => () }
        // X (cancelar) cada 5 s, desfasado, para salir de menús como la mochila
        // (nunca en el título ni en la selección de personaje: ahí X sale del juego)
        let in_menus = matches!(self.room.id, 22 | 30 | 3);
        if !in_menus {
            match frame % (speed * 5) { 20 => press(self, VK_X), 22 => release(self, VK_X), _ => () }
        }
        if frame > speed * 120 {
            // pasados 2 minutos de juego, caminar un poco para medir el mapa
            match frame % (speed * 4) { 10 => press(self, VK_DOWN), 30 => release(self, VK_DOWN), _ => () }
        }
    }

    /// Se llama una vez por fotograma desde `run()`. Devuelve true cuando hay que terminar.
    pub fn bench_tick(&mut self) -> bool {
        let Some(mut b) = self.bench.take() else { return false };
        let now = Instant::now();
        let ms = now.duration_since(b.last_frame).as_secs_f64() * 1000.0;
        b.last_frame = now;
        if b.frame > 0 && ms > b.worst_frame_ms {
            b.worst_frame_ms = ms;
        }
        b.frame += 1;
        b.window_frames += 1;
        if !b.rooms_seen.contains(&self.room.id) {
            b.rooms_seen.push(self.room.id);
            let name = self.assets.rooms.get_asset(self.room.id).map(|r| self.decode_str(r.name.as_ref()).into_owned()).unwrap_or_default();
            b.line(&format!("-> entra en sala {} '{}' (fotograma {}, velocidad de sala {})", self.room.id, name, b.frame, self.room.speed));
        }
        let speed = self.room.speed.max(1) as u64;
        if b.window_frames >= speed {
            let secs = now.duration_since(b.window_start).as_secs_f64();
            let fps = b.window_frames as f64 / secs.max(1e-9);
            let line = format!(
                "{:8.1} | {:10} | {:5.1} | {:8.1} | {} | {}",
                now.duration_since(b.start).as_secs_f64(),
                b.frame,
                fps,
                b.worst_frame_ms,
                self.room.id,
                self.room.instance_list.count_all()
            );
            b.line(&line);
            b.window_start = now;
            b.window_frames = 0;
            b.worst_frame_ms = 0.0;
        }
        let frame = b.frame;
        let done = frame >= b.max_frames;
        if done {
            let total = now.duration_since(b.start).as_secs_f64();
            let line = format!("FIN: {} fotogramas en {:.1} s = {:.1} FPS de media", frame, total, frame as f64 / total.max(1e-9));
            b.line(&line);
        }
        self.bench = Some(b);
        done
    }
}
