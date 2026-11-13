# Port de Pokémon Reloaded a PS Vita — plan técnico

## 1. Qué es el juego (análisis de la Beta 1.9.1)
- **Motor: Game Maker 8.0** (runner Delphi; todo el juego va dentro del .exe).
  No es GameMaker Studio, así que UndertaleModTool **no sirve**. El decompilador adecuado es **GM8Decompiler (OpenGMK)**.
- Decompilado con éxito: 3850 sprites, 270 fondos, 62 sonidos, 5046 scripts, 1990 objetos, 716 salas, 21 fuentes.
- **Sin DLLs ni extensiones**, sin `execute_string`, sin registro de Windows → muy buena noticia para portar.
- Código ofuscado: los scripts se llaman por número vía `ex(id, …)` (= `script_execute`). Funciona igual, pero cuesta leerlo.
- Recursos externos en `data/`: sprites PNG cargados con `sprite_add/sprite_replace`, textos cifrados (`*.dll` que en realidad son texto), **323 músicas MIDI**, gritos WAV.
- Resolución base 320×240 / 320×180 (escala con `window_set_region_scale`) → x2 = 640×480 cabe en la pantalla de Vita (960×544).
- Rutas con barra invertida (`"data\partidas\..."`): guardados, capturas, personajes custom.
- Usa ~60 teclas distintas (248 acciones configurables en `DefaultKeyAssignment`) y ratón (`mouse_x/y`, ~240 usos).

## 2. Ruta elegida: runner GM8 nativo para Vita (basado en OpenGMK)
Hay dos caminos posibles:

| | A) Convertir a GameMaker Studio → APK → YoYo Loader Vita | **B) Portar el runner GM8 (OpenGMK gm8emulator) a Vita** |
|---|---|---|
| Cada actualización del juego | Reconvertir 5000+ scripts y re-arreglar a mano | **Copiar el nuevo .exe y listo** |
| Compatibilidad GML | GM8 ≠ GMS (`sound_replace`, `game_load`, rutas…) | Reimplementación fiel de GM8 |
| Licencias | GMS 1.4 descatalogado + export Android | Todo open source (GPL-2) |
| Trabajo inicial | Medio | Alto (una vez) |

Como el juego **sigue actualizándose**, la opción **B** es la correcta: el trabajo se hace una vez en el runner y cada versión nueva se ejecuta tal cual.
La opción A queda como plan de respaldo (el `.gmk` decompilado sirve para ella).

### Trabajo del runner (OpenGMK → Vita)
1. Toolchain: Rust `armv7-sony-vita-newlibeabihf` + `cargo-vita` + VitaSDK.
2. Ventana/entrada: sustituir `ramen` por SDL2 (VitaSDK) + capa de controles `vita/keymap.ini`.
3. Gráficos: OpenGL 3.3 → **vitaGL** (shaders GLSL 330 reescritos a GLSL ES / CG). Atlas de texturas ≤ 4096 px.
4. Audio: backend SDL2 para el mezclador `udon`; MP3 con `rmp3`.
5. Sistema de archivos: traducir `\` → `/` y raíz a `ux0:data/reloaded/`.
6. Quitar imgui/clipboard/TAS (no hacen falta). Teclado del sistema (IME) para `keyboard_string`.
7. Rendimiento (CPU ARM 444 MHz): compilar en release con LTO, overclock a 444 MHz vía `scePowerSetArmClockFrequency`, perfilar el intérprete GML.
8. Empaquetar VPK (LiveArea con icono/fondo).

## 3. Música
GM8 en Vita no puede reproducir MIDI, pero **el juego busca `.mid` y si no existe usa `.mp3` automáticamente** (`ImportMusica`).
El script convierte los MIDI a MP3 con FluidSynth + soundfont GM: sin tocar el código. (Probado: ~4 s por pista.)

## 4. Controles (resumen) — ver `vita/keymap.ini`
| Vita | Normal | Mantener L | Mantener R |
|---|---|---|---|
| ✕ | Z (aceptar) | S (cambiar) | A (andar/asignar) |
| ○ | X (cancelar) | E (huir) | O (mochila) |
| □ | C (pescar/ordenar) | B (Ball) | F (buscar/flauta) |
| △ | V (bici/ver) | D (dar/cancelar cambio) | M (mapa/Mega) |
| Cruceta/stick | Flechas | RePág/AvPág/Inicio/Fin | P / K / Y / H |
| START / SELECT | Enter / Espacio | | |
| L / R (solos) | Shift (ver ataques) / Ctrl (acelerar) | | |
| Táctil frontal | Ratón (toque = clic) · Panel trasero: teclas 1-4 · SELECT+△: teclado | | |

## 5. Flujo de actualización (cada versión nueva)
```bash
./update_port.sh Proyecto_Reloaded_<version>.zip <version_anterior>
```
Hace: extraer → decompilar → volcar GML → diff con la versión anterior → convertir MIDI nuevos (con caché) → informe `REPORTE_COMPATIBILIDAD.txt` (alerta si aparecen DLLs, funciones no soportadas o **cambios en las teclas**).
Después: copiar `vita_data/` y `game.exe` a `ux0:data/reloaded/`. El VPK del runner no cambia.

Checklist de revisión por versión: ☐ informe sin funciones nuevas peligrosas ☐ teclas sin cambios (o actualizar keymap) ☐ arrancar, partida nueva, guardar/cargar, combate, música, Pokédex.

## 6. Estado
- [x] Identificación del motor y decompilación (.gmk + código GML en texto)
- [x] Auditoría de compatibilidad
- [x] Conversión de música verificada
- [x] Mapa de controles y script de actualización
- [ ] Runner OpenGMK para Vita (requiere VitaSDK y pruebas en consola/emulador Vita3K)

Nota: publica el port solo con permiso del autor del fan-game; no incluyas recursos de Nintendo/Game Freak en el VPK, que el usuario copie su propio `data/`.
