//! Precompilación por sala de las scripts pesadas.
//!
//! Las scripts se compilan la primera vez que se llaman. Las grandes (SecEffect1/2 de los combates,
//! ~170 KB cada una) tardan 10-15 ms en PC y ~250 ms en la Vita, y eso se nota como un tirón en
//! mitad del combate. Compilarlo todo al arrancar cuesta demasiada memoria (+270 MB), así que:
//! - cada script que tarda en compilar más de un umbral se apunta junto a la sala en la que se
//!   necesitó (archivo rld_scripts_pesadas.txt en la carpeta del juego);
//! - al entrar en una sala se compilan las que esa sala necesitó otras veces, durante el cambio de
//!   sala (que ya es una pausa), en vez de a mitad de la acción.
//! Hay una lista inicial para el combate para que el primer combate ya vaya fino.

use super::Game;
use crate::util::FastHashMap;
use std::{collections::HashSet, io::Write, time::Instant};

const FILE: &str = "rld_scripts_pesadas.txt";

/// Sala de combate y scripts que se compilan en mitad de los ataques
const SEED: &[(i32, &str)] = &[
    (191, "SecEffect1"),
    (191, "SecEffect2"),
    (191, "EffectAtackDestroy"),
    (191, "Ataque1GolpeaPokemon1"),
    (191, "Ataque1GolpeaPokemon2"),
    (191, "Ataque2GolpeaPokemon1"),
    (191, "Ataque2GolpeaPokemon2"),
    (191, "EffectHabilityCreate"),
    (191, "CreateAttack"),
    (191, "EffectAtack"),
];

#[derive(Default)]
pub struct HotScripts {
    by_room: FastHashMap<i32, Vec<usize>>,
    known: HashSet<usize>,
    pub disabled: bool,
    loaded: bool,
}

/// Umbral a partir del cual una compilación se considera un tirón
fn threshold_ms() -> f64 {
    if cfg!(target_os = "vita") { 25.0 } else { 1.5 }
}

impl Game {
    fn hot_load(&mut self) {
        if self.hot.loaded {
            return
        }
        self.hot.loaded = true;
        self.hot.disabled = std::env::var_os("RLD_NO_HOT").is_some();
        let mut entries: Vec<(i32, String)> = SEED.iter().map(|(r, n)| (*r, n.to_string())).collect();
        if let Ok(text) = std::fs::read_to_string(FILE) {
            for line in text.lines() {
                if let Some((r, n)) = line.split_once('\t') {
                    if let Ok(r) = r.trim().parse() {
                        entries.push((r, n.trim().to_string()));
                    }
                }
            }
        }
        let mut names: FastHashMap<String, usize> = Default::default();
        for (i, s) in self.assets.scripts.iter().enumerate() {
            if let Some(s) = s {
                names.insert(self.decode_str(s.name.as_ref()).into_owned(), i);
            }
        }
        for (room, name) in entries {
            if let Some(&id) = names.get(&name) {
                let list = self.hot.by_room.entry(room).or_default();
                if !list.contains(&id) {
                    list.push(id);
                }
                self.hot.known.insert(id);
            }
        }
    }

    /// Llamar tras compilar una script: si ha tardado, apuntarla para la sala actual.
    pub fn hot_note_compile(&mut self, id: usize, ms: f64) {
        if ms < threshold_ms() {
            return
        }
        self.hot_load();
        if self.hot.disabled || !self.hot.known.insert(id) {
            return
        }
        let room = self.room.id;
        if let Some(Some(s)) = self.assets.scripts.get(id) {
            let name = self.decode_str(s.name.as_ref()).into_owned();
            // Las de combate se compilan al empezar el combate (pausa de transición), aunque se
            // llamen en la sala del mapa donde te ve el entrenador: no cargar con ellas esa sala.
            if ["Ataque", "PrepareBattle", "SecEffect", "Effect", "Stat", "TrainerPokemon"].iter().any(|p| name.starts_with(p)) {
                return
            }
            self.hot.by_room.entry(room).or_default().push(id);
            if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(FILE) {
                let _ = writeln!(f, "{}\t{}", room, name);
            }
        }
    }

    /// Al entrar en una sala: compilar ya las scripts pesadas que esa sala ha necesitado antes.
    pub fn hot_prewarm(&mut self, room: i32) {
        self.hot_load();
        if self.hot.disabled {
            return
        }
        let Some(list) = self.hot.by_room.get(&room).cloned() else { return };
        let t0 = Instant::now();
        let mut n = 0;
        for id in list {
            let pending = matches!(self.assets.scripts.get(id), Some(Some(s)) if s.compiled.get().is_none());
            if pending && self.script_code(id).is_ok() {
                n += 1;
            }
        }
        if n > 0 && std::env::var_os("RLD_DEBUG_COMPILE").is_some() {
            eprintln!("precompilar sala {}: {} scripts en {:.1} ms", room, n, t0.elapsed().as_secs_f64() * 1e3);
        }
    }
}

