"""Offline validation checks; no native process, window or clipboard access."""
import importlib.util
from pathlib import Path
import struct
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location("visual_runner", Path(__file__).with_name("check-visual-native.py"))
RUNNER = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(RUNNER)


class VisualValidationTests(unittest.TestCase):
    def test_exact_names_presets_and_no_implicit_sweep(self):
        for preset in RUNNER.PRESETS:
            self.assertEqual(RUNNER.selected_fixtures([preset], []), list(RUNNER.PRESETS[preset]))
        self.assertEqual(RUNNER.selected_fixtures([], ["search", "search", "rejected-send"]),
                         ["search", "rejected-send"])
        for names in ([], ["search-made-up"], ["../channel"], ["SEARCH"], ["rejected-send-unknown"]):
            with self.assertRaises(ValueError):
                RUNNER.selected_fixtures([], names)

    def test_board_heading_catalog_narrow_variants_have_native_window_mapping(self):
        # The fixture catalog and window setup are separate registries. A name
        # ending in -narrow alone does not make the native window narrow.
        import re
        names = {name for name in RUNNER.known_fixtures()
                 if name.startswith("slack-list-board-heading") and name.endswith("-narrow")}
        self.assertEqual(names, {"slack-list-board-heading-narrow",
                                 "slack-list-board-heading-checked-narrow"})
        media = (RUNNER.ROOT / "src/desktop/media.rs").read_text(encoding="utf-8")
        narrow_branch = media.split("let initial_size = if protected_narrow", 1)[1].split(
            "WINDOW_MIN_INNER_SIZE", 1)[0]
        mapped = set(re.findall(r'"([a-z][a-z0-9-]*)"', narrow_branch))
        self.assertFalse(names - mapped, f"Catalog fixtures missing native narrow mapping: {names - mapped}")

    def test_capture_must_exist_and_match_native_dimensions(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / "capture.png"
            receipt = {"data": {"width": 12, "height": 8}}
            with self.assertRaises(FileNotFoundError):
                RUNNER.png_dimensions(path, receipt)
            for payload in (b"", b"not png", b"\x89PNG\r\n\x1a\n" + b"\0" * 16):
                path.write_bytes(payload)
                with self.assertRaises(RuntimeError):
                    RUNNER.png_dimensions(path, receipt)
            prefix = b"\x89PNG\r\n\x1a\n" + struct.pack(">I", 13) + b"IHDR"
            path.write_bytes(prefix + struct.pack(">II", 12, 8))
            self.assertEqual(RUNNER.png_dimensions(path, receipt), [12, 8])
            for dimensions in ((0, 8), (12, 0), (13, 8)):
                path.write_bytes(prefix + struct.pack(">II", *dimensions))
                with self.assertRaises(RuntimeError):
                    RUNNER.png_dimensions(path, receipt)

    def test_readiness_requires_first_paint_and_stability_not_nonempty_state(self):
        class Clock:
            now = 0
            def monotonic(self): return self.now
            def sleep(self, duration): self.now += duration
        class Agent:
            def __init__(self, values): self.values = iter(values); self.last = None; self.calls = 0
            def control(self, command, timeout):
                self.calls += 1
                self.last = next(self.values, self.last)
                return {"data": self.last}
        def state(painted, channel="C1", counter=1):
            return {"screen": "main", "active_channel": channel,
                    "performance": {"counter": counter, "startup": {"first_paint_ms": 42} if painted else {}}}
        clock = Clock()
        native = Agent([state(False)] * 3 + [state(True), state(True, "C2"),
                                            state(True, "C2", 2), state(True, "C2", 3)])
        with patch.object(RUNNER.time, "monotonic", clock.monotonic), patch.object(RUNNER.time, "sleep", clock.sleep):
            response, evidence = RUNNER.wait_ready(native, 2)
        self.assertEqual(native.calls, 7)
        self.assertEqual(response["data"]["active_channel"], "C2")
        self.assertEqual(evidence["stable_samples"], 3)
        clock = Clock()
        with patch.object(RUNNER.time, "monotonic", clock.monotonic), patch.object(RUNNER.time, "sleep", clock.sleep):
            with self.assertRaises(TimeoutError):
                RUNNER.wait_ready(Agent([state(False)]), .5)

    def test_taskbar_edge_touch_is_not_positive_area_overlap(self):
        self.assertFalse(RUNNER.overlap((10, 10, 100, 100), (0, 110, 500, 50)))
        self.assertTrue(RUNNER.overlap((10, 10, 100, 100), (0, 109, 500, 50)))


if __name__ == "__main__":
    unittest.main()
