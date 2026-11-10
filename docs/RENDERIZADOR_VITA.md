# Gráficos en PS Vita (vitaGL)

Estado: **el juego arranca en la PS Vita con gráficos** (11/11/2026). Arreglos necesarios en consola: vglInit devuelve GL_FALSE en un inicio normal; shaders sin el nombre `lerp` ni `mod` de vec2 (traductor GLSL→CG de vitaGL).

## Qué usa el juego

Solo dibujo 2D: `draw_sprite_ext` (1093 usos), textos (`draw_text_color` 871), rectángulos, elipses,
líneas, `draw_background_ext`, `draw_sprite_part`, y `d3d_set_fog` (24 usos, para teñir sprites de un
color, p. ej. destellos en combate). No usa superficies, modos de mezcla, iluminación 3D ni búfer Z.

## Cómo está hecho

- Se reutiliza el renderizador OpenGL de OpenGMK (el de PC), con una plataforma nueva
  (`render/opengl/vgl.rs`) que inicia vitaGL a 960×544 y sustituye lo que vitaGL no tiene:
  tamaño de texturas (`glGetTexLevelParameteriv`), texturas de profundidad, `glProvokingVertex`,
  `glGetTexImage`; además convierte los píxeles BGRA de GM8 a RGBA.
- Shaders 2D simplificados en GLSL 1.00 (`render/glsl/simple_*.glsl`), sin bloque uniforme ni
  iluminación, que vitaGL traduce a CG. En PC (`RLD_SIMPLE_GL=1`) dan **capturas idénticas** a los
  originales.
- **Texturas bajo demanda**: los gráficos del juego ocupan 178 MB en RGBA y la Vita tiene 112 MB de
  VRAM. Se guardan los bloques comprimidos del .exe (~22 MB) y cada sprite se descomprime al dibujarlo
  por primera vez, en páginas de 1024×1024 con presupuesto de 72 MB (se vacía la menos usada).
  En PC (`RLD_LAZY_TEX=1`): capturas idénticas y RAM 815 → 470 MB; el recorrido del benchmark usa solo
  12 MB de páginas. Probado también con 8 MB de presupuesto (vaciando páginas): imagen correcta.

## Controles (build con gráficos)

`game/vita_input.rs` aplica `ux0:data/reloaded/keymap.ini` (o el mapa por defecto `vita/keymap.ini`):
botones y capas L/R, stick = cruceta, SELECT+START = ESC, pantalla táctil = ratón (clic izquierdo),
panel trasero = teclas 1-4.

## Requisitos en la consola

- `ur0:data/libshacccg.suprx` (compilador de shaders de Sony, se extrae con ShaRKBR33D). Sin él vitaGL
  no puede compilar los shaders y el juego se cierra al arrancar (`runner.log` lo indicará).
- Igual que el benchmark: `game.exe` y `reloaded.pak` en `ux0:data/reloaded/`.

## Pendiente

- Probar en consola: compilación de shaders, imagen, velocidad, memoria.
- Audio (de momento sin sonido).
- `show_image`/`show_info` (ventanas informativas).
- Opción de escalado (ahora: el del juego, centrado).
