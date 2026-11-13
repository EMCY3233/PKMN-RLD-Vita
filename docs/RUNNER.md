# Runner OpenGMK para Pokémon Reloaded

## Estado (10/11/2026)
- OpenGMK `gm8emulator` compila y **ejecuta Pokémon Reloaded 1.9.1** (probado en Linux, con OpenGL por software).
  Funcionan: arranque, menú principal, textos cifrados, ventanas de mensajes, música y entrada de teclado.
- Parches propios (`opengmk-reloaded.patch`, o los scripts `patch_*.py`):
  1. GLX: vsync opcional (no rompe en drivers sin `GLX_EXT_swap_control`).
  2. Implementadas las 19 funciones GM8 que usa el juego y faltaban: `room_set_code`, `sprite_duplicate`,
     `sound_fade`, `display_mouse_*`, `window_mouse_set`, diálogos y `message_*` (neutros), `get_open_filename`, etc.
  3. `keyboard_key_press/release` (147 usos): cola de teclas simuladas.
  4. `keyboard_string` / `keyboard_lastchar` (pantallas de nombre y frase), respetando Latin-1 (tildes, ñ).
- **1.9.2.2 probada:** partida nueva → personaje → nombre → rival → frase → **mapa del laboratorio jugable**.
- La 1.9.1 está caducada a propósito por el autor (`ValidateCV`); se usa siempre la última versión.
- Teclas 1.9.1 → 1.9.2.2: sin cambios. Funciones nuevas sin soporte: ninguna.

## Aplicar
```bash
git clone https://github.com/OpenGMK/OpenGMK && cd OpenGMK && git apply ../runner/opengmk-reloaded.patch
cargo build --release -p gm8emulator
```
`play.sh` = prueba automática (Xvfb + xdotool + capturas).

## Siguiente: capa Vita
- Sustituir `ramen` (ventana/entrada) por SDL2: ~330 referencias, concentradas en `input.rs`, `game.rs`, `render/opengl.rs`.
- Shaders `#version 330 core` → GLSL ES 1.0 para vitaGL. Audio `udon/libsoundio` → SDL2 audio.
- Compilar con `cargo-vita` (target `armv7-sony-vita-newlibeabihf`, Rust nightly + build-std) en GitHub Actions
  usando la imagen Docker `vitasdk/vitasdk` (este entorno no tiene acceso a las descargas de VitaSDK).
