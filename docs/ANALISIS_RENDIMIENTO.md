# Análisis de rendimiento: Pokémon Reloaded 1.9.2.2 en PS Vita

Fecha: 10/11/2026. Fuentes: código decompilado del juego, perfilado del runner (`RLD_PROFILE=1`),
benchmarks en PS Vita (CPU a 444 MHz, sin gráficos) y documentación de VitaSDK/vitaGL.

## 1. Parámetros del juego original

| Parámetro | Valor |
|---|---|
| Motor | Game Maker 8.0 (runner Delphi), sin DLLs ni extensiones |
| Velocidad de sala | 30 fotogramas por segundo (todas las salas medidas) |
| Resolución lógica | 320×240 / 320×180 (vistas), escalada por el runner de Windows |
| Recursos | 3857 sprites (21 185 fotogramas), 270 fondos, 62 sonidos, 21 fuentes |
| Código | 5075 scripts, 1991 objetos, 717 salas; todos los eventos llaman a scripts por nombre |
| Eventos | 1348 create, 1649 alarm, 1126 step (1078 normales), 416 draw, 107 colisión |
| Datos externos (`data/`) | 34 354 archivos / 198 MB: 26 239 sprites `.gmspr`, 323 MIDI, textos cifrados |
| Aleatoriedad | ~6800 usos de `random`/`choose` y 10 de `current_time` → ejecución no determinista |
| Ofuscación | 114 010 llamadas `ex(N, …)` (despachador a `script_execute`) |
| Llamadas más frecuentes | `instance_create` (16 085 en código), `with` (6597), `instance_number/find` (4561), `sleep` (1589), `screen_redraw` (1971) |

## 2. Qué hacía lento el juego (y estado)

Medido en el mismo recorrido del benchmark (intro → título → selección → charla → casa → menús).
Tiempo total en PC: **9,4 s → 3,9 s (×2,4)**. Casa del protagonista: **811 → 1680 FPS (×2,07)**.

| # | Causa | Coste antes | Solución | Estado |
|---|---|---|---|---|
| 1 | `sprite_add_sprite` sin implementar en OpenGMK: `ImportBattlePokemon` reintentaba la carga y recorría en GML su caché de 1000 huecos | 45 % del tiempo total; además no se veían los sprites de Pokémon | Implementado el formato `.gmspr` (zlib + fotogramas BGRA + máscaras) | ✅ |
| 2 | `TextReader`: relee `txt_dialog.dll` (1,5 MB) y salta líneas una a una en GML para cada texto | ~100 ms por texto en Vita; pausas de 5 s | Caché de textos + índice de líneas nativo (`rld_text_line`) | ✅ 0 diferencias en 45 090 líneas |
| 3 | 52 scripts que son tablas escritas como cientos de `if(argument0=N)return(M);` (Pokédex, pesos, ataques…) | `GetIndexOrdenObj` 17 % | Tablas nativas (`rld_lookup`), mismo parseo de números y mismos errores que GM8 | ✅ 23 546 casos idénticos |
| 4 | Despachador de la ofuscación `ex(N, …)` → `script_execute` | ~16 % (586 000 llamadas) | El compilador convierte `ex(N, …)` en llamada directa | ✅ |
| 5 | `object_event_add/clear` recalculaban las listas de eventos de 1991 objetos con búsquedas cuadráticas | 39 ms por llamada (≈1 s en Vita) | Deduplicación lineal + HashSet; no recalcular si no cambia nada | ✅ 11 ms |
| 6 | Compilar las 5075 scripts al arrancar | 200 MB de RAM | Compilación diferida | ✅ |
| 7 | Leer 34 000 archivos sueltos de la tarjeta | Instalación lentísima | `reloaded.pak` | ✅ |

Todos los aceleradores se activan **solo si el código del juego coincide exactamente** con el esperado;
si una actualización lo cambia, se usa el original (más lento, nunca incorrecto). `RLD_NO_ACCEL=1` los
desactiva y `RLD_SELFTEST=1` compara ambas versiones.

### Pendiente (por orden de impacto)
| Causa | Coste actual (PC) | Idea |
|---|---|---|
| `sound_play` | ~1 ms por llamada (decodifica el sonido cada vez) | Cachear el audio ya decodificado; mezcla en otro núcleo |
| `getBigTrainerOffsets`, `BerriesManagement` | 31 y 38 µs por llamada, cada fotograma | Revisar si son tablas/búsquedas lineales acelerables |
| `object_event_add` | 11 ms × 19 | Actualización incremental de listas |
| `CargarStringsUtiles`, `ControlBaseInitialize` | 100 ms / 0,6 ms (una vez por partida/sala) | Aceptable; revisar si es E/S |
| Carga inicial | 25 s en Vita (descomprimir el .exe) | Preprocesar el .exe en el PC a un formato listo para la Vita |
| Intérprete GML en general | — | Caché de búsqueda de variables, evitar copias de `Value` |

## 3. Cómo aprovechar el hardware de la PS Vita

