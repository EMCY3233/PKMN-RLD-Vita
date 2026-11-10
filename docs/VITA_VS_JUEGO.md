# PS Vita vs. Pokémon Reloaded 1.9.2.2

## Hardware de la Vita
| Componente | PS Vita | Notas para homebrew |
|---|---|---|
| CPU | ARM Cortex-A9, 4 núcleos | 333 MHz por defecto, 444 MHz con overclock; las apps usan 3 núcleos |
| RAM principal | 512 MB LPDDR2 | App: 256 MB por defecto, **365 MB máx.** (cargo-vita ya lo activa) |
| Heap newlib | — | **128 MB por defecto** → por eso se cortó la 1ª prueba |
| VRAM (CDRAM) | 128 MB | ~112 MB para la app (texturas) |
| GPU | PowerVR SGX543MP4+ | OpenGL ES 2 vía vitaGL; texturas hasta 4096 px; soporta PVRTC/DXT |
| Pantalla | 960×544, 5" | |

## Lo que pide el juego (medido)
| Recurso | Juego | Encaja en Vita |
|---|---|---|
| Carga del .exe (pico RAM) | **303 MB** | ⚠️ Cabe en 365 MB, pero sin margen para jugar |
| Sprites internos | 21 185 fotogramas, **115 MB** RGBA | ❌ |
| Fondos internos | **63 MB** RGBA | ❌ junto con sprites: 178 MB > 112 MB de VRAM |
| Máscaras de colisión | 10 MB | ✅ |
| Sonidos internos | 3,7 MB | ✅ |
| Imagen más grande | 2080 px | ✅ (< 4096) |
| Carpeta data/ (PNG, MIDI, gritos) | 301 MB en disco, se carga bajo demanda | ✅ desde la tarjeta |
| Resolución | 320×240 | ✅ escala ×2 = 640×480 |
| Lógica | 5075 scripts GML interpretados | ⚠️ CPU ~10-15× más lenta que un PC |

## Conclusiones y cambios necesarios en el runner
1. **Heap**: subir `_newlib_heap_size_user` (hecho: 340 MB en loader-test v2).
2. **No duplicar imágenes**: OpenGMK guarda cada sprite en RAM y además en la GPU. En Vita hay que liberar la copia en RAM tras subirla.
3. **Texturas comprimidas o bajo demanda**: 178 MB no caben en 112 MB de VRAM. Opciones: RGBA4444/RGB5A1 (mitad de tamaño),
   PVRTC/DXT (1/4–1/8), o cargar sprites al entrar en cada sala y descargar los que no se usan.
4. **Rendimiento**: overclock a 444 MHz, compilar con LTO y medir FPS por sala; optimizar el intérprete GML si hace falta.
5. **Probado en consola (loader-test v2)**: lee el .exe en 1,9 s y lo **parsea entero en 24,8 s** (PC: 0,97 s → la Vita es ~25× más lenta
   en este trabajo, a 333 MHz y con un solo núcleo). Con 340 MB de heap no hay problemas de memoria.
6. **Tiempo de carga**: 25 s es demasiado para cada arranque. Plan: preprocesar el juego en el PC (en `update_port.sh`) a un paquete
   ya descomprimido y con texturas en formato de GPU, y usar los 3 núcleos + overclock a 444 MHz.
7. **Prueba decisiva pendiente**: velocidad del intérprete GML en la consola (FPS reales por sala).
