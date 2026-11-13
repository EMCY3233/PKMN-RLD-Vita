use crate::{
    game::{Background, View},
    gml::{self, runtime::Instruction},
    tile::Tile,
    types::{Colour, ID},
};
use serde::{Deserialize, Serialize};
use std::rc::Rc;

#[derive(Clone, Serialize, Deserialize)]
pub struct Room {
    pub name: gml::String,
    pub caption: gml::String,
    pub width: u32,
    pub height: u32,
    pub speed: u32,
    pub persistent: bool,
    pub bg_colour: Colour,
    pub clear_screen: bool,
    pub creation_code: Result<Rc<[Instruction]>, String>,

    pub backgrounds: Vec<Background>,
    pub views_enabled: bool,
    pub views: Vec<View>,
    pub instances: Vec<Instance>,
    /// plantillas compactas (592.000 tiles en el juego: con tile::Tile ocupaban ~47 MB)
    pub tiles: Vec<RoomTile>,
}

/// Tile guardado en una sala (se convierte en tile::Tile al entrar en ella). GM8.0 no tiene escala
/// de tiles (siempre 1), así que la escala va en f32 sin perder nada.
#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct RoomTile {
    pub x: i32,
    pub y: i32,
    pub source_bg: ID,
    pub tile_x: i32,
    pub tile_y: i32,
    pub width: i32,
    pub height: i32,
    pub depth: i32,
    pub id: ID,
    pub blend: u32,
    pub xscale: f32,
    pub yscale: f32,
}

impl RoomTile {
    pub fn to_tile(&self) -> Tile {
        use crate::math::Real;
        use std::cell::Cell;
        Tile {
            x: Cell::new(self.x.into()),
            y: Cell::new(self.y.into()),
            background_index: Cell::new(self.source_bg),
            tile_x: Cell::new(self.tile_x),
            tile_y: Cell::new(self.tile_y),
            width: Cell::new(self.width),
            height: Cell::new(self.height),
            depth: Cell::new(self.depth.into()),
            id: Cell::new(self.id),
            alpha: Cell::new(Real::from(self.blend >> 24) / Real::from(255)),
            blend: Cell::new((self.blend & 0xFFFFFF) as i32),
            xscale: Cell::new(f64::from(self.xscale).into()),
            yscale: Cell::new(f64::from(self.yscale).into()),
            visible: Cell::new(true),
        }
    }
}

/// An instance stored in a Room
#[derive(Clone, Serialize, Deserialize)]
pub struct Instance {
    pub x: i32,
    pub y: i32,
    pub object: i32,
    pub id: ID,
    pub creation: Result<Rc<[Instruction]>, String>,
    /// GM8.0 no tiene escala ni ángulo por instancia en las salas (1 y 0): f32 sin pérdida
    pub xscale: f32,
    pub yscale: f32,
    pub blend: u32,
    pub angle: f32,
}
