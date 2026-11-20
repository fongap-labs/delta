"""Synthetic PE fixtures cover direct and delay-loaded VC++ runtime imports."""

import importlib.util
import struct
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).resolve().parents[1] / "check_win_crt.py"
spec = importlib.util.spec_from_file_location("crtcheck", SCRIPT)
assert spec and spec.loader
checker = importlib.util.module_from_spec(spec)
spec.loader.exec_module(checker)


def fake_pe(import_name="KERNEL32.dll", delay_name=None):
    data = bytearray(0x600)
    data[:2] = b"MZ"
    struct.pack_into("<I", data, 0x3C, 0x80)
    data[0x80:0x84] = b"PE\x00\x00"
    coff = 0x84
    struct.pack_into("<H", data, coff + 2, 1)
    struct.pack_into("<H", data, coff + 16, 0xF0)
    opt = coff + 20
    struct.pack_into("<H", data, opt, 0x20B)
    struct.pack_into("<Q", data, opt + 24, 0x140000000)
    struct.pack_into("<I", data, opt + 108, 16)
    sec = opt + 0xF0
    for offset, value in ((8, 0x400), (12, 0x1000), (16, 0x400), (20, 0x200)):
        struct.pack_into("<I", data, sec + offset, value)
    struct.pack_into("<II", data, opt + 112 + 8, 0x1000, 40)
    struct.pack_into("<IIIII", data, 0x200, 0, 0, 0, 0x1050, 0)
    data[0x250:0x250 + len(import_name) + 1] = import_name.encode() + b"\0"
    if delay_name:
        struct.pack_into("<II", data, opt + 112 + 13 * 8, 0x1100, 64)
        struct.pack_into("<IIIIIIII", data, 0x300, 1, 0x1140, 0, 0, 0, 0, 0, 0)
        data[0x340:0x340 + len(delay_name) + 1] = delay_name.encode() + b"\0"
    return data


class PortableRuntimeImportsTests(unittest.TestCase):
    def imports(self, contents):
        with tempfile.TemporaryDirectory() as temporary:
            exe = Path(temporary) / "Delta.exe"
            exe.write_bytes(contents)
            return checker._imports(exe)

    def test_system_dll_only(self):
        self.assertEqual(self.imports(fake_pe()), {"kernel32.dll"})

    def test_dynamic_crt(self):
        names = self.imports(fake_pe("VCRUNTIME140.dll"))
        self.assertTrue(checker._DYNAMIC_CRT.fullmatch(next(iter(names))))

    def test_delayed_crt(self):
        names = self.imports(fake_pe("KERNEL32.dll", "MSVCP140.dll"))
        self.assertEqual(names, {"kernel32.dll", "msvcp140.dll"})

    def test_reject_invalid_pe(self):
        with self.assertRaises(checker.InvalidPeError):
            self.imports(b"not a PE")


if __name__ == "__main__":
    unittest.main()
