#!/usr/bin/env bash
# Prepara el kit de la Vita en Linux x86-64 sin Docker ni GitHub Actions.
#  1) Si se puede, lo restaura de la rama kit-vitasdk de este repositorio (rápido, ya trae vitaGL bueno).
#  2) Si no, lo saca de la imagen Docker vitasdk/vitasdk (capas desde mirror.gcr.io, sin límite de
#     descargas anónimas) y compila vitaGL con TEXTURES_SPEEDHACK.
# Uso: tools/setup_vitasdk.sh [carpeta destino, por defecto ~/vitasdk]
set -euo pipefail
DEST="${1:-$HOME/vitasdk}"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
REMOTE="$(git -C "$ROOT" remote get-url origin)"
TMP="$(mktemp -d)"
if git clone -q --depth 1 --single-branch --branch kit-vitasdk "$REMOTE" "$TMP/kit" 2>/dev/null; then
  bash "$TMP/kit/restaurar_kit.sh" "$DEST"
  rm -rf "$TMP"; exit 0
fi
echo "Rama kit-vitasdk no disponible: se descarga la imagen Docker"
M=https://mirror.gcr.io/v2/vitasdk/vitasdk
IDX=$(curl -sS -H "Accept: application/vnd.oci.image.index.v1+json" $M/manifests/latest)
DIG=$(echo "$IDX" | python3 -c "import sys,json;print([m['digest'] for m in json.load(sys.stdin)['manifests'] if m.get('platform',{}).get('architecture')=='amd64'][0])")
curl -sS -H "Accept: application/vnd.oci.image.manifest.v1+json" $M/manifests/$DIG > "$TMP/man.json"
mkdir -p "$TMP/root"
for d in $(python3 -c "import json;print(' '.join(l['digest'] for l in json.load(open('$TMP/man.json'))['layers']))"); do
  curl -sSL $M/blobs/$d | tar -xz -C "$TMP/root" 2>/dev/null || true
done
mkdir -p "$DEST" && cp -a "$TMP/root/usr/local/vitasdk/." "$DEST/"
export VITASDK="$DEST" PATH="$DEST/bin:$PATH"
git clone -q https://github.com/Rinnegatamante/vitaGL.git "$TMP/vitaGL"
( cd "$TMP/vitaGL" && git checkout -q 2bdbe895ddc6a7e410a9ea55a8f53d8173c61ad8 || true
  make NO_DEBUG=1 TEXTURES_SPEEDHACK=1 -j"$(nproc)" >/dev/null
  cp libvitaGL.a "$DEST/arm-vita-eabi/lib/libvitaGL.a"; cp source/vitaGL.h "$DEST/arm-vita-eabi/include/vitaGL.h" )
rm -rf "$TMP"
echo "Kit listo en $DEST  ->  export VITASDK=$DEST"
