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
    /// Perfilado por script: pila de (script, inicio, tiempo en hijas) y tiempo propio acumulado
    prof_stack: Vec<(ProfKey, Instant, f64)>,
    prof_self: crate::util::FastHashMap<ProfKey, (u32, f64)>,
    /// Acumulado de todo el recorrido (RLD_PROFILE=1)
    prof_total: crate::util::FastHashMap<ProfKey, (u64, f64)>,
    pub prof_all: bool,
    /// RLD_GOTO=<sala>: tras llegar a la casa (sala 10) saltar a esa sala (pruebas de minijuegos)
    goto: Option<i32>,
    home_frames: u64,
    /// RLD_GOTO_LIST: recorrido por salas (ver bench_tour)
    tour: Option<Tour>,
}

/// Recorrido automático por salas: tras cargar la partida (llegar a RLD_GOTO_FROM), salta a cada sala
/// de RLD_GOTO_LIST, se queda RLD_GOTO_DWELL fotogramas (240) y apunta en tour.tsv instancias, tiempos,
/// errores y una captura tour_<sala>.png
pub struct Tour {
    list: Vec<i32>,
    idx: usize,
    dwell: u64,
    started: bool,
    frames: u64,
    sum_ms: f64,
    worst_ms: f64,
    draw_ns0: u64,
    errors0: u32,
    max_inst: usize,
    last: Instant,
    log: Option<File>,
    target: i32,
}

/// Qué se está midiendo: una script (por id) o una función interna del motor (por dirección)
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProfKey {
    Script(usize),
    Func(usize),
    /// evento de objeto: (objeto, tipo de evento, subevento)
    Event(i32, u16, u32),
}

/// Vigilante de cuelgues (RLD_HANG_TRACE): última script y último evento empezados, y fotograma actual
pub static HANG_SCRIPT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(u64::MAX);
pub static HANG_EVENT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(u64::MAX);
pub static HANG_FRAME: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn start_hang_watchdog() {
    use std::sync::atomic::Ordering::Relaxed;
    let _ = std::thread::Builder::new().name("vigilante".into()).spawn(|| {
        let mut last = (HANG_FRAME.load(Relaxed), Instant::now());
        let mut reported = false;
        loop {
            std::thread::sleep(std::time::Duration::from_millis(500));
            let f = HANG_FRAME.load(Relaxed);
            if f != last.0 {
                last = (f, Instant::now());
                reported = false;
            } else if !reported && last.1.elapsed().as_secs() >= 10 {
                reported = true;
                let ev = HANG_EVENT.load(Relaxed);
                eprintln!(
                    "CUELGUE: fotograma {} sin terminar; último evento objeto {} tipo {} sub {}; última script {}",
                    f, ev >> 32, (ev >> 16) & 0xffff, ev & 0xffff, HANG_SCRIPT.load(Relaxed)
                );
                let _ = std::fs::write(
                    "tour_hang.txt",
                    format!("obj {} ev {} sub {} script {}", ev >> 32, (ev >> 16) & 0xffff, ev & 0xffff, HANG_SCRIPT.load(Relaxed)),
                );
            }
        }
    });
}

