#!/usr/bin/env python3
"""Regenerate first-party fixtures with Python/Pillow; never used at runtime."""
import argparse
import hashlib
import json
import struct
import zlib
from pathlib import Path

import PIL
from PIL import Image, features


def chunk(kind, body):
    return struct.pack(">I", len(body)) + kind + body + struct.pack(">I", zlib.crc32(kind + body))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=Path(__file__).resolve().parent)
    root = parser.parse_args().output
    root.mkdir(parents=True, exist_ok=True)
    generated = []

    def write(name, data):
        (root / name).write_bytes(data)
        generated.append(name)

    def png(name, width, height, depth, color, rows, extras=b"", interlace=0):
        write(name, b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, depth, color, 0, 0, interlace)) + extras + chunk(b"IDAT", zlib.compress(rows)) + chunk(b"IEND", b""))

    data = bytes((i * 37 + i // 4 * 11) % 256 for i in range(13 * 9 * 4))
    Image.frombytes("RGBA", (13, 9), data).save(root / "rgba.png")
    generated.append("rgba.png")
    write("rgba.raw", data)
    png("gray16.png", 3, 1, 16, 0, b"\0\0\x11\x80\xff\xff\0")
    png("palette.png", 4, 1, 2, 3, b"\0\x1b", chunk(b"PLTE", bytes([255, 0, 0, 0, 255, 0, 0, 0, 255, 100, 50, 10])) + chunk(b"tRNS", bytes([255, 128, 0, 200])))
    png("gray-alpha.png", 2, 1, 8, 4, bytes([0, 33, 128, 200, 0]))
    png("gray1.png", 8, 1, 1, 0, bytes([0, 0b10100110]))
    png("rgb-trns.png", 2, 1, 8, 2, bytes([0, 1, 2, 3, 4, 5, 6]), chunk(b"tRNS", struct.pack(">HHH", 1, 2, 3)))
    rgb = lambda x, y: bytes((x * 17 % 256, y * 29 % 256, (x + y) * 11 % 256))
    rows = bytearray()
    for x0, y0, dx, dy in [(0, 0, 8, 8), (4, 0, 8, 8), (0, 4, 4, 8), (2, 0, 4, 4), (0, 2, 2, 4), (1, 0, 2, 2), (0, 1, 1, 2)]:
        for y in range(y0, 9, dy):
            rows.append(0)
            for x in range(x0, 13, dx):
                rows.extend(rgb(x, y))
    png("adam7.png", 13, 9, 8, 2, bytes(rows), interlace=1)
    write("adam7.raw", b"".join(rgb(x, y) + b"\xff" for y in range(9) for x in range(13)))
    source = Image.frombytes("RGB", (37, 23), bytes(c for y in range(23) for x in range(37) for c in (x * 5 % 256, y * 9 % 256, (x * 3 + y * 4) % 256)))

    def jpeg(name, image, **options):
        image.save(root / name, quality=91, **options)
        generated.append(name)
        with Image.open(root / name) as decoded:
            write(name + ".raw", decoded.convert("RGBA").tobytes())

    for mode in ("RGB", "L"):
        for progressive in (False, True):
            jpeg(f"{mode.lower()}-{progressive}.jpg", source.convert(mode), progressive=progressive, subsampling=0)
    Image.new("CMYK", (2, 2), (13, 24, 80, 0)).save(root / "cmyk.jpg")
    generated.append("cmyk.jpg")
    with Image.open(root / "rgb-False.jpg") as base:
        for subsampling in (1, 2):
            jpeg(f"rgb-subsampling{subsampling}.jpg", base, subsampling=subsampling)
        exif = Image.Exif()
        exif[274] = 6
        jpeg("rgb-orientation6.jpg", base, subsampling=0, exif=exif)
    provenance = {"generator": "generate.py", "pillow": PIL.__version__, "libjpeg": features.version("jpg"), "zlib": zlib.ZLIB_VERSION, "sha256": {name: hashlib.sha256((root / name).read_bytes()).hexdigest() for name in sorted(generated)}}
    (root / "provenance.json").write_text(json.dumps(provenance, indent=2) + "\n")


if __name__ == "__main__":
    main()
