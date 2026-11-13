use crate::gml::{self, runtime::Instruction};
use serde::{Deserialize, Serialize};
use std::rc::Rc;

#[derive(Clone, Serialize, Deserialize)]
pub struct Script {
    pub name: gml::String,
    pub source: gml::String,
    /// Se compila la primera vez que se ejecuta (ver Game::script_code). Compilar las 5075 scripts
    /// de Pokémon Reloaded al arrancar ocupaba ~200 MB, inasumible en PS Vita.
    #[serde(skip)]
    pub compiled: std::cell::OnceCell<Rc<[Instruction]>>,
    /// último cambio de sala en el que se usó (para liberar el código compilado si falta memoria)
    #[serde(skip)]
    pub last_used: std::cell::Cell<u32>,
}
