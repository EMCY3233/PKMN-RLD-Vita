#[cfg(all(feature = "vita", not(feature = "desktop")))]
extern crate ramen_vita as ramen;

mod action;
mod asset;
mod game;
mod gml;
mod handleman;
#[cfg(feature = "desktop")]
mod imgui_key_utils;
#[cfg(feature = "desktop")]
mod imgui_utils;
mod input;
mod instance;
mod instancelist;
mod math;
mod render;
mod tile;
mod types;
mod util;

use game::{
    savestate::{self, SaveState},
    Game, GameClock, PlayType, Replay,
};
use std::{
    env, fs,
    path::{Path, PathBuf},
    process,
};

const EXIT_SUCCESS: i32 = 0;
const EXIT_FAILURE: i32 = 1;

fn help(argv0: &str, opts: getopts::Options) {
    print!(
        "{}",
        opts.usage(&format!(
            "Usage: {} FILE [options]",
            match Path::new(argv0).file_name() {
                Some(file) => file.to_str().unwrap_or(argv0),
                None => argv0,
            }
        ))
    );
}

/// Registro de arranque: en Vita va a ux0:data/reloaded/runner.log (no hay consola).
pub fn startlog(msg: &str) {
    // En Linux, añade la memoria usada (actual y pico) para localizar picos de RAM
    #[cfg(target_os = "linux")]
    let msg = &{
        let st = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
        let g = |k: &str| st.lines().find(|l| l.starts_with(k)).map(|l| l.split_whitespace().nth(1).unwrap_or("0").parse::<u64>().unwrap_or(0) / 1024).unwrap_or(0);
        format!("{msg}   [RAM {} MB, pico {} MB]", g("VmRSS"), g("VmHWM"))
    };
    eprintln!("{msg}");
    #[cfg(target_os = "vita")]
    {
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(format!("{VITA_DIR}/runner.log")) {
            let _ = writeln!(f, "{msg}");
        }
    }
}

fn main() {
    #[cfg(target_os = "vita")]
    {
        let _ = std::fs::create_dir_all(VITA_DIR);
        let _ = std::fs::remove_file(format!("{VITA_DIR}/runner.log"));
    }
    std::panic::set_hook(Box::new(|info| startlog(&format!("PANIC: {info}"))));
    startlog(&format!("Reloaded Runner {} ({}) arrancando", env!("CARGO_PKG_VERSION"), std::env::consts::OS));
    // PS Vita: CPU a 444 MHz (por defecto 333), bus y GPU al máximo
    #[cfg(target_os = "vita")]
    unsafe {
        use vitasdk_sys::*;
        let r = scePowerSetArmClockFrequency(444);
        scePowerSetBusClockFrequency(222);
        scePowerSetGpuClockFrequency(222);
        scePowerSetGpuXbarClockFrequency(166);
        startlog(&format!("Overclock CPU 444 MHz: {}", if r >= 0 { "OK" } else { "falló" }));
    }
    // Sin renderizador real no hacen falta los píxeles de sprites/fondos: no guardarlos (~190 MB)
    #[cfg(not(feature = "desktop"))]
    gm8exe::SKIP_PIXELS.store(true, std::sync::atomic::Ordering::Relaxed);
    // El intérprete de GML es recursivo: en PS Vita el hilo principal solo tiene ~256 KB de pila y
    // el juego la desbordaba en la selección de personaje. Se ejecuta en un hilo con 16 MB.
    let code = std::thread::Builder::new()
        .name("runner".into())
        .stack_size(16 * 1024 * 1024)
        .spawn(xmain)
        .and_then(|h| h.join().map_err(|_| std::io::Error::other("el hilo del runner terminó con un panic")))
        .unwrap_or_else(|e| {
            startlog(&format!("ERROR: {e}"));
            EXIT_FAILURE
        });
    startlog(&format!("Fin (código {code})"));
    process::exit(code);
}

