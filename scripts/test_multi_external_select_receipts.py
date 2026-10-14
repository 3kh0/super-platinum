"""Pure multi-external evidence validation. No app, OS interaction, or session reads."""
import copy
import json
from pathlib import Path
import runpy
import struct
import tempfile
import unittest
from unittest import mock
import types
import zlib

from native_check import expected_viewport

R = runpy.run_path(str(Path(__file__).with_name("check-multi-external-select-native.py")))


def chunk(kind, payload):
    return struct.pack(">I", len(payload)) + kind + payload + struct.pack(">I", zlib.crc32(kind + payload) & 0xffffffff)


def png(width=100, height=200, raw=None):
    raw = raw if raw is not None else (b"\0" + b"\0" * width * 3) * height
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 2, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(raw)) + chunk(b"IEND", b""))


class Receipts(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.directory = Path(self.temporary.name)
        self.fixture = R["MODAL"] + "-narrow"
        self.nonce, self.pid = "a" * 32, 42

    def good(self, fixture=None):
        fixture = fixture or self.fixture
        spec, stages = R["contract"](fixture)
        receipt = dict.fromkeys(spec.required, True)
        receipt.update(ok=True, preparedRequests=spec.prepared_requests,
                       viewport=expected_viewport(fixture), fixture=fixture,
                       nonce=self.nonce, pid=self.pid, captures={})
        for stage in stages:
            path = self.directory / f"{fixture}-{stage}.png"
            path.write_bytes(png())
            receipt["captures"][stage] = str(path)
        edges = [{"stage": stage, "edge": edge} for stage in stages for edge in ("before", "after")]
        return receipt, edges

    def validate(self, receipt, edges):
        return R["validate_native"](self.fixture, self.nonce, self.pid, self.directory, receipt, edges)

    def test_all_finalized_fixture_contracts_accept_complete_evidence(self):
        for fixture in R["FIXTURES"]:
            receipt, edges = self.good(fixture)
            result = R["validate_native"](fixture, self.nonce, self.pid, self.directory, receipt, edges)
            self.assertEqual(set(result), set(receipt["captures"]))

    def test_distinct_ordinary_contract_requires_all_assertions_and_three_requests(self):
        fixture = R["MESSAGE"]
        receipt, edges = self.good(fixture)
        spec, _ = R["contract"](fixture)
        self.assertEqual(receipt["preparedRequests"], 3)
        for flag in spec.required:
            changed = copy.deepcopy(receipt)
            changed[flag] = False
            with self.subTest(flag=flag), self.assertRaises(RuntimeError):
                R["validate_native"](fixture, self.nonce, self.pid, self.directory, changed, edges)
        receipt["preparedRequests"] = 2
        with self.assertRaises(RuntimeError):
            R["validate_native"](fixture, self.nonce, self.pid, self.directory, receipt, edges)

    def test_ordinary_uncertain_controls_proof_cannot_be_missing(self):
        for fixture in (R["MESSAGE"], R["MESSAGE"] + "-narrow"):
            receipt, edges = self.good(fixture)
            self.assertIs(receipt.pop("uncertainControlsClear"), True)
            with self.subTest(fixture=fixture), self.assertRaises(RuntimeError):
                R["validate_native"](fixture, self.nonce, self.pid, self.directory, receipt, edges)

    def test_every_required_flag_and_exact_request_count(self):
        spec, _ = R["contract"](self.fixture)
        receipt, edges = self.good()
        for flag in spec.required:
            for value in (None, False, 1):
                changed = copy.deepcopy(receipt)
                changed[flag] = value
                with self.subTest(flag=flag, value=value), self.assertRaises(RuntimeError):
                    self.validate(changed, edges)
        for count in (None, True, 1, 3):
            changed = copy.deepcopy(receipt)
            changed["preparedRequests"] = count
            with self.assertRaises(RuntimeError):
                self.validate(changed, edges)

    def test_terminal_owner_and_viewport_must_match(self):
        receipt, edges = self.good()
        for key, value in (("fixture", R["MODAL"]), ("nonce", "b" * 32),
                           ("pid", 43), ("viewport", [668, 4000]), ("ok", False)):
            changed = copy.deepcopy(receipt)
            changed[key] = value
            with self.assertRaises(RuntimeError):
                self.validate(changed, edges)

    def test_stages_must_be_exact_and_ordered(self):
        receipt, edges = self.good()
        for altered in (edges[:-1], edges + edges[-1:], list(reversed(edges)),
                        [{**edges[0], "extra": True}] + edges[1:]):
            with self.assertRaises(RuntimeError):
                self.validate(receipt, altered)
        for extra in (False, True):
            changed = copy.deepcopy(receipt)
            if extra:
                changed["captures"]["unexpected"] = "elsewhere.png"
            else:
                del changed["captures"]["initial"]
            with self.assertRaises(RuntimeError):
                self.validate(changed, edges)

    def test_wrong_path_missing_png_and_corrupt_signature_fail(self):
        receipt, edges = self.good()
        changed = copy.deepcopy(receipt)
        changed["captures"]["initial"] = str(self.directory / "foreign.png")
        with self.assertRaises(RuntimeError):
            self.validate(changed, edges)
        path = Path(receipt["captures"]["initial"])
        for content in (b"", b"old screenshot", b"\x89PNG\r\n\x1a\n" + struct.pack(">I", 13) + b"IHDR" + struct.pack(">II", 0, 200)):
            path.write_bytes(content)
            with self.assertRaises(RuntimeError):
                self.validate(receipt, edges)
        path.unlink()
        with self.assertRaises(RuntimeError):
            self.validate(receipt, edges)

    def test_ready_tuple_is_exact_including_types(self):
        expected = R["ready_tuple"](self.fixture, self.nonce, self.pid)
        R["validate_ready"](expected, expected)
        for key, value in (("version", True), ("fixture", R["MESSAGE"]),
                           ("nonce", "b" * 32), ("pid", 43)):
            with self.assertRaises(RuntimeError):
                R["validate_ready"]({**expected, key: value}, expected)
            missing = expected.copy()
            del missing[key]
            with self.assertRaises(RuntimeError):
                R["validate_ready"](missing, expected)
        with self.assertRaises(RuntimeError):
            R["validate_ready"]({**expected, "extra": True}, expected)
        with self.assertRaises(RuntimeError):
            R["ready_tuple"]("not-a-fixture", self.nonce, self.pid)
        with self.assertRaises(RuntimeError):
            R["ready_tuple"](self.fixture, "A" * 32, self.pid)

    def test_ready_publication_is_complete_and_never_overwrites(self):
        expected = R["ready_tuple"](self.fixture, self.nonce, self.pid)
        R["publish_ready"](self.directory, expected)
        path = self.directory / "ready.json"
        self.assertEqual(json.loads(path.read_text()), expected)
        before = path.read_bytes()
        with self.assertRaises(FileExistsError):
            R["publish_ready"](self.directory, {**expected, "pid": 43})
        self.assertEqual(path.read_bytes(), before)
        self.assertFalse((self.directory / "ready.staged").exists())

    def test_existing_output_is_refused_without_touching_evidence(self):
        destination = self.directory / self.fixture
        destination.mkdir()
        marker = destination / "existing.png"
        marker.write_bytes(b"old evidence must remain")
        with self.assertRaises(FileExistsError):
            R["run"](self.fixture, self.directory / "never-launch.exe", self.directory)
        self.assertEqual(marker.read_bytes(), b"old evidence must remain")

    def test_exit_audit_rejects_live_or_replaced_process(self):
        R["validate_exit"](100, {"creation_filetime": 100, "exit_filetime": 200})
        for current in ({"creation_filetime": 101, "exit_filetime": 200},
                        {"creation_filetime": 100, "exit_filetime": 0},
                        {"creation_filetime": 100, "exit_filetime": 99},
                        {"creation_filetime": 100, "exit_filetime": True}):
            with self.assertRaises(RuntimeError):
                R["validate_exit"](100, current)

    def test_png_complete_structure_payload_and_owned_dimensions(self):
        path = self.directory / "stage.png"
        path.write_bytes(png())
        self.assertEqual(R["png_signature"](path, [100, 200]), [100, 200])
        with self.assertRaises(RuntimeError):
            R["png_signature"](path, [101, 200])
        corrupt = bytearray(png());corrupt[-5] ^= 1
        for data in (png()[:24], png()[:-12], png() + b"trailing", bytes(corrupt),
                     png(raw=b"\0"), png(raw=(b"\x05" + b"\0" * 300) * 200)):
            path.write_bytes(data)
            with self.assertRaises(RuntimeError):
                R["png_signature"](path, [100, 200])

    def test_closed_log_rejects_late_duplicate_or_incomplete_receipts(self):
        receipt, edges = self.good()
        expected = R["ready_tuple"](self.fixture, self.nonce, self.pid)
        lines = [R["WAITING"] + json.dumps(expected)] + [R["STAGE"] + json.dumps(edge) for edge in edges] + [R["PREFIX"] + json.dumps(receipt)]
        path = self.directory / "native.log"
        path.write_text("\n".join(lines) + "\n")
        R["validate_closed_log"](path, expected, self.directory, receipt, [100, 200])
        for extra in (lines[0], lines[-1], R["STAGE"] + json.dumps(edges[-1])):
            path.write_text("\n".join(lines + [extra]) + "\n")
            with self.assertRaises(RuntimeError):
                R["validate_closed_log"](path, expected, self.directory, receipt, [100, 200])
        path.write_text("\n".join(lines))
        with self.assertRaises(RuntimeError):
            R["validate_closed_log"](path, expected, self.directory, receipt, [100, 200])

    def lifecycle(self, *, movement=False, late_duplicate=False):
        runner = R["run"].__globals__
        fixture, nonce, pid = self.fixture, self.nonce, self.pid
        spec, stages = R["contract"](fixture)
        holder = {}

        class Process:
            def __init__(self): self.pid, self.returncode = pid, None
            def poll(self): return self.returncode

        class FakeNative:
            def __init__(self, name, log, binary, env):
                self.fixture, self.log, self.env = name, log, env
                self.process = Process()
                holder["native"] = self
            def __enter__(self):
                directory = Path(self.env["SUPER_PLATINUM_MULTI_EXTERNAL_SELECT_CAPTURE_DIR"])
                self.receipt = dict.fromkeys(spec.required, True)
                self.receipt.update(ok=True, preparedRequests=2, fixture=fixture, nonce=nonce, pid=pid,
                                    viewport=expected_viewport(fixture, "win32"), captures={})
                for stage in stages:
                    path = directory / f"{fixture}-{stage}.png";path.write_bytes(png())
                    self.receipt["captures"][stage] = str(path)
                self.expected = R["ready_tuple"](fixture, nonce, pid)
                self.edges = [{"stage": stage, "edge": edge} for stage in stages for edge in ("before", "after")]
                self.log.write_text(R["WAITING"] + json.dumps(self.expected) + "\n"
                    + "".join(R["STAGE"] + json.dumps(edge) + "\n" for edge in self.edges)
                    + R["PREFIX"] + json.dumps(self.receipt) + "\n")
                return self
            def receipts(self, prefix):
                return R["closed_receipts"](self.log, prefix)
            def require_running(self): pass
            def capture(self, path): Path(path).write_bytes(png())
            def __exit__(self, *_):
                self.process.returncode = 0
                holder["exited"] = True
                if late_duplicate:
                    with self.log.open("a") as output:
                        output.write(R["PREFIX"] + json.dumps(self.receipt) + "\n")

        class API:
            def __init__(self): self.kernel = types.SimpleNamespace(CloseHandle=lambda _: None)
            def open(self, _): return 123
            def times(self, _):
                return {"creation_filetime": 100, "exit_filetime": 200 if holder.get("exited") else 0}

        def normalization(native, evidence, stack, *_):
            data = {"movement_observed": False, "restore_observed": False}
            evidence["pointer"] = data
            evidence["expectedPngDimensions"] = [100, 200]
            def close():
                holder["closed_before_exit"] = not holder.get("exited", False)
                data.update(movement_observed=movement, restore_attempted=not movement,
                            restore_observed=not movement)
                if movement: data["restore_skipped"] = "Observed movement"
            stack.callback(close)
            return types.SimpleNamespace(check=lambda *args, **kwargs: None)

        binary = self.directory / "fake.exe";binary.write_bytes(b"immutable fixture binary")
        captures = self.directory / "captures"
        with mock.patch.dict(runner, NativeFixture=FakeNative, normalize=normalization), \
                mock.patch.object(runner["secrets"], "token_hex", return_value=nonce), \
                mock.patch.object(runner["sys"], "platform", "win32"), \
                mock.patch.dict(runner["sys"].modules, {"windows_process_metrics": types.SimpleNamespace(WindowsAPI=API)}), \
                mock.patch("builtins.print"):
            with self.assertRaises(RuntimeError):
                R["run"](fixture, binary, captures)
        receipt = json.loads((captures / fixture / f"{fixture}.json").read_text())
        self.assertFalse(receipt["ok"])
        self.assertTrue(receipt["processExited"])
        self.assertTrue(holder["closed_before_exit"])
        self.assertEqual(receipt["cleanupProcessIdentity"]["exit_filetime"], 200)
        return receipt

    def test_lifecycle_silent_pointer_restore_skip_cannot_pass(self):
        receipt = self.lifecycle(movement=True)
        self.assertTrue(receipt["pointer"]["movement_observed"])
        self.assertIn("restoration", receipt["error"])

    def test_lifecycle_late_duplicate_during_exit_cannot_pass(self):
        receipt = self.lifecycle(late_duplicate=True)
        self.assertTrue(receipt["pointer"]["restore_observed"])
        self.assertIn("duplicate", receipt["error"])


if __name__ == "__main__":
    unittest.main()
