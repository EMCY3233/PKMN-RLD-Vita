#!/usr/bin/env bash
# Une las partes, comprueba las sumas y descomprime el kit en la carpeta indicada (por defecto ~/vitasdk).
set -euo pipefail
DIR="$(cd "$(dirname "$0")" && pwd)"
DEST="${1:-$HOME/vitasdk}"
cd "$DIR"
sha256sum -c SHA256SUMS
mkdir -p "$DEST"
cat vitasdk.tar.xz.part* | xz -d | tar -x -C "$DEST" --strip-components=1
"$DEST/bin/arm-vita-eabi-gcc" --version | head -1
echo "Kit listo en $DEST  ->  export VITASDK=$DEST"