/// Carpeta del juego en la PS Vita
#[cfg(target_os = "vita")]
const VITA_DIR: &str = "ux0:data/reloaded";

/// Montón (heap) de newlib en PS Vita: cargar el juego necesita ~300 MB (por defecto solo hay 128 MB)
#[cfg(target_os = "vita")]
#[used]
#[no_mangle]
pub static _newlib_heap_size_user: u32 = 280 * 1024 * 1024;

fn xmain() -> i32 {
    #[cfg(not(target_os = "vita"))]
    let args: Vec<String> = env::args().collect();
    // En la Vita no hay línea de comandos: de momento arranca siempre en modo benchmark
    #[cfg(target_os = "vita")]
    let args: Vec<String> = vec![
        "gm8emulator".into(),
        format!("{VITA_DIR}/game.exe"),
        "--bench".into(),
        "5400".into(),
        "--bench-log".into(),
        format!("{VITA_DIR}/bench.log"),
    ];
    let process = args[0].clone();

    let mut opts = getopts::Options::new();
    opts.optflag("h", "help", "prints this help message");
    opts.optflag("s", "strict", "enable various data integrity checks");
    opts.optflag("t", "singlethread", "parse gamedata synchronously");
    opts.optflag("v", "verbose", "enables verbose logging");
    opts.optflag("r", "realtime", "disables clock spoofing");
    opts.optflag("c", "capture", "captures a recording");
    opts.optflagopt("l", "no-framelimit-until", "disables the frame-limiter until specified frame", "FRAME");
    opts.optopt("n", "project-name", "name of TAS project to create or load", "NAME");
    opts.optopt("f", "replay-file", "path to savestate file to replay", "FILE");
    opts.optopt("o", "output-file", "output savestate name in replay mode", "FILE.bin");
    opts.optopt("B", "bench", "benchmark: run N frames without frame limiter, with scripted inputs, and log FPS", "FRAMES");
    opts.optopt("", "bench-log", "where to write the benchmark log (default: bench.log)", "FILE");
    opts.optmulti("a", "game-arg", "argument to pass to the game", "ARG");
    opts.optflagopt("p", "start-save", "Either loads the savestate specified after this parameter or starts at the first frame. If a .gmtas is specified by -f this will start the replay from this savestate instead", "savestate");

    let matches = match opts.parse(&args[1..]) {
        Ok(matches) => matches,
        Err(fail) => {
            use getopts::Fail::*;
            match fail {
                ArgumentMissing(arg) => eprintln!("missing argument {}", arg),
                UnrecognizedOption(opt) => eprintln!("unrecognized option {}", opt),
                OptionMissing(opt) => eprintln!("missing option {}", opt),
                OptionDuplicated(opt) => eprintln!("duplicated option {}", opt),
                UnexpectedArgument(arg) => eprintln!("unexpected argument {}", arg),
            }
            return EXIT_FAILURE;
        },
    };

    if args.len() < 2 || matches.opt_present("h") {
        help(&process, opts);
        return EXIT_SUCCESS;
    }

    let strict = matches.opt_present("s");
    let multithread = !matches.opt_present("t");
    let spoof_time = !matches.opt_present("r");
    let capture_recording = matches.opt_present("c");
    let frame_limit_at = matches
        .opt_str("l")
        .map(|frame| match frame.parse::<usize>() {
            Ok(f) => f,
            Err(e) => {
                panic!("{}", e);
            },
        })
        .unwrap_or(0);
    let bench_frames = matches.opt_str("B").and_then(|f| f.parse::<u64>().ok());
    let bench_log = matches.opt_str("bench-log").unwrap_or_else(|| "bench.log".into());
    let frame_limiter = !matches.opt_present("l") && bench_frames.is_none();
    let verbose = matches.opt_present("v");
    let output_bin = matches.opt_str("o").map(PathBuf::from);
    let pause = matches.opt_present("p");
    let start_save_path = matches.opt_str("p").map(PathBuf::from);
    let project_path = matches.opt_str("n").map(|name| {
        let mut p = env::current_dir().expect("std::env::current_dir() failed");
        p.push("projects");
        p.push(name);
        p
    });

    if let Some(bin) = &output_bin {
        if bin.extension().and_then(|x| x.to_str()) != Some("bin") {
            eprintln!("invalid output file for -o: must be a .bin file");
            return EXIT_FAILURE;
        }
    }

    let temp_dir = project_path.as_ref().map(|proj_path| {
        // attempt to find temp dir in project path
        std::fs::read_dir(proj_path)
            .ok()
            .and_then(|iter| {
                iter.filter_map(|x| x.ok())
                    .find(|p| {
                        p.metadata().ok().filter(|p| p.is_dir()).is_some()
                            && p.file_name().to_str().filter(|s| s.starts_with("gm_ttt_")).is_some()
                    })
                    .map(|entry| entry.path())
            })
            // if we can't find one, make one
            .unwrap_or_else(|| {
                let mut random_int = [0u8; 4];
                getrandom::getrandom(&mut random_int).expect("Couldn't generate a random number");
                let path = [proj_path.clone(), format!("gm_ttt_{}", u32::from_le_bytes(random_int) % 100000).into()]
                    .iter()
                    .collect();
                if let Err(e) = std::fs::create_dir_all(&path) {
                    println!("Could not create temp folder: {}", e);
                    println!("If this game uses the temp folder, it will most likely crash.");
                }
                path
            })
    });
    let can_clear_temp_dir = temp_dir.is_none();
    let replay = match matches
        .opt_str("f")
        .map(|filename| {
            let filepath = PathBuf::from(&filename);
            match filepath.extension().and_then(|x| x.to_str()) {
                Some("bin") => match SaveState::from_file(&filepath, &mut savestate::Buffer::new()) {
                    Ok(state) => Ok(state.into_replay()),
                    Err(e) => Err(format!("couldn't load {:?}: {:?}", filepath, e)),
                },

                Some("gmtas") => match Replay::from_file(&filepath) {
                    Ok(replay) => Ok(replay),
                    Err(e) => Err(format!("couldn't load {:?}: {:?}", filepath, e)),
                },

                _ => Err("unknown filetype for -f, expected '.bin' or '.gmtas'".into()),
            }
        })
        .transpose()
    {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{}", e);
            return EXIT_FAILURE;
        },
    };

    let input = {
        if matches.free.len() == 1 {
            &matches.free[0]
        } else if matches.free.len() > 1 {
            eprintln!("unexpected second input {}", matches.free[1]);
            return EXIT_FAILURE;
        } else {
            eprintln!("no input file");
            return EXIT_FAILURE;
        }
    };

    let mut game_args = matches.opt_strs("game-arg");
    game_args.insert(0, input.to_string());
    let game_args = game_args;

    let file_path = Path::new(&input);

    startlog(&format!("Leyendo {}", input));
    let mut file = match fs::read(file_path) {
        Ok(data) => data,
        Err(err) => {
            startlog(&format!("ERROR: no se puede abrir '{}': {}", input, err));
            return EXIT_FAILURE;
        },
    };

    if verbose {
        println!("loading '{}'...", input);
    }

    #[rustfmt::skip]
    let assets = gm8exe::reader::from_exe(
        &mut file,                              // mut exe: AsRef<[u8]>
        if verbose {                            // logger: Option<Fn(&str)>
            Some(|s: &str| println!("{}", s))
        } else {
            None
        },
        strict,                                 // strict: bool
        multithread,                            // multithread: bool
    );
    let assets = match assets {
        Ok(assets) => {
            // El .exe ya no hace falta: liberar sus 22 MB cuanto antes (importante en PS Vita)
            drop(std::mem::take(&mut file));
            startlog("Juego leído y descomprimido");
            assets
        },
        Err(err) => {
            startlog(&format!("ERROR al leer el juego '{}' - {}", input, err));
            return EXIT_FAILURE;
        },
    };

    // En algunas plataformas (PS Vita) canonicalize no está disponible: usar la ruta tal cual
    let absolute_path = match file_path.canonicalize() {
        Ok(p) => p,
        Err(_) if file_path.is_absolute() || cfg!(target_os = "vita") => file_path.to_path_buf(),
        Err(e) => {
            eprintln!("Failed to resolve game path: {}", e);
            return EXIT_FAILURE;
        },
    };

    // Pokémon Reloaded (y casi todos los juegos GM8 occidentales) usan la página de códigos de Windows-1252
    let encoding = encoding_rs::WINDOWS_1252;

    // Carpeta del juego: rutas relativas del juego ("data\\...") y reloaded.pak se resuelven desde aquí
    if let Some(dir) = absolute_path.parent() {
        #[cfg(target_os = "vita")]
        let _ = env::set_current_dir(dir);
        gml::pak::init(dir);
        if env::var_os("RLD_SELFTEST").is_some() {
            game::accel::selftest_text_lines(dir);
        }
        match env::var("RLD_SELFTEST_FS").as_deref() {
            Ok("1") => game::accel::selftest_fs(),
            Ok("2") => game::accel::selftest_fs_2(),
            _ => (),
        }
    }

    let play_type = if project_path.is_some() {
        PlayType::Record
    } else if replay.is_some() {
        PlayType::Replay
    } else {
        PlayType::Normal
    };

    let mut components = match Game::launch(
        assets,
        absolute_path,
        game_args,
        temp_dir,
        encoding,
        frame_limiter,
        frame_limit_at,
        capture_recording,
        play_type,
    ) {
        Ok(g) => { startlog("Juego iniciado; empieza el benchmark"); g },
        Err(e) => {
            startlog(&format!("ERROR al iniciar el juego: {}", e));
            return EXIT_FAILURE;
        },
    };

    components.startup_diagnostics();
    if env::var_os("RLD_SELFTEST").is_some() {
        components.accel_selftest_tables();
    }

    if let Some(n) = bench_frames {
        components.bench = Some(game::bench::Bench::new(n, &bench_log));
    }

    let time_now = GameClock::SpoofedNanos(gml::datetime::now_as_nanos());

    if let Err(err) = if let Some(path) = project_path {
        components.clock = time_now;
        #[cfg(feature = "desktop")]
        components.record(path, pause, start_save_path.as_ref());
        #[cfg(not(feature = "desktop"))]
        { let _ = (path, pause, start_save_path); eprintln!("El modo TAS solo existe en la versión de PC"); }
        Ok(())
    } else {
        // cache temp_dir and included files because the other functions take ownership
        let temp_dir: Option<PathBuf> = if can_clear_temp_dir {
            Some(components.decode_str(components.temp_directory.as_ref()).into_owned().into())
        } else {
            None
        };
        let files_to_delete = components
            .included_files
            .iter()
            .filter(|i| i.remove_at_end)
            .map(|i| PathBuf::from(components.decode_str(i.name.as_ref()).into_owned()))
            .collect::<Vec<_>>();
        let result = if let Some(replay) = replay {
            components.replay(replay, output_bin, start_save_path.as_ref())
        } else {
            components.clock = if spoof_time { time_now } else { GameClock::StartupEpoch(std::time::Instant::now()) };
            components.run()
        };
        for file in files_to_delete.into_iter() {
            std::fs::remove_file(file).ok();
        }
        if let Some(temp_dir) = temp_dir {
            std::fs::remove_dir_all(temp_dir).ok();
        }
        result
    } {
        startlog(&format!("Error durante la ejecución: {}", err));
        EXIT_FAILURE
    } else {
        EXIT_SUCCESS
    }
}
