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
- Audio: salida sceAudioOut (udon/session/vita.rs, puerto BGM 48 kHz, hilo en el núcleo 1). Música MIDI con rustysynth (22 050 Hz, 32 voces, sin reverberación) y SoundFont TimGM6mb en el VPK (sustituible en ux0:data/reloaded/soundfont.sf2). sound_replace siempre carga el archivo nuevo.
- `show_image`/`show_info` (ventanas informativas).
- Presentación: escala entera si cabe al menos x2 (320x180 -> x3); si no (combate 576x324), ajustada a 960x540. Registro de rendimiento cada 5 s en runner.log (FPS, ms de lógica y de dibujo, sala).

## Correcciones (11/11)
- **Contorno negro transparente**: `sprite_add/replace(..., removeback=1)` sobre GIF cuyo color
  transparente es negro borraba también el contorno negro opaco (1.736 imágenes: retratos de
  `data/trainer`, Pokémon de `data/pkmn` y `pkmn_shiny`, huevos, cajas). Ahora, si la esquina inferior
  izquierda ya es transparente, no se borra nada más. Fondos de combate: sin el problema.
- **Ralentización progresiva** (dibujo de 12 a 73 ms en la Ruta 1): cada lote hacía dos `glBufferData`
  (reserva de memoria de vídeo con la CDRAM casi llena). En la Vita los vértices van ahora desde la
  memoria del programa; las páginas de texturas se reinician al quedar vacías y se compactan al cambiar
  de sala si ocupan más de la mitad del presupuesto. El runner.log incluye lotes, cambios de textura,
  páginas y memoria libre de vitaGL cada 5 s.

## Rendimiento (11/11, tarde)
- **Lotes de dibujo**: cada lote cambiaba de textura (~0,3 ms por lote en la Vita) porque las fuentes y el
  píxel blanco de las primitivas estaban en el atlas 0 y los sprites en las páginas diferidas. Ahora las
  páginas son de 2048² y llevan una copia del atlas 0 en su esquina (0,0); `setup_queue` sigue en la página
  cuando llega algo del atlas 0. Ruta 1: ~35 → ~1 lote por fotograma; combate: ~15 → ~2. Imagen idéntica
  píxel a píxel (RLD_NO_MIRROR=1 para comparar).
- **Atlas de fuentes**: se creaba de 4096×4096 (64 MB de CDRAM) para 4096×221 ocupados. Ahora, con texturas
  diferidas, máximo 2048 de ancho y la textura del tamaño ocupado (2048×512, 4 MB).
- **Música**: el sintetizador de cada canción se crea en un hilo aparte (antes en el del juego: tirón en cada
  cambio de zona); el SoundFont se carga en segundo plano al arrancar. Audio en bloques de 512 muestras
  (10,7 ms) y prioridad alta para que los efectos vayan a tiempo.
- **Archivos**: lo que está en reloaded.pak y no suelto en disco se lee directamente del paquete, sin intentar
  abrir antes el archivo en la tarjeta (las partidas y lo que escribe el juego siguen teniendo prioridad).
