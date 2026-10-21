import copy
import unittest

import workflow_activity_file_recovery as R
import workflow_activity_files as F
from test_workflow_activity_files import labels, request, require


def ledger(case, stage):
    count, completed = R.COUNTS[case][R.STAGES[case].index(stage)]
    sequence = ([(F.CANVAS, "offline"), (F.LIST, "malformed"),
                 (F.CANVAS, "held"), (F.LIST, "held")] if case == "file-recovery"
                else [(F.CANVAS, "held"), (F.CANVAS, "success"), (F.LIST, "success")])
    retired = case.endswith("-retirement") and stage in ("retired", "late-refused", "recovered", "back")
    held = {"metadata-held": 1, "retired": 1, "retry-canvas-held": 3, "retry-list-held": 4}.get(stage)
    releases = []
    if case == "file-recovery":
        if stage in ("canvas-recovered", "retry-list-held", "recovered", "back"):
            releases.append(dict(serial=3, receiverRetired=False, delivered=True))
        if stage in ("recovered", "back"):
            releases.append(dict(serial=4, receiverRetired=False, delivered=True))
    elif stage in ("late-refused", "recovered", "back"):
        releases.append(dict(serial=1, receiverRetired=True, delivered=False))
    return dict(originals=copy.deepcopy(F.ORIGINALS), networkCalls=0, httpCapability=False,
                requests=[request(i, file_id, "metadata", mode)
                          for i, (file_id, mode) in enumerate(sequence[:count], 1)],
                settlements=[dict(serial=i, cancelled=retired and i == 1,
                                  returned=not (retired and i == 1)) for i in range(1, completed + 1)],
                completed=completed, cancelled=int(retired), held=held, releases=releases)


def canonical(case, stage):
    if stage == "about":
        return None
    phases = R.phases(case, stage)
    rows = []
    for file_id, phase in phases.items():
        row = dict(id=file_id, phase=phase, current=False, source=None, effective=None,
                   receipt=None, response=None, notice=None)
        if phase == "ready":
            raw = copy.deepcopy(F.CANVAS_RAW if file_id == F.CANVAS else F.LIST_RAW)
            row.update(current=True, source=copy.deepcopy(raw["file"]), effective=copy.deepcopy(raw["file"]),
                       receipt=copy.deepcopy(raw), response=raw)
        elif phase == "unavailable":
            row["notice"] = R.OFFLINE if file_id == F.CANVAS else R.MALFORMED
        rows.append(row)
    return dict(entries=rows, navigating=None, lastNavigation=None,
                pendingFile=next((file_id for file_id, phase in phases.items() if phase == "loading"), None))


class RecoveryLedgerTests(unittest.TestCase):
    def test_original_failure_retry_and_retirement_sequences(self):
        for case, stages in R.STAGES.items():
            for stage in stages:
                with self.subTest(case=case, stage=stage):
                    R.validate_files(ledger(case, stage), case, stage, require)

    def test_no_automatic_retry_after_initial_failures(self):
        value = ledger("file-recovery", "metadata-failed")
        value["requests"].append(request(3, F.CANVAS, "metadata", "success"))
        with self.assertRaises(RuntimeError):
            R.validate_files(value, "file-recovery", "metadata-failed", require)

    def test_metadata_retry_cannot_be_a_destination_read(self):
        value = ledger("file-recovery", "retry-list-held")
        value["requests"][-1]["purpose"] = "navigation"
        with self.assertRaises(RuntimeError):
            R.validate_files(value, "file-recovery", "retry-list-held", require)

    def test_late_original_receiver_cannot_deliver_into_reentry(self):
        value = ledger("file-recovery-retirement", "recovered")
        value["releases"][0].update(receiverRetired=False, delivered=True)
        with self.assertRaises(RuntimeError):
            R.validate_files(value, "file-recovery-retirement", "recovered", require)

    def test_cancellation_must_belong_to_original_read(self):
        value = ledger("file-recovery-retirement", "recovered")
        value["settlements"][0].update(cancelled=False, returned=True)
        value["settlements"][1].update(cancelled=True, returned=False)
        with self.assertRaises(RuntimeError):
            R.validate_files(value, "file-recovery-retirement", "recovered", require)