impl Bench {
    pub fn new(max_frames: u64, log_path: &str) -> Self {
        if std::env::var_os("RLD_HANG_TRACE").is_some() {
            start_hang_watchdog();
        }
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
            prof_stack: Vec::new(),
            prof_self: Default::default(),
            prof_total: Default::default(),
            prof_all: std::env::var_os("RLD_PROFILE").is_some(),
            goto: std::env::var("RLD_GOTO").ok().and_then(|v| v.parse().ok()),
            home_frames: 0,
            tour: std::env::var("RLD_GOTO_LIST").ok().map(|l| Tour {
                list: l.split(',').filter_map(|x| x.trim().parse().ok()).collect(),
                idx: 0,
                dwell: std::env::var("RLD_GOTO_DWELL").ok().and_then(|v| v.parse().ok()).unwrap_or(240),
                started: false,
                frames: 0,
                sum_ms: 0.0,
                worst_ms: 0.0,
                draw_ns0: 0,
                errors0: 0,
                max_inst: 0,
                last: now,
                log: std::fs::OpenOptions::new().create(true).append(true).open("tour.tsv").ok(),
                target: -1,
            }),
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
    fn prof_name(&self, k: ProfKey) -> String {
        match k {
            ProfKey::Script(id) => self.assets.scripts.get(id).and_then(|s| s.as_ref()).map(|s| String::from_utf8_lossy(s.name.as_ref()).into_owned()).unwrap_or_default(),
            ProfKey::Func(addr) => crate::gml::mappings::FUNCTIONS
                .entries()
                .find(|(_, f)| f.addr() as usize == addr)
                .map(|(n, _)| format!("{}()", n))
                .unwrap_or_else(|| format!("fn@{:x}", addr)),
            ProfKey::Event(obj, ev, sub) => {
                let name = self.assets.objects.get_asset(obj).map(|o| String::from_utf8_lossy(o.name.as_ref()).into_owned()).unwrap_or_default();
                let evn = match ev { 0 => "create", 1 => "destroy", 2 => "alarm", 3 => "step", 4 => "collision", 5 => "keyboard", 6 => "mouse", 7 => "other", 8 => "draw", 9 => "keypress", 10 => "keyrelease", _ => "?" };
                format!("{} [{} {}]", name, evn, sub)
            },
        }
    }

    /// Entrada en un evento de objeto (solo con RLD_PROFILE=1)
    #[inline]
    pub fn bench_enter_event(&mut self, obj: i32, ev: usize, sub: u32) -> bool {
        match self.bench.as_mut() {
            Some(b) if b.prof_all => {
                b.prof_stack.push((ProfKey::Event(obj, ev as u16, sub), Instant::now(), 0.0));
                true
            },
            _ => false,
        }
    }

    /// Entrada en una script (solo cuenta en modo benchmark)
    pub fn bench_enter(&mut self, id: usize) {
        if let Some(b) = self.bench.as_mut() {
            HANG_SCRIPT.store(id as u64, std::sync::atomic::Ordering::Relaxed);
            b.prof_stack.push((ProfKey::Script(id), Instant::now(), 0.0));
        }
    }

    /// Entrada en una función interna (solo con RLD_PROFILE=1, porque medir cada llamada cuesta)
    #[inline]
    pub fn bench_enter_fn(&mut self, addr: usize) -> bool {
        match self.bench.as_mut() {
            Some(b) if b.prof_all => {
                b.prof_stack.push((ProfKey::Func(addr), Instant::now(), 0.0));
                true
            },
            _ => false,
        }
    }

    /// Salida de la script más reciente: acumula su tiempo propio (sin contar las que llamó)
    pub fn bench_exit(&mut self) {
        if let Some(b) = self.bench.as_mut() {
            if let Some((id, start, child)) = b.prof_stack.pop() {
                let total = start.elapsed().as_secs_f64();
                let selft = (total - child).max(0.0);
                let e = b.prof_self.entry(id).or_insert((0, 0.0));
                e.0 += 1;
                e.1 += selft;
                if b.prof_all {
                    let t = b.prof_total.entry(id).or_insert((0, 0.0));
                    t.0 += 1;
                    t.1 += selft;
                }
                if let Some(parent) = b.prof_stack.last_mut() {
                    parent.2 += total;
                }
            }
        }
    }

    /// Pulsaciones simuladas (pasar menús, nombre, diálogos). Se llama en cada sondeo de entrada:
    /// una vez por fotograma y también dentro de los bucles modales del juego (io_handle, sleep,
    /// keyboard_wait), que de otro modo esperarían eternamente una tecla dentro del mismo fotograma.
    /// Las teclas simuladas no escriben texto, así que los campos de nombre se aceptan por defecto.
    pub fn bench_poll(&mut self) {
        let Some(b) = self.bench.as_mut() else { return };
        // RLD_GOTO_IDLE=1: una vez hecho el salto de RLD_GOTO, no pulsar nada (medir la sala quieta)
        if b.goto.is_none() && b.home_frames > 0 && std::env::var_os("RLD_GOTO_IDLE").is_some() {
            return
        }
        b.polls += 1;
        let frame = b.polls;
        // recorrido por salas: pulsar una tecla de vez en cuando para que avancen los diálogos de las escenas
        // (RLD_TOUR_KEY: código de tecla; por defecto X = cancelar, que no acepta opciones)
        if b.tour.as_ref().map_or(false, |t| t.started) {
            let k = std::env::var("RLD_TOUR_KEY").ok().and_then(|v| v.parse().ok()).unwrap_or(0x58u8);
            match frame % 20 {
                0 => self.pending_keys.push((k, true)),
                2 => self.pending_keys.push((k, false)),
                _ => (),
            }
        }
        // RLD_KEYS=sondeo:vk:1|0,… y RLD_SHOTS_P=sondeo,…: pulsaciones y capturas en sondeos de
        // teclado concretos (también dentro de bucles modales); RLD_MANUAL=1: sin pulsaciones
        // automáticas. Para reproducir casos concretos en PC.
        if let Ok(list) = std::env::var("RLD_KEYS") {
            for item in list.split(',') {
                let p: Vec<&str> = item.split(':').collect();
                if let [f, vk, d] = p[..] {
                    if f.trim().parse::<u64>().ok() == Some(frame) {
                        if let Ok(vk) = vk.trim().parse::<u8>() {
                            self.pending_keys.push((vk, d.trim() == "1"));
                        }
                    }
                }
            }
        }
        if let Ok(list) = std::env::var("RLD_SHOTS_P") {
            if list.split(',').any(|f| f.trim().parse::<u64>().ok() == Some(frame)) {
                let (w, h) = (self.unscaled_width, self.unscaled_height);
                let rgba = self.renderer.get_pixels(0, 0, w as _, h as _);
                if let Some(img) = image::RgbaImage::from_vec(w, h, rgba.into()) {
                    let _ = img.save(format!("shotp_{}.png", frame));
                }
            }
        }
        if std::env::var_os("RLD_MANUAL").is_some() {
            return
        }
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
        HANG_FRAME.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        // RLD_PRECOMPILE=1: en el fotograma 5, compilar todas las scripts y medir tiempo y memoria
        if b.frame == 5 && std::env::var_os("RLD_PRECOMPILE").is_some() {
            let rss = || std::fs::read_to_string("/proc/self/statm").ok().and_then(|s| s.split_whitespace().nth(1).and_then(|v| v.parse::<u64>().ok())).unwrap_or(0) * 4096 / 1048576;
            let (m0, t0) = (rss(), Instant::now());
            let (mut n, mut bytes) = (0, 0usize);
            for id in 0..self.assets.scripts.len() {
                if let Some(Some(sc)) = self.assets.scripts.get(id) {
                    if sc.compiled.get().is_none() {
                        bytes += sc.source.as_ref().len();
                        if self.script_code(id).is_ok() {
                            n += 1;
                        }
                    }
                }
            }
            b.line(&format!("precompilado: {} scripts, {} KB de código, {:.0} ms, memoria {} -> {} MB", n, bytes / 1024, t0.elapsed().as_secs_f64() * 1e3, m0, rss()));
        }
        // RLD_SHOTS=1600,2000: guardar capturas shot_<fotograma>.png (comparar renderizadores)
        if let Ok(list) = std::env::var("RLD_SHOTS") {
            if list.split(',').any(|f| f.trim().parse::<u64>().ok() == Some(b.frame)) {
                self.renderer.flush_queue();
                let (w, h) = (self.unscaled_width, self.unscaled_height);
                let rgba = self.renderer.get_pixels(0, 0, w as _, h as _);
                if let Some(img) = image::RgbaImage::from_vec(w, h, rgba.into()) {
                    let _ = img.save(format!("shot_{}.png", b.frame));
                }
            }
        }
        let now = Instant::now();
        let ms = now.duration_since(b.last_frame).as_secs_f64() * 1000.0;
        b.last_frame = now;
        if b.frame > 0 && ms > b.worst_frame_ms {
            b.worst_frame_ms = ms;
        }
        // Fotograma lento (>1 s): apuntar las scripts que más tiempo propio consumieron
        static SLOW_MS: std::sync::OnceLock<f64> = std::sync::OnceLock::new();
        let slow = *SLOW_MS.get_or_init(|| std::env::var("RLD_SLOW_MS").ok().and_then(|v| v.parse().ok()).unwrap_or(1000.0));
        if b.frame > 0 && ms > slow {
            let mut v: Vec<_> = b.prof_self.iter().map(|(k, v)| (*k, *v)).collect();
            v.sort_by(|a, b| b.1 .1.partial_cmp(&a.1 .1).unwrap_or(std::cmp::Ordering::Equal));
            let total: f64 = v.iter().map(|x| x.1 .1).sum();
            let mut line = format!("   lento: fotograma {} de {:.0} ms (scripts {:.0} ms) ->", b.frame + 1, ms, total * 1000.0);
            for (id, (calls, secs)) in v.iter().take(5) {
                line.push_str(&format!(" {}({}x) {:.0}ms;", self.prof_name(*id), calls, secs * 1000.0));
            }
            b.line(&line);
        }
        b.prof_self.clear();
        b.frame += 1;
        b.window_frames += 1;
        if !b.rooms_seen.contains(&self.room.id) {
            b.rooms_seen.push(self.room.id);
            let name = self.assets.rooms.get_asset(self.room.id).map(|r| self.decode_str(r.name.as_ref()).into_owned()).unwrap_or_default();
            b.line(&format!("-> entra en sala {} '{}' (fotograma {}, velocidad de sala {})", self.room.id, name, b.frame, self.room.speed));
            if self.room.views_enabled {
                let vs: Vec<String> = self
                    .room
                    .views
                    .iter()
                    .filter(|v| v.visible)
                    .map(|v| format!("vista {}x{} -> puerto {}x{}", v.source_w, v.source_h, v.port_w, v.port_h))
                    .collect();
                b.line(&format!("   {}", vs.join(", ")));
            }
        }
        if let Some(mut t) = b.tour.take() {
            let keep = self.bench_tour(&mut t);
            b.tour = Some(t);
            if !keep {
                b.max_frames = b.frame + 1;
            }
        }
        if let Some(target) = b.goto {
            // RLD_GOTO_FROM=<sala>: desde qué sala saltar (por defecto la casa, 10)
            let from = std::env::var("RLD_GOTO_FROM").ok().and_then(|v| v.parse().ok()).unwrap_or(10);
            if self.room.id == from {
                b.home_frames += 1;
                if b.home_frames == 90 {
                    b.line(&format!("-> RLD_GOTO: saltando a la sala {}", target));
                    self.scene_change = Some(crate::game::SceneChange::Room(target));
                    b.goto = None;
                    for vk in [VK_Z, VK_X, VK_RETURN, VK_DOWN] {
                        self.pending_keys.push((vk, false));
                    }
                }
            }
        }
        let speed = self.room.speed.max(1) as u64;
        if b.window_frames >= speed || now.duration_since(b.window_start).as_secs() >= 10 {
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
            if b.prof_all {
                let mut v: Vec<_> = b.prof_total.iter().map(|(k, v)| (*k, *v)).collect();
                v.sort_by(|a, b| b.1 .1.partial_cmp(&a.1 .1).unwrap_or(std::cmp::Ordering::Equal));
                let measured: f64 = v.iter().map(|x| x.1 .1).sum();
                b.line(&format!("PERFIL (tiempo propio, todo el recorrido): medido {:.1} s de {:.1} s; resto = motor (eventos, colisiones automáticas, dibujado)", measured, total));
                for kind in ["evento", "script", "función"] {
                    b.line(&format!("  -- top {}s --", kind));
                    for (k, (calls, secs)) in v.iter().filter(|(k, _)| matches!((k, kind), (ProfKey::Script(_), "script") | (ProfKey::Func(_), "función") | (ProfKey::Event(..), "evento"))).take(30) {
                        let l = format!("  {:6.2} s {:5.1}% {:>10} llamadas  {:8.2} µs/llamada  {}", secs, secs / total * 100.0, calls, secs / *calls as f64 * 1e6, self.prof_name(*k));
                        b.line(&l);
                    }
                }
            }
        }
        self.bench = Some(b);
        done
    }
}

impl Game {
    /// Un fotograma del recorrido por salas. Devuelve false cuando ha terminado.
    fn bench_tour(&mut self, t: &mut Tour) -> bool {
        let now = Instant::now();
        let ms = now.duration_since(t.last).as_secs_f64() * 1000.0;
        t.last = now;
        if !t.started {
            let from = std::env::var("RLD_GOTO_FROM").ok().and_then(|v| v.parse().ok()).unwrap_or(10);
            if self.room.id != from {
                return true
            }
            t.frames += 1;
            if t.frames < 60 {
                return true
            }
            t.started = true;
            t.frames = 0;
            return self.tour_next(t)
        }
        t.frames += 1;
        // los primeros fotogramas tras el salto (carga de la sala) no cuentan para la media
        if t.frames > 10 {
            t.sum_ms += ms;
            t.worst_ms = t.worst_ms.max(ms);
        } else if t.frames == 10 {
            t.draw_ns0 = crate::game::PERF_DRAW_NS.with(|c| c.get());
        }
        t.max_inst = t.max_inst.max(self.room.instance_list.count_all());
        if t.frames == t.dwell / 2 {
            self.renderer.flush_queue();
            let (w, h) = (self.unscaled_width, self.unscaled_height);
            let rgba = self.renderer.get_pixels(0, 0, w as _, h as _);
            if let Some(img) = image::RgbaImage::from_vec(w, h, rgba.into()) {
                let _ = img.save(format!("tour_{}.png", t.target));
            }
        }
        if t.frames >= t.dwell {
            let n = (t.frames - 10).max(1) as f64;
            let draw = (crate::game::PERF_DRAW_NS.with(|c| c.get()) - t.draw_ns0) as f64 / 1e6 / n;
            let name = self.assets.rooms.get_asset(t.target).map(|r| self.decode_str(r.name.as_ref()).into_owned()).unwrap_or_default();
            let line = format!(
                "{}\t{}\t{}\t{}\t{:.2}\t{:.1}\t{:.2}\t{}\t{}",
                t.target, name, self.room.id, t.max_inst, t.sum_ms / n, t.worst_ms, draw,
                self.errors_logged - t.errors0, self.tour_first_error
            );
            if let Some(f) = t.log.as_mut() {
                let _ = writeln!(f, "{}", line);
                let _ = f.flush();
            }
            return self.tour_next(t)
        }
        true
    }

