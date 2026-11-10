//! Imita la parte de la API de `ramen` que usa gm8emulator.
//! En PS Vita no hay ventanas: la "ventana" es la pantalla completa y los eventos
//! (botones convertidos a teclas, táctil convertido a ratón) se inyectan con `push_event`.
//! input.rs, event.rs y decoration.rs proceden de ramen (licencia Zlib).
pub mod event;
pub mod input;

pub mod error {
    #[derive(Debug, Clone)]
    pub struct Error(pub String);
    impl std::fmt::Display for Error {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str(&self.0) }
    }
    impl std::error::Error for Error {}
}

pub mod connection {
    use crate::{error::Error, window::Builder};
    #[derive(Clone)]
    pub struct Connection;
    impl Connection {
        pub fn new() -> Result<Self, Error> { Ok(Connection) }
        pub fn builder(&self) -> Builder { Builder::default() }
    }
}

pub mod window {
    mod decoration;
    pub use decoration::{Controls, Style};
    use crate::{error::Error, event::Event};
    use std::{collections::VecDeque, sync::Mutex};

    /// Cola global de eventos que rellena la capa de entrada de la plataforma.
    static PENDING: Mutex<VecDeque<Event>> = Mutex::new(VecDeque::new());

    /// Inyecta un evento (p. ej. botón de Vita convertido en tecla).
    pub fn push_event(e: Event) { if let Ok(mut q) = PENDING.lock() { q.push_back(e); } }

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Cursor {
    /// ⇖
    Arrow = 0,

    /// (Invisible)
    Blank,

    /// \+
    Cross,

    /// 👆
    Hand,

    /// 👆?
    Help,

    /// I
    IBeam,

    /// ⇖⌛
    Progress,

    /// ⤢
    ResizeNESW,

    /// ↕
    ResizeNS,

    /// ⤡
    ResizeNWSE,

    /// ↔
    ResizeWE,

    /// ✥
    ResizeAll,

    /// 🚫
    Unavailable,

    /// ⌛
    Wait,
}

/// Represents an open window. Dropping it closes the window.

    #[derive(Default)]
    pub struct Builder { size: (u16, u16) }
    impl Builder {
        pub fn build(self) -> Result<Window, Error> { Ok(Window { events: Vec::new(), size: self.size }) }
        pub fn borderless(self, _: bool) -> Self { self }
        pub fn cursor(self, _: Cursor) -> Self { self }
        pub fn controls(self, _: Option<Controls>) -> Self { self }
        pub fn class_name<T: Into<std::borrow::Cow<'static, str>>>(self, _: T) -> Self { self }
        pub fn resizable(self, _: bool) -> Self { self }
        pub fn title<T: Into<std::borrow::Cow<'static, str>>>(self, _: T) -> Self { self }
        pub fn maximised(self, _: bool) -> Self { self }
        pub fn position(self, _: Option<(i16, i16)>) -> Self { self }
        pub fn size(mut self, size: (u16, u16)) -> Self { self.size = size; self }
        pub fn visible(self, _: bool) -> Self { self }
        pub fn depth(self, _: u8) -> Self { self }
        pub fn visual(self, _: u32) -> Self { self }
    }

    pub struct Window { events: Vec<Event>, size: (u16, u16) }
    impl Window {
        pub fn events(&self) -> impl IntoIterator<Item = &Event> { self.events.iter() }
        pub fn poll_events(&mut self) {
            self.events.clear();
            if let Ok(mut q) = PENDING.lock() { self.events.extend(q.drain(..)); }
        }
        pub fn size(&self) -> (u16, u16) { self.size }
        pub fn set_cursor(&self, _: Cursor) {}
        pub fn set_borderless(&self, _: bool) {}
        pub fn set_controls(&self, _: Option<Controls>) {}
        pub fn set_maximised(&self, _: bool) {}
        pub fn set_position(&self, _: (i16, i16)) {}
        pub fn set_resizable(&self, _: bool) {}
        pub fn set_size(&self, _: (u16, u16)) {}
        pub fn set_title(&self, _: &str) {}
        pub fn set_visible(&self, _: bool) {}
    }
}
