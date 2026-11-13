#!/usr/bin/env bash
# Compila ReloadedGL.vpk sin GitHub Actions (mismos pasos que el trabajo vita-runner-gl de la CI).
# Requiere: VITASDK apuntando a un VitaSDK (p. ej. extraído de la imagen vitasdk/vitasdk), Rust nightly
# con rust-src y cargo-vita. vitaGL con TEXTURES_SPEEDHACK ya instalado en $VITASDK (ver build.yml).
set -euo pipefail
: "${VITASDK:?define VITASDK}"
export PATH="$VITASDK/bin:$PATH"
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT/runner/gm8emulator"
export RUSTUP_TOOLCHAIN=nightly
export RUSTFLAGS="-Zcrate-attr=feature(stdarch_arm_neon_intrinsics)"
export CARGO_PROFILE_RELEASE_STRIP=false
if ! cargo vita build vpk -- --release --no-default-features --features vitagl 2>&1 | tee /tmp/rld_vita_build.log; then
  if grep -q "for SCE data at end of segment" /tmp/rld_vita_build.log; then
    RLD_ELF_PAD=1 cargo vita build vpk -- --release --no-default-features --features vitagl
  else
    exit 1
  fi
fi
cd "$ROOT/runner/target/armv7-sony-vita-newlibeabihf/release"
vita-mksfoex -s TITLE_ID=PKRLD0010 -d ATTRIBUTE2=12 "Pokemon Reloaded" gl.sfo
LA="$ROOT/vita/livearea"
vita-pack-vpk -s gl.sfo -b gm8emulator.self \
  -a "$LA/icon0.png=sce_sys/icon0.png" \
  -a "$LA/bg0.png=sce_sys/livearea/contents/bg0.png" \
  -a "$LA/startup.png=sce_sys/livearea/contents/startup.png" \
  -a "$LA/template.xml=sce_sys/livearea/contents/template.xml" \
  -a "$ROOT/vita/soundfont.sf2=soundfont.sf2" \
  ReloadedGL.vpk
rm -f gm8emulator.vpk
echo "VPK: $PWD/ReloadedGL.vpk"