    fn tour_next(&mut self, t: &mut Tour) -> bool {
        let Some(&target) = t.list.get(t.idx) else {
            t.frames = 0;
            t.dwell = u64::MAX;
            return false
        };
        t.idx += 1;
        t.target = target;
        t.frames = 0;
        t.sum_ms = 0.0;
        t.worst_ms = 0.0;
        t.max_inst = 0;
        t.errors0 = self.errors_logged;
        self.tour_first_error.clear();
        let _ = std::fs::write("tour_cur.txt", target.to_string());
        // Viaje como una puerta del juego (script DoorEnter): punto de aparición de tour_spawns.txt
        // ("sala x y" por línea); si no hay, salto directo a la sala
        let spawn = std::fs::read_to_string("tour_spawns.txt").ok().and_then(|txt| {
            txt.lines().find_map(|l| {
                let v: Vec<i32> = l.split_whitespace().filter_map(|x| x.parse().ok()).collect();
                (v.len() == 3 && v[0] == target).then(|| (v[1], v[2]))
            })
        });
        // sin punto conocido: el centro de la sala (fuera de los límites el juego hace perder el conocimiento)
        let spawn = spawn.or_else(|| self.assets.rooms.get_asset(target).map(|r| (r.width as i32 / 2, r.height as i32 / 2)));
        match spawn {
            Some((x, y)) => {
                let code = format!(
                    "with((14)){{ global.room_out=room; global.xx={x}; global.yy={y}; global.index_place=0; global.ciclista=false; ex(5171); global.mira=(14).mira; global.lugar={target}; room=global.lugar; }}"
                );
                let dummy = self
                    .room
                    .instance_list
                    .insert_dummy(crate::instance::Instance::new_dummy(self.assets.objects.get_asset(0).map(|x| x.as_ref())));
                let mut ctx = crate::gml::Context::with_single_instance(dummy);
                let r = self.execute_string(&mut ctx, &[crate::gml::Value::from(code)]);
                self.room.instance_list.remove_dummy(dummy);
                if r.is_err() || self.scene_change.is_none() {
                    self.scene_change = Some(crate::game::SceneChange::Room(target));
                }
            },
            None => self.scene_change = Some(crate::game::SceneChange::Room(target)),
        }
        true
    }
}

impl Game {
    /// Nombres de las scripts en curso (pila del perfilador del benchmark), de fuera a dentro
    pub fn bench_stack_names(&self) -> String {
        let Some(b) = self.bench.as_ref() else { return String::new() };
        let ks: Vec<ProfKey> = b.prof_stack.iter().map(|x| x.0).collect();
        ks.iter().map(|k| self.prof_name(*k)).collect::<Vec<_>>().join(" > ")
    }
}
