# Compilar y probar sin GitHub Actions

GitHub Actions en un repositorio privado tiene 2.000 minutos al mes en el plan gratuito; cada VPK son ~7.
Todo lo necesario para compilar fuera de Actions está guardado en el repositorio.

## Dónde está cada cosa
| Qué | Dónde |
|---|---|
| Código del port | rama `port-inicial` |
| Kit de la Vita (VitaSDK + vitaGL con TEXTURES_SPEEDHACK), 224 MB en 3 partes | rama `kit-vitasdk` (`LEEME.md`, `restaurar_kit.sh`) |
| Preparar el kit (rama o, si falta, imagen Docker por mirror.gcr.io) | `tools/setup_vitasdk.sh` |
| Compilar `ReloadedGL.vpk` (mismos pasos que la CI) | `tools/build_vita_local.sh` |
| Recorrido automático por las 680 salas | `tools/pruebas/recorrido_salas.py` (+ `tour_spawns.txt`, teclas) |
| Resultados del recorrido y scripts pesadas | `tools/pruebas/resultados/` |
| Hallazgos de rendimiento y gráficos | `docs/RENDERIZADOR_VITA.md` |

## Pasos (Linux x86-64)
```bash
tools/setup_vitasdk.sh ~/vitasdk          # una vez por máquina/sesión
export VITASDK=~/vitasdk
rustup toolchain install nightly --profile minimal -c rust-src
cargo install cargo-vita --locked
tools/build_vita_local.sh                  # -> runner/target/armv7-sony-vita-newlibeabihf/release/ReloadedGL.vpk
```
Para instalar: descomprimir el VPK (es un zip) e instalar la carpeta con VitaShell («Install folder»).

## Pruebas en PC (runner Linux)
`cargo build --release` en `runner/`. Variables útiles: `RLD_LAZY_TEX=1` (texturas como en la Vita),
`RLD_SIMPLE_GL=1`, `RLD_SEED=7 RLD_MANUAL=1 RLD_KEYS=...` (partida reproducible), `RLD_SHOTS_P=f1,f2`
(capturas para comparar píxel a píxel), `RLD_PROFILE=1`, `RLD_PERF=1`, `RLD_HITCH_MS=8` (tirones en PC),
`RLD_DEBUG_COMPILE=1`, `RLD_DEBUG_HOLDERS=1`, `RLD_SELFTEST=1` (aceleradores contra el GML original),
`RLD_NO_ACCEL=1`. Hace falta un servidor X (`Xvfb :77`) y el juego (`game.exe`, `reloaded.pak` y la partida),
que no están en el repositorio.

Recorrido por salas: `RLD_TOUR_DIR=<carpeta del juego con keys.txt y tour_spawns.txt> python3 tools/pruebas/recorrido_salas.py 0,1,2,...`
