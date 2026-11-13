# Pokémon Reloaded → PS Vita

Port del fan-game **Pokémon Reloaded** (Game Maker 8) a PS Vita usando un runner GM8 de código abierto
basado en [OpenGMK](https://github.com/OpenGMK/OpenGMK), con controles adaptados a la consola.

> Este repositorio **no contiene el juego**. Descarga la última versión en pokemon-reloaded.blogspot.com.

## Estado
| Paso | Estado |
|---|---|
| Decompilación y análisis (GM8Decompiler) | ✅ |
| Runner OpenGMK ejecuta el juego (Linux): menú → partida nueva → mapa | ✅ (1.9.2.2) |
| Mapa de controles Vita (`vita/keymap.ini`) | ✅ diseñado |
| Cadena de compilación Vita (GitHub Actions + VitaSDK) | 🔄 `vita/loader-test` |
| Runner en Vita (SDL2 + vitaGL + audio) | ⏳ |

## Estructura
- `runner/` — OpenGMK con los parches para Reloaded (GPL-2.0, ver `runner/LICENCE.md`)
- `vita/loader-test/` — VPK de prueba: lee `ux0:data/reloaded/game.exe` en la consola y escribe `loader_test.log`
- `vita/keymap.ini` — botones de Vita → teclas del juego
- `tools/gmldump/` — vuelca el código GML a texto para comparar versiones
- `update_port.sh` — flujo para cada versión nueva (decompilar, comparar, convertir MIDI→MP3, informe)
- `docs/` — plan técnico y notas del runner

## Probar el VPK de prueba
1. Descarga el artefacto `reloaded-loader-test-vpk` de la pestaña **Actions** e instálalo con VitaShell.
2. Copia el `.exe` del juego a `ux0:data/reloaded/game.exe`.
3. Abre la app y mira `ux0:data/reloaded/loader_test.log`.
