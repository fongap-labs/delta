"""Reject dynamic Visual C++ runtime imports in Delta Windows portable executables.

The build runner has the MSVC redistributable installed, so executing a binary on
that runner does not prove the archive will start on a clean Windows machine.
Inspect regular PE imports and delay-load imports without third-party tools.
"""

from __future__ import annotations

import argparse
import re
import struct
from pathlib import Path


class InvalidPeError(ValueError):
    """The file is not a readable PE image with valid import metadata."""


def _imports(path: Path) -> set[str]:
    data = path.read_bytes()

    def unpack(fmt: str, offset: int) -> tuple:
        size = struct.calcsize(fmt)
        if offset < 0 or offset + size > len(data):
            raise InvalidPeError(f"{path}: truncated PE at file offset {offset}")
        return struct.unpack_from(fmt, data, offset)

    if data[:2] != b"MZ":
        raise InvalidPeError(f"{path}: missing MZ signature")
    (pe_offset,) = unpack("<I", 0x3C)
    if unpack("<4s", pe_offset)[0] != b"PE\x00\x00":
        raise InvalidPeError(f"{path}: missing PE signature")
    coff = pe_offset + 4
    num_sections = unpack("<H", coff + 2)[0]
    opt_size = unpack("<H", coff + 16)[0]
    opt = coff + 20
    magic = unpack("<H", opt)[0]
    if magic == 0x20B:
        directories = opt + 112
        number_dirs = unpack("<I", opt + 108)[0]
        image_base = unpack("<Q", opt + 24)[0]
    elif magic == 0x10B:
        directories = opt + 96
        number_dirs = unpack("<I", opt + 92)[0]
        image_base = unpack("<I", opt + 28)[0]
    else:
        raise InvalidPeError(f"{path}: unsupported PE optional header magic: 0x{magic:x}")
    if (directories - opt) + min(number_dirs, 16) * 8 > opt_size:
        raise InvalidPeError(f"{path}: optional header data directory exceeds its size")

    sections = []
    for i in range(num_sections):
        sec = opt + opt_size + i * 40
        virtual_size = unpack("<I", sec + 8)[0]
        virtual_address = unpack("<I", sec + 12)[0]
        raw_size = unpack("<I", sec + 16)[0]
        raw_offset = unpack("<I", sec + 20)[0]
        sections.append((virtual_address, max(virtual_size, raw_size), raw_size, raw_offset))

    def file_offset(rva: int) -> int:
        for start, mapped_size, raw_size, raw_offset in sections:
            delta = rva - start
            if 0 <= delta < mapped_size:
                if delta >= raw_size or raw_offset + delta >= len(data):
                    raise InvalidPeError(f"{path}: RVA 0x{rva:x} points outside section data")
                return raw_offset + delta
        raise InvalidPeError(f"{path}: RVA 0x{rva:x} not mapped by a section")

    def dll_name(rva: int) -> str:
        offset = file_offset(rva)
        end = data.find(b"\x00", offset, min(offset + 512, len(data)))
        if end < 0:
            raise InvalidPeError(f"{path}: unterminated imported DLL name at RVA 0x{rva:x}")
        try:
            return data[offset:end].decode("ascii").lower()
        except UnicodeDecodeError as exc:
            raise InvalidPeError(f"{path}: invalid imported DLL name") from exc

    names: set[str] = set()
    for directory_index, record_size in ((1, 20), (13, 32)):
        if number_dirs <= directory_index:
            continue
        directory_rva, directory_size = unpack("<II", directories + 8 * directory_index)
        if not directory_rva:
            continue
        if directory_size < record_size:
            raise InvalidPeError(f"{path}: malformed import directory {directory_index}")
        directory_start = file_offset(directory_rva)
        # Some linkers exclude the zero terminator from the advertised directory size.
        for idx in range(min(directory_size // record_size + 1, 4096)):
            pos = directory_start + idx * record_size
            if directory_index == 1:
                descriptor = unpack("<IIIII", pos)
                name_rva = descriptor[3]
            else:
                descriptor = unpack("<IIIIIIII", pos)
                name_rva = descriptor[1]
                if descriptor[0] & 1 == 0:
                    name_rva -= image_base
            if not any(descriptor):
                break
            names.add(dll_name(name_rva))
        else:
            raise InvalidPeError(f"{path}: unterminated import directory {directory_index}")
    return names


_DYNAMIC_CRT = re.compile(r"^(?:vcruntime|msvcp|msvcr|concrt|vcomp)[\w.-]*\.dll$", re.I)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("executables", type=Path, nargs="+", help="Windows PE executables to inspect")
    args = parser.parse_args()
    failed = False
    for exe in args.executables:
        try:
            imported = _imports(exe)
        except (OSError, InvalidPeError) as exc:
            print(f"FAIL: {exc}")
            failed = True
            continue
        forbidden = sorted(dll for dll in imported if _DYNAMIC_CRT.fullmatch(dll))
        if forbidden:
            print(f"FAIL: {exe}: dynamically imports {', '.join(forbidden)}")
            failed = True
        else:
            print(f"PASS: {exe}: no dynamic VC++ runtime imports ({len(imported)} DLLs checked)")
    return 1 if failed else 0


if __name__ == "__main__":
    raise SystemExit(main())
