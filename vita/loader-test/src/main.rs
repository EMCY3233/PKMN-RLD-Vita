//! Prueba del núcleo del runner en PS Vita:
//! lee ux0:data/reloaded/game.exe con el parser de OpenGMK y escribe un informe.
use std::{fs, io::Write, time::Instant};

/// Tamaño del montón (heap) de newlib en Vita: el juego necesita ~300 MB al cargarse.
#[cfg(target_os = "vita")]
#[used]
#[no_mangle]
pub static _newlib_heap_size_user: u32 = 340 * 1024 * 1024;

const BASE: &str = if cfg!(target_os = "vita") { "ux0:data/reloaded" } else { "/tmp/rld" };

fn main() {
    let _ = fs::create_dir_all(BASE);
    // Si algo falla (panic), queda escrito en el log
    std::panic::set_hook(Box::new(|info| {
        if let Ok(mut f) = fs::OpenOptions::new().append(true).open(format!("{BASE}/loader_test.log")) {
            let _ = writeln!(f, "PANIC: {info}");
        }
    }));
    let mut log = fs::File::create(format!("{BASE}/loader_test.log")).expect("no se puede crear el log");
    let _ = writeln!(log, "Reloaded Loader Test - plataforma: {}", std::env::consts::OS);
    let t = Instant::now();
    match fs::read(format!("{BASE}/game.exe")) {
        Err(e) => { let _ = writeln!(log, "ERROR: no encuentro {BASE}/game.exe ({e})"); }
        Ok(mut exe) => {
            let _ = writeln!(log, "game.exe: {} bytes, leído en {:?}", exe.len(), t.elapsed());
            let _ = writeln!(log, "Procesando el juego (puede tardar)...");
            let _ = log.sync_all();
            let t = Instant::now();
            match gm8exe::reader::from_exe(&mut exe, None::<fn(&str)>, false, false) {
                Ok(g) => {
                    let n = |c: usize| c;
                    let _ = writeln!(log, "OK: parseado en {:?}", t.elapsed());
                    let _ = writeln!(log, "sprites {} | sonidos {} | fondos {} | scripts {} | objetos {} | salas {}",
                        n(g.sprites.iter().flatten().count()), n(g.sounds.iter().flatten().count()),
                        n(g.backgrounds.iter().flatten().count()), n(g.scripts.iter().flatten().count()),
                        n(g.objects.iter().flatten().count()), n(g.rooms.iter().flatten().count()));
                }
                Err(e) => { let _ = writeln!(log, "ERROR al parsear: {e}"); }
            }
        }
    }
    let _ = writeln!(log, "Fin. Ya puedes cerrar la app.");
    drop(log);
    if cfg!(target_os = "vita") { loop { std::thread::sleep(std::time::Duration::from_secs(1)); } }
}
