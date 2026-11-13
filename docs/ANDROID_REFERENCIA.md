# Qué aprovechar del port de Android (Klisee/pokemon-reloaded-android)

Revisado el 10/11/2026 (116 commits, versión 0.4.x, en modo mantenimiento; el autor anuncia
"Reloaded OpenGMK" multiplataforma). Mismo enfoque que este port: OpenGMK ejecutando el juego original
sin tocarlo. Licencia GPL-2.0, igual que OpenGMK y este proyecto: se puede reutilizar código citando
el origen.

## Ya incorporado a este port

| Arreglo de Android | Problema que resuelve | Aquí |
|---|---|---|
| a79fc2a – sala persistente | OpenGMK guardaba la sala mirando la **nueva**; la mochila en combate reiniciaba el combate → **XP infinita** | ✅ `game.rs` (load_room) |
| e0de882 / 0434cff – máscara vacía | Sprites `.gmspr` sin píxeles de máscara nunca chocaban → **Vuelo** no aterriza y el combate se cuelga | ✅ en `sprite_add_sprite` (nuestra máscara se calcula de los píxeles, pero se añade la misma red de seguridad) |
| 4e15417 / 6898ba3 – variables sin asignar | "uninitialized variable ACTUAL" cerraba la partida en **combates dobles** | ✅ valen 0 y, además, cualquier error deja seguir el juego como GM8 (`RLD_STRICT=1` para depurar) |

## Para las siguientes fases

| Pieza | Dónde | Uso en Vita |
|---|---|---|
| **Renderizador OpenGL ES** con arreglos para GPU PowerVR (TBDR): enlazar VAO/VBO/programa en cada dibujo, no invalidar búferes, `std140` correcto, subir atlas por lotes (arranque 85 s → instantes) | `render/opengl.rs`, commit fc195ef | La GPU de la Vita (SGX543MP4+) **también es PowerVR TBDR**: los mismos errores aparecerían con vitaGL. Base directa del renderizador Vita |
| Primitivas rellenas (sombras, barras de PS) | commit 3d60c95 | Mismo renderizador |
| `sprite_width` sin escala y UV de `draw_sprite_part` con ancho negativo (montura del Surf a la izquierda) | commit 760d8b8 | La parte de las UV va con el renderizador. Lo de `sprite_width` contradice a OpenGMK (que lo verifica contra GM8): **comprobar en Windows antes de aplicarlo** |
| **Música MIDI**: sintetizador `rustysynth` (Rust puro) en streaming como fuente de `udon`, con un SoundFont de 6 MB | `game/audio/midi.rs`, commit 5986012 | Funciona en Vita sin cambios; ponerlo en otro núcleo y a 22 050 Hz si la CPU no llega |
| Ventanas `show_info` (RTF a color) y `show_image` | `game/info_window.rs` | Las imágenes de los científicos y ayudas |
| Controles táctiles que cambian según el estado del juego (mapa, menús, combate, Safari, listas) y nombres de salas/personajes | `touch_controls.rs`, `field_controls.rs`, `assets/room_names.json` | Ideas para el táctil de la Vita (el puzle necesita ratón) |
| Pausa real al ir a segundo plano, recuperación del contexto gráfico | commits 58f0b94, 18a4590 | Equivale a suspender la Vita |
| Modos de 60/120 FPS por interpolación | commit dca35d9 | No prioritario (la Vita va a 30) |

## No aplicable

Lanzador Kotlin, importador del zip, Discord Rich Presence, "Reloaded Online", mods, autoactualizador.
