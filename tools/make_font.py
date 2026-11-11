#!/usr/bin/env python3
"""Genera vita/font.png: atlas monoespaciado (DejaVu Sans Mono, licencia libre) para los textos
que dibuja el propio runner en la Vita (guía de controles, manual del juego, barra de teclas).
Celdas de 10x20 px, 16 columnas; caracteres Latin-1 32..255 (la celda 127 es un bloque blanco
para fondos). Blanco con alfa = cobertura del glifo."""
import sys
from PIL import Image, ImageDraw, ImageFont

CW, CH, COLS = 10, 20, 16
font = ImageFont.truetype(sys.argv[1] if len(sys.argv) > 1 else "/usr/share/fonts/truetype/dejavu/DejaVuSansMono.ttf", 16)
chars = list(range(32, 256))
rows = (len(chars) + COLS - 1) // COLS
img = Image.new("L", (CW * COLS, CH * rows), 0)
d = ImageDraw.Draw(img)
for i, c in enumerate(chars):
    x, y = (i % COLS) * CW, (i // COLS) * CH
    if c == 127:
        d.rectangle([x, y, x + CW - 1, y + CH - 1], fill=255)
    elif c >= 160 or c < 127:
        d.text((x, y + 1), bytes([c]).decode("latin-1"), font=font, fill=255)
out = Image.new("RGBA", img.size, (255, 255, 255, 0))
out.putalpha(img)
out.save(sys.argv[2] if len(sys.argv) > 2 else "vita/font.png", optimize=True)
print(img.size)
