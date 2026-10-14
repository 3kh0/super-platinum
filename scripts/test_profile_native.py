"""Offline receipt rejection tests; no native process, session or OS input."""
import copy
import json
from pathlib import Path
import struct
import tempfile
import unittest

import profile_native as runner


def good(kind, fixture, platform="win32"):
    width, height = runner.expected_viewport(fixture, platform)
    value = {"ok": True, **{key: True for key in runner.SPECS[kind]["required"]}}
    layout = {"ok": True, "width": width, "height": height}
    if kind == "profile-edit":
        layout.update(dialogHeight=300, fields=3)
    if kind == "self-status-editor":
        value.update(layout, dialogHeight=300, bodyOverflow=0, footerBottom=height - 5,
                     preparedRequests=3, readbacks=1)
    else:
        value["layout"] = layout
    return value


class ProfileReceiptTests(unittest.TestCase):
    def test_original_contracts_accept_both_sizes_and_platform_viewports(self):
        for kind in runner.SPECS:
            for fixture in (kind, kind + "-narrow"):
                for platform in ("win32", "darwin"):
                    with self.subTest(fixture=fixture, platform=platform):
                        value = good(kind, fixture, platform)
                        runner.validate_receipt(kind, fixture, value, platform)
                        with self.assertRaises(RuntimeError):
                            runner.validate_receipt(kind, fixture, value,
                                                    "darwin" if platform == "win32" else "win32")

    def test_ok_alone_or_truthy_flags_cannot_replace_native_assertions(self):
        for kind in runner.SPECS:
            with self.assertRaises(RuntimeError): runner.validate_receipt(kind, kind, {"ok": True})
            for flag in ("ok", *runner.SPECS[kind]["required"]):
                for mutation in (None, False, 1, "true"):
                    with self.subTest(kind=kind, flag=flag, mutation=mutation):
                        value = good(kind, kind)
                        if mutation is None:
                            del value[flag]
                        else:
                            value[flag] = mutation
                        with self.assertRaises(RuntimeError): runner.validate_receipt(kind, kind, value, "win32")

    def test_status_requires_exact_counts_disconnected_flag_and_bounded_layout(self):
        kind = "self-status-editor"
        for key, bad in (("preparedRequests", 2), ("preparedRequests", 4),
                         ("readbacks", True), ("readbacks", 2), ("noTransport", False),
                         ("bodyOverflow", 2), ("footerBottom", 900),
                         ("dialogHeight", float("nan")), ("width", 1280.0)):
            value = good(kind, kind)
            value[key] = bad
            with self.subTest(key=key, bad=bad), self.assertRaises(RuntimeError):
                runner.validate_receipt(kind, kind, value, "win32")
        for kind in ("profile-edit", "profile-title"):
            value = good(kind, kind)
            self.assertNotIn("preparedRequests", value)
            self.assertNotIn("noTransport", value)
            self.assertFalse(runner.source_contract(kind)["counts_published_in_native_receipt"])
            value["layout"]["ok"] = False
            with self.assertRaises(RuntimeError): runner.validate_receipt(kind, kind, value, "win32")

    def test_exactly_one_complete_terminal_receipt_including_after_exit(self):
        prefix = runner.SPECS["profile-edit"]["prefix"]
        line = prefix + json.dumps(good("profile-edit", "profile-edit"))
        self.assertEqual(runner.terminal_receipts(line, prefix), [])
        self.assertEqual(len(runner.terminal_receipts("startup\n" + line + "\n", prefix, final=True)), 1)
        for text in ("", line, line + "\n" + line + "\n",
                     line + "\n" + prefix + '{"ok":false}\n', line + "\n" + prefix + '{"ok":'):
            with self.subTest(text=text), self.assertRaises(RuntimeError):
                runner.terminal_receipts(text, prefix, final=True)
        with self.assertRaises(RuntimeError): runner.terminal_receipts(prefix + "[]\n", prefix)

    def test_capture_missing_or_wrong_dimensions_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "image.png"
            receipt = {"data": {"width": 1942, "height": 1256}}
            with self.assertRaises(FileNotFoundError): runner.png_dimensions(path, receipt)
            path.write_bytes(b"not a capture")
            with self.assertRaises(RuntimeError): runner.png_dimensions(path, receipt)
            path.write_bytes(b"\x89PNG\r\n\x1a\n" + struct.pack(">I", 13) + b"IHDR" + struct.pack(">II", 1942, 1256))
            self.assertEqual(runner.png_dimensions(path, receipt), [1942, 1256])
            mismatch = copy.deepcopy(receipt)
            mismatch["data"]["height"] = 1255
            with self.assertRaises(RuntimeError): runner.png_dimensions(path, mismatch)


if __name__ == "__main__":
    unittest.main()
