# Kit de compilación para PS Vita (VitaSDK + vitaGL con TEXTURES_SPEEDHACK)

Esta rama (`kit-vitasdk`) solo guarda el kit para compilar `ReloadedGL.vpk` **sin GitHub Actions**.
No tiene código del port: el código está en la rama `port-inicial`.

## Qué contiene
- `vitasdk.tar.xz.part00..02`: VitaSDK completo (carpeta `vitasdk/`, 1,1 GB descomprimido, 224 MB comprimido),
  sacado de la imagen Docker `vitasdk/vitasdk:latest` (amd64, compilada el 2026-08-25), con todas las
  librerías de vdpm que usa la CI.
- Ya trae **vitaGL compilado con `TEXTURES_SPEEDHACK=1`** (commit 2bdbe89, `NO_DEBUG=1`), que es el que
  evita los tirones de ~500 ms (ver `docs/RENDERIZADOR_VITA.md` en `port-inicial`).
- `SHA256SUMS`: sumas de comprobación de las partes.
- `restaurar_kit.sh`: une, comprueba y descomprime.

## Uso (Linux x86-64, p. ej. una sesión nueva de Claude o un PC con Ubuntu)
```bash
# 1. Descargar solo esta rama (no hace falta clonar el resto)
git clone --single-branch --branch kit-vitasdk https://github.com/EMCY3233/PKMN-RLD-Vita.git kit
# 2. Restaurar el kit en una carpeta (por defecto ~/vitasdk)
bash kit/restaurar_kit.sh ~/vitasdk
export VITASDK=~/vitasdk
# 3. Rust nightly con rust-src y cargo-vita (una vez)
rustup toolchain install nightly --profile minimal -c rust-src
cargo install cargo-vita --locked
# 4. Compilar el VPK desde la rama del código
git clone --branch port-inicial https://github.com/EMCY3233/PKMN-RLD-Vita.git
PKMN-RLD-Vita/tools/build_vita_local.sh
# -> runner/target/armv7-sony-vita-newlibeabihf/release/ReloadedGL.vpk
```

## Si esta rama se pierde
El mismo kit se puede volver a sacar de la imagen Docker sin cuenta (ver
`tools/setup_vitasdk.sh` en `port-inicial`: descarga las capas desde `mirror.gcr.io`, que no tiene el
límite de descargas anónimas de Docker Hub, y compila vitaGL con la opción).
