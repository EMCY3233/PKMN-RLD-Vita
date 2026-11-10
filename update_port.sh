#!/usr/bin/env bash
# Pipeline de actualización del port PS Vita de Pokémon Reloaded.
# Uso: ./update_port.sh <Proyecto_Reloaded_X.zip> [version_anterior]
# Requisitos: rust/cargo, git, unzip, fluidsynth, ffmpeg, una soundfont GM (.sf2)
set -euo pipefail
ZIP="$1"; PREV="${2:-}"
ROOT="$(cd "$(dirname "$0")" && pwd)"
SF2="${SF2:-/usr/share/sounds/sf2/FluidR3_GM.sf2}"

(cd "$ROOT/runner" && cargo build --release -p gm8decompiler)
(cd "$ROOT/tools/gmldump" && cargo build --release)

VER=$(basename "$ZIP" .zip | grep -oE '[0-9]+(_[0-9]+)+' | tr _ . || date +%Y%m%d)
W="$ROOT/versions/$VER"; mkdir -p "$W"
echo "== [1/5] Extrayendo $ZIP -> $W/game"
unzip -q -o "$ZIP" -d "$W/game"
EXE=$(find "$W/game" -maxdepth 2 -iname '*.exe' -size +5M | head -1)

echo "== [2/5] Decompilando (GM8Decompiler)"
"$ROOT/runner/target/release/gm8decompiler" "$EXE" -o "$W/reloaded.gmk"

echo "== [3/5] Volcando código GML"
rm -rf "$W/gml"; "$ROOT/tools/gmldump/target/release/gmldump" "$EXE" "$W/gml"

echo "== [4/5] Convirtiendo MIDI -> MP3 (el juego usa .mp3 si no hay .mid)"
OUT="$W/vita_data"; rm -rf "$OUT"; cp -r "$W/game/data" "$OUT"
find "$OUT/audio/musica" -iname '*.mid' -print0 | while IFS= read -r -d '' m; do
  mp3="${m%.*}.mp3"; cache="$ROOT/cache/midi/$(md5sum "$m" | cut -c1-32).mp3"
  if [ ! -f "$cache" ]; then mkdir -p "$(dirname "$cache")"
    fluidsynth -ni -q -F /tmp/_m.wav -r 44100 "$SF2" "$m" >/dev/null 2>&1
    ffmpeg -y -loglevel error -i /tmp/_m.wav -ac 2 -b:a 128k "$cache"; fi
  cp "$cache" "$mp3"; rm "$m"
done
# Herramientas de Windows incluidas que no sirven en Vita
find "$OUT" -iname '*.exe' -delete; find "$OUT" -iname 'desktop.ini' -delete
cp "$EXE" "$OUT/../game.exe"

echo "== [5/5] Informe de compatibilidad"
R="$W/REPORTE_COMPATIBILIDAD.txt"
{ echo "Versión: $VER"; cat "$W/gml/summary.txt" | head -2
  for f in execute_string execute_file external_define external_call registry_ game_save game_load \
           window_set_fullscreen display_set_size screen_save sound_add mci_ cd_ splash_ show_message get_string \
           keyboard_string file_find_first d3d_; do
    printf "%-22s %6s\n" "$f" "$(grep -rhoE "\b$f" "$W/gml" | wc -l)"; done
  echo; echo "Extensiones/DLL:"; grep -E "EXTENSION|INCLUDED" "$W/gml/summary.txt" || echo "  ninguna (OK)"
} > "$R"
if [ -n "$PREV" ] && [ -d "$ROOT/versions/$PREV/gml" ]; then
  echo; echo "== Cambios respecto a $PREV" | tee -a "$R"
  diff -rq "$ROOT/versions/$PREV/gml" "$W/gml" | sed 's|'"$ROOT"'/versions/||g' | tee "$W/CAMBIOS.txt" | wc -l | xargs echo "Archivos cambiados:" | tee -a "$R"
  diff <(sed 's/;/;\n/g' "$ROOT/versions/$PREV/gml/scripts/DefaultKeyAssignment.gml") \
       <(sed 's/;/;\n/g' "$W/gml/scripts/DefaultKeyAssignment.gml") > "$W/CAMBIOS_TECLAS.diff" \
    && echo "Teclas: sin cambios" | tee -a "$R" || echo "¡ATENCIÓN! Cambios en teclas -> revisar vita/keymap.ini (CAMBIOS_TECLAS.diff)" | tee -a "$R"
fi
echo "Listo. Copia $OUT y game.exe a ux0:data/reloaded/ en la Vita."
