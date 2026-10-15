"""Pure evidence-contract tests. Never launches a native app or calls OS APIs."""
import copy
import importlib.util
import json
from pathlib import Path
import struct
import sys
import tempfile
import unittest
import zlib

# Staged execution uses the repository's existing support modules; installed
# execution finds them alongside this test. Importing them launches no process.
HERE = Path(__file__).resolve().parent
ROOT = next(parent for parent in HERE.parents if (parent / "Cargo.toml").is_file())
sys.path.insert(0, str(ROOT / "scripts"))
spec = importlib.util.spec_from_file_location("hydration_runner", HERE / "check-slack-list-hydration-native.py")
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)


def receipt():
    first, final = runner.populations()
    fields = [["token", "fixture-not-a-token"], ["list_id", "F_LIST_FIXTURE"],
              ["id", "R_HYDRATE_PARENT"], ["include_is_subscribed", "true"], ["_x_reason", "get-record-subtasks"]]
    value = {key: True for key in runner.REQUIRED}
    value.update(ok=True, fixture=runner.FIXTURE, nonce="a" * 32, pid=123, preparedRequests=3,
                 recordOpenRequests=3, mutationRequests=0, applied=[True, False, True],
                 viewport=[1280, 780], openGenerations=[1, 2, 3],
                 attempts=[{"method": "POST", "url": "https://example.test/api/lists.records.info",
                            "fields": copy.deepcopy(fields), "open": i, "detail": i, "serial": i + 9, "revision": 4}
                           for i in (1, 2, 3)], responses=[runner.response(i) for i in range(3)],
                 initialPopulation=runner.initial_population(), afterFirst=first, beforeLate=copy.deepcopy(first),
                 finalPopulation=final)
    return value


def png():
    def chunk(kind, raw):
        return struct.pack(">I", len(raw)) + kind + raw + struct.pack(">I", zlib.crc32(kind + raw) & 0xffffffff)
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", 1, 1, 8, 2, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(b"\x00\x01\x02\x03")) + chunk(b"IEND", b""))


class HydrationEvidence(unittest.TestCase):
    def test_independent_raw_oracle_accepts_exact_three_request_sequence(self):
        runner.validate_wire(receipt())

    def test_wrong_cursor_payload_absence_and_stale_adoption_refuse(self):
        for change in ("cursor", "missing_child", "stale", "unrelated", "response"):
            value = receipt()
            if change == "cursor": value["finalPopulation"]["response_metadata"]["next_cursor"] = ""
            elif change == "missing_child": value["finalPopulation"]["records"].pop(2)
            elif change == "stale": value["finalPopulation"]["records"].append(runner.response(1)["subtasks"][0])
            elif change == "unrelated": value["finalPopulation"]["records"][1]["fields"] = []
            else: value["responses"][2]["subtasks"][0]["view_positions"] = {}
            with self.assertRaises(RuntimeError, msg=change): runner.validate_wire(value)

    def test_wrong_wire_owner_extra_argument_and_duplicate_mount_refuse(self):
        for change in ("parent", "cursor", "method", "open", "applied", "mutation"):
            value = receipt()
            if change == "parent": value["attempts"][0]["fields"][2][1] = "OTHER"
            elif change == "cursor": value["attempts"][0]["fields"].append(["cursor", "guessed"])
            elif change == "method": value["attempts"][0]["method"] = "GET"
            elif change == "open": value["attempts"][1]["open"] = 1
            elif change == "applied": value["applied"] = [True, True, True]
            else: value["mutationRequests"] = 1
            with self.assertRaises(RuntimeError, msg=change): runner.validate_wire(value)

    def full(self, directory):
        value = receipt()
        value["captures"], value["geometry"] = {}, []
        for stage in runner.STAGES:
            path = directory / f"{runner.FIXTURE}-{stage}.png"
            path.write_bytes(png())
            value["captures"][stage] = str(path)
            controls = (["#slack-list-delete-cancel", "#slack-list-delete-confirm-button"] if stage == "confirmation" else
                        ["#slack-list-record-back", "#slack-list-record-delete", "#slack-list-record-duplicate"]
                        if stage in ("pending", "detail", "stale") else
                        ["#slack-list-close", 'tr[data-row-id="R_HYDRATE_PARENT"] .slack-list-record-open'])
            value["geometry"].append({"stage": stage, "ok": True, "bounded": True, "partial": True,
                                      "duplicateBlocked": True, "hitPointsPerControl": 9, "viewport": [1280, 780],
                                      "controls": [{"selector": selector, "hit": True} for selector in controls]})
        edges = [{"stage": stage, "edge": edge} for stage in runner.STAGES for edge in ("before", "after")]
        return value, edges

    def test_exact_owned_captures_and_geometry_validate(self):
        with tempfile.TemporaryDirectory() as name:
            directory = Path(name)
            value, edges = self.full(directory)
            result = runner.validate_native(runner.FIXTURE, "a" * 32, 123, directory, value, edges,
                                            platform="win32", expected_dimensions=[1, 1])
            self.assertEqual(len(result), 7)

    def test_missing_hit_wrong_control_and_reordered_stage_refuse(self):
        with tempfile.TemporaryDirectory() as name:
            directory = Path(name)
            for change in ("hit", "selector", "stage"):
                value, edges = self.full(directory)
                if change == "hit": value["geometry"][0]["controls"][0]["hit"] = False
                elif change == "selector": value["geometry"][0]["controls"][0]["selector"] = "unrelated-control"
                else: edges.reverse()
                with self.assertRaises(RuntimeError):
                    runner.validate_native(runner.FIXTURE, "a" * 32, 123, directory, value, edges,
                                           platform="win32", expected_dimensions=[1, 1])

    def test_duplicate_closed_terminal_receipt_refuses(self):
        with tempfile.TemporaryDirectory() as name:
            directory = Path(name); value, _ = self.full(directory)
            expected = runner.ready_tuple(runner.FIXTURE, "a" * 32, 123)
            log = directory / "closed.log"
            log.write_text(runner.WAITING + json.dumps(expected) + "\n"
                           + (runner.PREFIX + json.dumps(value) + "\n") * 2, encoding="utf-8")
            with self.assertRaises(RuntimeError): runner.validate_closed_log(log, expected, directory, value, [1, 1])


if __name__ == "__main__":
    unittest.main()
