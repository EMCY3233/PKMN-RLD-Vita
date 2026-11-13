#!/usr/bin/env python3
"""
rldpak: empaqueta la carpeta data/ de Pokémon Reloaded en un único archivo reloaded.pak.

Por qué: la carpeta tiene ~34.000 archivos pequeños; copiarlos a la PS Vita es lentísimo y
abrir miles de archivos en la tarjeta también. El runner lee del paquete de forma transparente
(los archivos que existan en disco, como partidas o configuración, tienen prioridad).

Formato (little-endian):
    cabecera   8 bytes  b"RLDPAK1\\0"
               u32      número de entradas
               u64      posición del índice
    datos      contenido de cada archivo, uno tras otro, sin comprimir (PNG/MP3 ya lo están)
    índice     por entrada: u16 longitud de ruta, ruta (ASCII/UTF-8, minúsculas, separador '/',
               relativa a la carpeta del juego, p. ej. "data/pkmnbattle/025.png"),
               u64 posición, u32 tamaño

Uso:
    rldpak.py pack <carpeta_del_juego> <salida.pak> [--midi-mp3 <carpeta_mp3>]
    rldpak.py list <archivo.pak>
    rldpak.py extract <archivo.pak> <ruta_interna> <salida>

--midi-mp3: carpeta con los MIDI ya convertidos (misma ruta relativa, extensión .mp3).
            Cada .mid se sustituye por su .mp3 (el juego busca .mp3 si no hay .mid).
"""
import os
import struct
import sys
import unicodedata

MAGIC = b"RLDPAK1\0"
# Archivos que no se empaquetan: herramientas de Windows, basura del sistema, temporales de Office
SKIP_EXT = {".exe", ".dll_backup"}
SKIP_NAMES = {"desktop.ini", "thumbs.db"}
# Datos del jugador: nunca van en el paquete (si se empaqueta una carpeta ya jugada, se quedarían
# partidas o capturas ajenas dentro)
SKIP_DIRS = {"data/partidas", "data/capturas"}


def norm(rel: str) -> str:
    rel = rel.replace("\\", "/").lstrip("./")
    rel = unicodedata.normalize("NFKD", rel).encode("ascii", "ignore").decode()
    return rel.lower()


def collect(game_dir: str, midi_mp3: str | None):
    data_dir = os.path.join(game_dir, "data")
    files = []
    for root, _dirs, names in os.walk(data_dir):
        for n in names:
            low = n.lower()
            if low in SKIP_NAMES or n.startswith("~$") or os.path.splitext(low)[1] in SKIP_EXT:
                continue
            full = os.path.join(root, n)
            rel = os.path.relpath(full, game_dir)
            if any(norm(rel).startswith(d + "/") for d in SKIP_DIRS):
                continue
            if midi_mp3 and low.endswith(".mid"):
                mp3 = os.path.join(midi_mp3, os.path.splitext(rel)[0] + ".mp3")
                if os.path.isfile(mp3) and os.path.getsize(mp3) > 0:
                    files.append((norm(os.path.splitext(rel)[0] + ".mp3"), mp3))
                    continue
            files.append((norm(rel), full))
    files.sort()
    # rutas duplicadas tras normalizar: gana la primera
    seen, out = set(), []
    for k, p in files:
        if k not in seen:
            seen.add(k)
            out.append((k, p))
    return out


def pack(game_dir, out_path, midi_mp3=None):
    files = collect(game_dir, midi_mp3)
    index = []
    with open(out_path + ".tmp", "wb") as f:
        f.write(MAGIC + struct.pack("<IQ", len(files), 0))
        for key, path in files:
            with open(path, "rb") as src:
                data = src.read()
            index.append((key, f.tell(), len(data)))
            f.write(data)
        index_pos = f.tell()
        for key, off, size in index:
            k = key.encode()
            f.write(struct.pack("<H", len(k)) + k + struct.pack("<QI", off, size))
        f.seek(len(MAGIC))
        f.write(struct.pack("<IQ", len(files), index_pos))
    os.replace(out_path + ".tmp", out_path)
    total = sum(s for _, _, s in index)
    print(f"{out_path}: {len(files)} archivos, {total / 1048576:.1f} MB")


def read_index(pak):
    with open(pak, "rb") as f:
        if f.read(8) != MAGIC:
            sys.exit("no es un reloaded.pak")
        count, ipos = struct.unpack("<IQ", f.read(12))
        f.seek(ipos)
        idx = {}
        for _ in range(count):
            (n,) = struct.unpack("<H", f.read(2))
            k = f.read(n).decode()
            off, size = struct.unpack("<QI", f.read(12))
            idx[k] = (off, size)
        return idx


def main():
    a = sys.argv[1:]
    if len(a) >= 3 and a[0] == "pack":
        mp3 = a[a.index("--midi-mp3") + 1] if "--midi-mp3" in a else None
        pack(a[1], a[2], mp3)
    elif len(a) == 2 and a[0] == "list":
        for k, (o, s) in sorted(read_index(a[1]).items()):
            print(f"{s:10} {k}")
    elif len(a) == 4 and a[0] == "extract":
        o, s = read_index(a[1])[norm(a[2])]
        with open(a[1], "rb") as f, open(a[3], "wb") as w:
            f.seek(o)
            w.write(f.read(s))
    else:
        print(__doc__)
        sys.exit(1)


if __name__ == "__main__":
    main()
