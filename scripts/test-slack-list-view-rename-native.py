"""Pure receipt-oracle regressions; never launches a process or accesses Slack."""
import copy
import runpy
from pathlib import Path
import unittest
import json
import tempfile
from types import SimpleNamespace
from unittest.mock import Mock, patch

M = runpy.run_path(str(Path(__file__).with_name("check-slack-list-view-rename-native.py")))

def receipt(fixture="slack-list-view-rename"):
    before = M["rename_document"](fixture)
    after = copy.deepcopy(before)
    after["list_metadata"]["views"][0]["name"] = "Delivery overview"
    permission = {"ok": True, "prefs": {"allow_lists": "full"}}
    personal = {"ok": True, "prefs": {"task_lists": "[]", "assigned_to_list": ""}}
    return {"layoutKinds":[before["list_metadata"]["views"][0]["type"]]*2,"fixture":fixture,"selectedView":"V_CUSTOM","selectedViewPreserved":True,"before": before, "after": after, "initialPopulation": M["initial_population"](),
            "finalPopulation": M["initial_population"](), "writeRequests": 1, "readRequests": 7,
            "visitGenerations": [10, 20], "ledger": {"requests": M["expected_wire"](fixture),
                "responses": [{"ok": True, "file": before}, permission, personal,
                              {"syntheticFailure": "response_lost"}, {"ok": True, "file": after},
                              {"ok": True, "file": after}, permission, personal],
                "server": after, "writePending": False, "readPending": False,
                "finished": [{"serial": 5, "kind": kind, "applied": applied}
                             for kind, applied in [("write", False), ("check", False), ("check", True)]]}}

class Oracle(unittest.TestCase):
    def test_picker_identity_and_label_are_required_at_every_stage(self):
        for stage in M["STAGES"]:
            expected = "Delivery overview" if stage in ("observed", "final") else "Project delivery"
            g = {"pickerExact": True, "pickerID": "V_CUSTOM", "pickerLabel": expected}
            M["validate_picker_stage"](g, stage)
            for bad in ({"pickerID": ""}, {"pickerLabel": "All loaded items"},
                        {"pickerLabel": "Wrong shared view"}, {"pickerExact": False}):
                with self.assertRaises(RuntimeError): M["validate_picker_stage"]({**g, **bad}, stage)
            for missing in g:
                incomplete = dict(g); incomplete.pop(missing)
                with self.assertRaises(RuntimeError): M["validate_picker_stage"](incomplete, stage)

    def test_exact_receipt(self):

        for fixture in M["FIXTURES"]: M["validate_wire"](receipt(fixture))

    def test_missing_or_replayed_request(self):
        for change in (lambda r: r["ledger"]["requests"].pop(),
                       lambda r: r["ledger"]["requests"].append(r["ledger"]["requests"][3])):
            r = receipt(); change(r)
            with self.assertRaises(RuntimeError): M["validate_wire"](r)

    def test_id_not_key_and_no_layout_change_or_cursor(self):
        for fields in ([["key", "NAME"]], [["type", "calendar"]], [["cursor", "invented"]]):
            r = receipt(); r["ledger"]["requests"][3]["fields"].extend(fields)
            with self.assertRaises(RuntimeError): M["validate_wire"](r)

    def test_population_completeness_and_unknown_metadata_are_exact(self):
        for change in (lambda r: r.update(layoutKinds=["calendar","calendar"]),
                       lambda r: r["finalPopulation"]["response_metadata"].pop("next_cursor"),
                       lambda r: r["after"]["list_metadata"].pop("opaque"),
                       lambda r: r["finalPopulation"]["records"].pop()):
            r = receipt(); change(r)
            with self.assertRaises(RuntimeError): M["validate_wire"](r)

    def test_old_visit_cannot_apply_or_be_reused(self):
        for change in (lambda r: r["ledger"]["finished"][1].update(applied=True),
                       lambda r: r.update(visitGenerations=[10, 10])):
            r = receipt(); change(r)
            with self.assertRaises(RuntimeError): M["validate_wire"](r)