| Recurso | Situación | Propuesta |
|---|---|---|
| **CPU** Cortex-A9, 4 núcleos (3 para apps) | GML corre en 1 núcleo; ya a 444 MHz | Núcleo 0: lógica GML. Núcleo 1: audio (mezcla + sintetizador MIDI). Núcleo 2: carga/decodificación de imágenes y lectura de `reloaded.pak` por adelantado |
| **GPU** SGX543MP4+ vía vitaGL | Aún sin renderizador | Dibujar a 320×240 en un framebuffer y escalarlo en la GPU (×2 = 640×480 nítido, o 725×544 a pantalla completa con filtro "sharp bilinear"). Sprites agrupados en atlas → pocas llamadas de dibujo |
| **VRAM (CDRAM)** ~112 MB | Gráficos internos 178 MB en RGBA | Texturas en CDRAM, cargadas por sala y liberadas al salir; sin copia en RAM. No usar compresión con pérdida (ETC1/PVRTC) en pixel art; RGBA8888 o 5551 para sprites sin semitransparencia |
| **RAM** 365 MB máx. | Pico 171 MB | Margen para caché de sprites `.gmspr` y audio |
| **Pantalla** 960×544 a 60 Hz | Juego a 30 FPS | Presentar cada 2 vblanks (`sceDisplayWaitVblankStart`): ritmo estable y menos consumo |
| **Audio**: `sceAudiodec` decodifica MP3/AAC/AT9 por hardware | MIDI sin soporte | Opción A: sintetizador MIDI ligero (los 323 MIDI ocupan ~6 MB) en el núcleo 1. Opción B: MP3 de bajo bitrate decodificado por hardware (más espacio, CPU ~0) |
| **Almacenamiento** | `reloaded.pak` | Lectura anticipada en segundo plano; índice ya en memoria |
| **Controles** | Mapa en `vita/keymap.ini` | Táctil frontal = ratón; panel trasero = atajos; stick = cruceta |

## 4. Proyección y plan

- Lógica en la casa: **35 FPS medidos → ~70 FPS estimados** con las optimizaciones de este informe
  (pendiente de confirmar en consola). El objetivo de 30 FPS deja ~50 % del tiempo de cada fotograma
  para dibujar.
- Orden de trabajo propuesto:
  1. Confirmar en Vita el benchmark con los aceleradores.
  2. Renderizador vitaGL (sprites, fondos, texto, escalado) → primera versión jugable con imagen.
  3. Controles (keymap.ini) y táctil.
  4. Audio: efectos (WAV) + música (MIDI sintetizado en el núcleo 1).
  5. Carga rápida (preprocesado del .exe) y optimizaciones pendientes de la tabla anterior.

## Fuentes
- [VitaSDK – SceAudiodec](https://docs.vitasdk.org/group__SceAudiodec.html) (decodificación MP3/AAC/Atrac por hardware)
- [vitaGL](https://github.com/TheOfficialFloW/vitaGL) (OpenGL acelerado por hardware en Vita; ETC1 acelerado)
- [PS Vita Dev Wiki – CPU](https://psdevwiki.com/vita/CPU)
- [Vita Rust Book – Memoria](https://vita-rust.github.io/book/dev/mem.html)

## 5. Auditoría completa (10/11/2026, tarde)

Revisión línea a línea de los ~3100 cambios sobre OpenGMK. Fallos encontrados y corregidos:

| Gravedad | Fallo | Corrección | Verificación |
|---|---|---|---|
| Alta | Sprites `.gmspr` (todos los Pokémon) con rojo y azul invertidos: el archivo guarda BGRA y el renderizador espera RGBA | Conversión BGRA→RGBA | Captura: Eevee, Pikachu y Riolu con sus colores |
| Alta | `reloaded.pak` entregado contenía `data/partidas/reloaded1.data`, una partida creada por un benchmark | Paquete regenerado desde el zip original; `rldpak.py` excluye `data/partidas` y `data/capturas` | `rldpak list`: 0 archivos de partidas |
| Media | En Vita, `working_directory` podría devolverse como `ux0:/…` y no coincidir con la ruta del paquete (sprites de Pokémon no encontrados) | Normalización `ux0:/` = `ux0:`; diagnóstico de arranque en `runner.log` | Prueba unitaria + diagnóstico |
| Media | Borrar un archivo que viene en el paquete (p. ej. `config.ini`) no lo hacía desaparecer para el juego | Registro persistente de borrados (`rld_borrados.txt`); escribir lo restaura | Prueba de borrado/escritura/reinicio |
| Media | Reescritura de tablas: aceptaba `return(5)+1;`, separaba un `else` y podía actuar dentro de cadenas/comentarios | Terminador obligatorio, rechazo de `else`, salto de cadenas y comentarios | 5 pruebas unitarias; 52 scripts / 23 546 casos idénticos |
| Baja | `sprite_add_sprite`: desbordamiento con archivos corruptos, fotogramas de distinto tamaño, máscara precisa con caja manual distinta de `sprite_collision_mask` | Comprobaciones y mismo criterio | — |
| Baja | `update_port.sh` convertía MIDI a MP3 (~500 MB) sin usar el resultado; el informe no detectaba funciones sin implementar | Script reescrito: informe de funciones sin implementar y de aceleradores | Ejecutado sobre 1.9.2.2 |
| Baja | `file_copy`/`file_rename` no funcionaban con archivos que solo están en el paquete | Copia/renombrado desde el paquete | — |

Revisado sin fallos: funciones implementadas (`room_set_code`, `sound_replace` con MIDI, teclas simuladas,
`keyboard_string`), compilación diferida, despachador `ex` (rellena los 15 argumentos), listas de eventos,
pila de 16 MB, memoria. Ninguna función/variable sin implementar usada por el juego. Única función que lee
píxeles: `screen_save` (capturas de pantalla), sin efecto en la lógica.

Pendiente de confirmar en consola (lo dirá `runner.log`): rutas reales de la Vita y escritura de partidas.
Recorrido final: salas y fotogramas idénticos con y sin aceleradores; versión de PC con gráficos sin errores.
