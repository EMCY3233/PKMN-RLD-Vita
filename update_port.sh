#!/usr/bin/env bash
# Flujo de actualización del port PS Vita de Pokémon Reloaded.
# Uso: ./update_port.sh <Proyecto_Reloaded_X.zip> [version_anterior]
# Requisitos: rust/cargo, unzip, zip, python3
# Resultado: versions/<ver>/Reloaded_Vita_<ver>.zip (game.exe + reloaded.pak) e informe de compatibilidad.
set -euo pipefail
ZIP="$1"; PREV="${2:-}"
ROOT="$(cd "$(dirname "$0")" && pwd)"

(cd "$ROOT/runner" && cargo build --release -p gm8decompiler)
(cd "$ROOT/tools/gmldump" && cargo build --release)

VER=$(basename "$ZIP" .zip | grep -oE '[0-9]+(_[0-9]+)+' | tr _ . || true)
[ -n "$VER" ] || VER=$(date +%Y%m%d)
W="$ROOT/versions/$VER"; rm -rf "$W"; mkdir -p "$W"

echo "== [1/5] Extrayendo $ZIP (siempre desde el zip original: nunca desde una carpeta ya jugada)"
unzip -q -o "$ZIP" -d "$W/game"
EXE=$(find "$W/game" -maxdepth 2 -iname '*.exe' -size +5M | head -1)
[ -n "$EXE" ] || { echo "No encuentro el .exe del juego"; exit 1; }

echo "== [2/5] Decompilando (GM8Decompiler)"
"$ROOT/runner/target/release/gm8decompiler" "$EXE" -o "$W/reloaded.gmk"

echo "== [3/5] Volcando código GML"
"$ROOT/tools/gmldump/target/release/gmldump" "$EXE" "$W/gml"

echo "== [4/5] Informe de compatibilidad"
R="$W/REPORTE_COMPATIBILIDAD.txt"
K="$ROOT/runner/gm8emulator/src/gml/kernel.rs"
{
  echo "Versión: $VER"; head -2 "$W/gml/summary.txt"
  echo
  echo "Extensiones/DLL:"; grep -E "EXTENSION|INCLUDED" "$W/gml/summary.txt" || echo "  ninguna (OK)"
  echo
  echo "Funciones de GM8 que usa el juego y el runner NO implementa (deben ser 0):"
  grep -oE "unimplemented kernel function [a-z0-9_]+" "$K" | awk '{print $4}' | sort -u > "$W/.unimpl"
  grep -rhoE "\b[a-z_][a-z0-9_]*\(" "$W/gml" | tr -d '(' | sort | uniq -c > "$W/.used"
  awk 'NR==FNR{u[$1]=1;next} ($2 in u){print "  ", $2, "(" $1 " usos)"; n++} END{if(!n) print "   ninguna (OK)"}' "$W/.unimpl" "$W/.used"
  echo
  echo "Aceleradores del port (si alguno dice CAMBIADO, funciona igual pero más lento: revisar game/accel.rs):"
  EX_OK='return script_execute(argument0,argument1,argument2,argument3,argument4,argument5,argument6,argument7,argument8,argument9,argument10,argument11,argument12,argument13,argument14,argument15);'
  if [ "$(tr -d '\r\n' < "$W/gml/scripts/ex.gml" | sed 's/^ *//;s/ *$//')" = "$EX_OK" ]; then echo "   ex (despachador): OK"; else echo "   ex (despachador): CAMBIADO"; fi
  if grep -qF 'file_text_readln(f); } txt=txt+file_text_read_string(f); file_text_close(f);' "$W/gml/scripts/TextReader.gml"; then echo "   TextReader: OK"; else echo "   TextReader: CAMBIADO"; fi
  echo "   Tablas if(argument0=N)return(M): $(grep -lE 'if\(argument0=[0-9]+\)return' "$W/gml/scripts/"*.gml | wc -l) scripts candidatas"
} > "$R"
rm -f "$W/.unimpl" "$W/.used"
if [ -n "$PREV" ] && [ -d "$ROOT/versions/$PREV/gml" ]; then
  {
    echo; echo "Cambios respecto a $PREV:"
    diff -rq "$ROOT/versions/$PREV/gml" "$W/gml" > "$W/CAMBIOS.txt" || true
    echo "   archivos de código cambiados: $(wc -l < "$W/CAMBIOS.txt") (lista en CAMBIOS.txt)"
    if diff <(sed 's/;/;\n/g' "$ROOT/versions/$PREV/gml/scripts/DefaultKeyAssignment.gml" | sed 's/^ *//') \
            <(sed 's/;/;\n/g' "$W/gml/scripts/DefaultKeyAssignment.gml" | sed 's/^ *//') > "$W/CAMBIOS_TECLAS.diff"; then
      echo "   teclas: sin cambios"
    else
      echo "   ¡ATENCIÓN! teclas cambiadas -> revisar vita/keymap.ini (CAMBIOS_TECLAS.diff)"
    fi
  } >> "$R"
fi
cat "$R"

echo "== [5/5] Empaquetando data/ en reloaded.pak y creando el zip para la Vita"
mkdir -p "$W/vita"
python3 -I "$ROOT/tools/rldpak.py" pack "$W/game" "$W/vita/reloaded.pak"
cp "$EXE" "$W/vita/game.exe"
cat > "$W/vita/LEEME_VITA.txt" <<EOF
Pokemon Reloaded $VER - preparado para PS Vita
Copiar game.exe y reloaded.pak a ux0:data/reloaded/
Las partidas se guardan aparte (ux0:data/reloaded/data/partidas/): al actualizar
basta con sustituir game.exe y reloaded.pak.
EOF
(cd "$W/vita" && rm -f "../Reloaded_Vita_$VER.zip" \
  && zip -q -0 "../Reloaded_Vita_$VER.zip" reloaded.pak \
  && zip -q -9 "../Reloaded_Vita_$VER.zip" game.exe LEEME_VITA.txt)
echo "Listo: $W/Reloaded_Vita_$VER.zip"