class Cleanup(unittest.TestCase):
    def test_pointer_cleanup_preserves_primary_and_fails_without_primary(self):
        primary = ValueError("original workflow failure")
        def broken(): raise OSError("pointer restore failed")
        evidence = {}
        with self.assertRaises(ValueError) as caught:
            with M["pointer_cleanup"](evidence) as stack:
                stack.callback(broken)
                raise primary
        self.assertIs(caught.exception, primary)
        self.assertIn("pointer restore failed", evidence["pointerCleanupError"])
        with self.assertRaises(OSError):
            with M["pointer_cleanup"]({}) as stack: stack.callback(broken)

    def test_final_audit_collects_cleanup_handle_log_and_hash_failures(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp); binary = root / "fixture.bin"; log = root / "fixture.log"
            binary.write_bytes(b"changed")
            log.write_text(M["WAITING"] + '{}\n' + M["PREFIX"] + '{"ok":false}\n'
                           + M["STAGE"] + '{', encoding="utf-8")
            native = SimpleNamespace(process=SimpleNamespace(poll=lambda: 1), cleanup_errors=[{"stage": "terminate"}],
                                     binary=binary, log=log)
            api = SimpleNamespace(times=lambda _: {"creation_filetime": 5, "exit_filetime": 8},
                                  kernel=SimpleNamespace(CloseHandle=Mock(return_value=False)))
            evidence = {"binarySha256": "old", "processCreationFiletime": 5, "pointerCleanupError": "restore"}
            errors = M["final_audit"](native, evidence, api, 99, None, root)
            stages = {e["stage"] for e in errors}
            self.assertTrue({"cleanup", "pointer-cleanup", "handle", "closed-log-stages", "closed-log-validation", "binary"} <= stages)
            self.assertEqual(evidence["closedLog"]["terminal"], [{"ok": False}])
            self.assertEqual(evidence["exitIdentity"]["creation_filetime"], 5)
            self.assertEqual(evidence["cleanupErrors"], native.cleanup_errors)
            self.assertEqual(evidence["binarySha256After"], M["binary_sha"](binary))
            api.kernel.CloseHandle.assert_called_once_with(99)

    def test_failure_diagnostic_is_owned_and_errors_are_retained(self):
        native = SimpleNamespace(log=Path("owned.log")); evidence = {}
        M["diagnostic_capture"](None, native, evidence)
        self.assertIn("failureCaptureSkipped", evidence)
        audit = SimpleNamespace(capture=Mock(side_effect=RuntimeError("owner retired")))
        M["diagnostic_capture"](audit, native, evidence)
        self.assertIn("owner retired", evidence["failureCaptureError"])
        self.assertNotIn("failureScreenshot", evidence)
        audit.capture.assert_called_once_with(Path("owned-failure.png"), "failure")

    def test_run_failure_retains_original_and_all_final_evidence(self):
        primary = ValueError("native assertion failed")
        events = []
        class FakeNative:
            def __init__(self, fixture, log, binary, env):
                self.fixture, self.log, self.binary, self.env = fixture, log, binary, env
                self.process = SimpleNamespace(pid=71, poll=lambda: None)
                self.cleanup_errors = []
            def __enter__(self):
                self.waiting = M["ready_tuple"](self.fixture, self.env["SUPER_PLATINUM_LIST_VIEW_RENAME_NONCE"], 71)
                self.log.write_text(M["WAITING"] + json.dumps(self.waiting) + "\n" + M["PREFIX"] + '{"ok":false}\n', encoding="utf-8")
                return self
            def __exit__(self, *_):
                events.append("process cleanup")
                self.process.poll = lambda: 1
                self.cleanup_errors.append({"stage": "synthetic cleanup failure"})
            def require_running(self): pass
            def receipts(self, prefix):
                return [self.waiting] if prefix == M["WAITING"] else ([{"ok": False}] if prefix == M["PREFIX"] else [])
        def normalize(native, evidence, stack, *_):
            def close():
                events.append("pointer cleanup")
                raise RuntimeError("pointer failure")
            stack.callback(close)
        class FakeAudit:
            def __init__(self, *_): pass
            def capture(self, *_): events.append("diagnostic before cleanup")
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp); binary = root / "fixture.bin"; binary.write_bytes(b"frozen")
            g = M["run"].__globals__
            with patch.dict(g, {"NativeFixture": FakeNative, "normalize": normalize, "CaptureAudit": FakeAudit,
                                "publish_ready": lambda *_: None, "validate_native": Mock(side_effect=primary)}), \
                    patch.object(g["sys"], "platform", "linux"), patch("builtins.print"):
                with self.assertRaises(ValueError) as caught: M["run"](M["FIXTURE"], binary, root / "captures")
            self.assertIs(caught.exception, primary)
            self.assertEqual(events, ["diagnostic before cleanup", "pointer cleanup", "process cleanup"])
            evidence = json.loads((root / "captures" / M["FIXTURE"] / (M["FIXTURE"] + ".json")).read_text(encoding="utf-8"))
            self.assertFalse(evidence["ok"])
            self.assertEqual(evidence["error"], "ValueError: native assertion failed")
            self.assertEqual(evidence["closedLog"]["terminal"], [{"ok": False}])
            self.assertEqual(evidence["binarySha256After"], evidence["binarySha256"])
            self.assertTrue(evidence["processExited"])
            self.assertTrue(evidence["cleanupErrors"])
            self.assertIn("pointer failure", evidence["pointerCleanupError"])

    def test_final_audit_success_requires_closed_log_and_unchanged_binary(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp); binary = root / "fixture.bin"; log = root / "fixture.log"
            binary.write_bytes(b"frozen"); log.write_text("", encoding="utf-8")
            native = SimpleNamespace(process=SimpleNamespace(poll=lambda: 0), cleanup_errors=[], binary=binary, log=log)
            evidence = {"binarySha256": M["binary_sha"](binary), "native": {"ok": True}}
            g = M["final_audit"].__globals__
            with patch.object(g["sys"], "platform", "linux"), patch.dict(g, {"validate_closed_log": Mock(return_value={"initial": [100, 100]})}):
                self.assertEqual(M["final_audit"](native, evidence, None, None, {"fixture": "fake"}, root), [])
            self.assertTrue(evidence["closedLogRevalidated"])
            self.assertEqual(evidence["cleanupErrors"], [])
            self.assertEqual(evidence["binarySha256After"], evidence["binarySha256"])


if __name__ == "__main__": unittest.main()