class RecoveryCanonicalTests(unittest.TestCase):
    def test_canonical_state_exists_only_after_workflow_entry(self):
        R.validate_canonical(None, "file-recovery", "about", require)
        with self.assertRaises(RuntimeError):
            R.validate_canonical({}, "file-recovery", "about", require)
        with self.assertRaises(RuntimeError):
            R.validate_canonical(None, "file-recovery", "metadata-failed", require)

    def test_unavailable_pending_and_original_canonical_rows(self):
        for case, stages in R.STAGES.items():
            for stage in stages:
                with self.subTest(case=case, stage=stage):
                    R.validate_canonical(canonical(case, stage), case, stage, require)

    def test_failure_cannot_seed_a_guessed_title(self):
        value = canonical("file-recovery", "metadata-failed")
        value["entries"][0]["source"] = dict(id=F.CANVAS, title=F.CANVAS)
        with self.assertRaises(RuntimeError):
            R.validate_canonical(value, "file-recovery", "metadata-failed", require)

    def test_sibling_publication_preserves_earlier_original_response(self):
        for key in ("source", "receipt", "response"):
            value = canonical("file-recovery", "recovered")
            value["entries"][0][key] = None
            with self.subTest(key=key), self.assertRaises(RuntimeError):
                R.validate_canonical(value, "file-recovery", "recovered", require)

    def test_retry_does_not_dispatch_navigation(self):
        value = canonical("file-recovery", "retry-list-held")
        value["navigating"] = F.LIST
        with self.assertRaises(RuntimeError):
            R.validate_canonical(value, "file-recovery", "retry-list-held", require)

    def test_retired_pending_snapshot_is_not_forged_as_success(self):
        value = canonical("file-recovery-retirement", "late-refused")
        value["entries"][0]["phase"] = "ready"
        with self.assertRaises(RuntimeError):
            R.validate_canonical(value, "file-recovery-retirement", "late-refused", require)


def recovery_labels(stage):
    value = labels()
    value.pop("inert")
    state = R.phases("file-recovery", stage)
    for row in value["rows"]:
        ready = state[row["file"]] == "ready"
        if not ready:
            row.update(tag="SPAN", interactive=False, href=None, id="",
                       name=row["kind"].title() + " unavailable",
                       text=row["kind"].title() + " unavailable")
    retry_rows = value["rows"] if stage == "metadata-failed" else value["rows"][:2] if stage == "canvas-recovered" else []
    value["retries"] = [
        dict(file=row["file"], text="Retry file details", disabled=False, connected=True,
             rect=copy.deepcopy(row["rect"]), visible=True, part=row["part"], kind=row["kind"],
             originalOccurrence=True, container=row["container"]) for row in retry_rows]
    notice = (R.OFFLINE if stage == "metadata-failed" else R.MALFORMED if stage == "canvas-recovered"
              else None if stage == "recovered" else R.LOADING)
    value["notice"] = None if notice is None else dict(
        id="workflow-activity-file-notice", text=notice, role="status", visible=True,
        rect=copy.deepcopy(value["rows"][0]["rect"]))
    return value


class RecoveryLabelTests(unittest.TestCase):
    def validate(self, stage, value):
        R.validate_labels(value, "file-recovery", stage, [1280, 800],
                          dict(require=require, rectangle=lambda r, v: None))

    def test_failed_loading_sibling_recovery_and_final_original_labels(self):
        for stage in ("metadata-failed", "retry-canvas-held", "canvas-recovered",
                      "retry-list-held", "recovered"):
            with self.subTest(stage=stage):
                self.validate(stage, recovery_labels(stage))

    def test_unavailable_list_cannot_gain_an_action(self):
        value = recovery_labels("metadata-failed")
        value["rows"][0].update(tag="A", href=F.LIST_URL, interactive=True)
        with self.assertRaises(RuntimeError):
            self.validate("metadata-failed", value)

    def test_pending_read_has_no_live_retry_control(self):
        value = recovery_labels("retry-list-held")
        value["retries"] = recovery_labels("canvas-recovered")["retries"]
        with self.assertRaises(RuntimeError):
            self.validate("retry-list-held", value)

    def test_retry_requires_original_occurrence(self):
        for key, replacement in (("part", "99"), ("file", F.CANVAS), ("originalOccurrence", False),
                                 ("visible", False), ("disabled", True)):
            value = recovery_labels("metadata-failed")
            value["retries"][0][key] = replacement
            with self.subTest(key=key), self.assertRaises(RuntimeError):
                self.validate("metadata-failed", value)

    def test_sibling_failure_remains_visible_after_canvas_recovers(self):
        value = recovery_labels("canvas-recovered")
        value["notice"] = None
        with self.assertRaises(RuntimeError):
            self.validate("canvas-recovered", value)

    def test_final_recovery_clears_old_failure_notice(self):
        value = recovery_labels("recovered")
        value["notice"] = recovery_labels("metadata-failed")["notice"]
        with self.assertRaises(RuntimeError):
            self.validate("recovered", value)


if __name__ == "__main__":
    unittest.main()
