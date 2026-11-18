#!/usr/bin/env python3
"""Unpack Noita's data/data.wak (vanilla scripts, XML, images) for reading.

Format: 16-byte header (u32 0, u32 file_count, u32 data_start, u32 0), then file_count
entries of (u32 offset, u32 size, u32 name_len, name bytes). Offsets are absolute.

Usage: tools/unpack_wak.py [WAK] [OUT_DIR]
Defaults: the Steam install path and .cache/noita-data (gitignored).
"""
import os
import struct
import sys

DEFAULT_WAK = "/data/Games/steamapps/common/Noita/data/data.wak"
DEFAULT_OUT = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), ".cache", "noita-data")


def unpack(wak_path: str, out_dir: str) -> int:
    with open(wak_path, "rb") as f:
        blob = f.read()
    _, count, _, _ = struct.unpack_from("<4I", blob, 0)
    pos = 16
    root = os.path.realpath(out_dir)
    for _ in range(count):
        offset, size, name_len = struct.unpack_from("<3I", blob, pos)
        pos += 12
        name = blob[pos : pos + name_len].decode("utf-8")
        pos += name_len
        target = os.path.realpath(os.path.join(root, name))
        if not target.startswith(root + os.sep):
            raise ValueError(f"entry escapes output dir: {name}")
        os.makedirs(os.path.dirname(target), exist_ok=True)
        with open(target, "wb") as out:
            out.write(blob[offset : offset + size])
    return count


if __name__ == "__main__":
    wak = sys.argv[1] if len(sys.argv) > 1 else DEFAULT_WAK
    out = sys.argv[2] if len(sys.argv) > 2 else DEFAULT_OUT
    print(f"unpacked {unpack(wak, out)} files into {out}")
