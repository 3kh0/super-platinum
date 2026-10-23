"""Portable v5 inert existing-pair review; no native/history execution."""
import base64
import copy
import unittest

import canvas_end_reuse_recovery_oracle as R
import canvas_split_recovery_oracle as prior
import test_canvas_end_reuse_oracle as F
from test_canvas_split_recovery_oracle import packets as prior_packets


def packets(mode, corpus="ascii"):
    stages, terminal, _ = prior_packets(mode, corpus)
    original = F.initial(corpus); write, _ = F.request(original); checkpoint = F.checkpoint(original, write, corpus)
    c = F.end.expected(corpus)
    for stage in stages:
        stage.update(splitMode="reuse", corpus=corpus, canonical=copy.deepcopy(original))
        if stage["stage"] == "stale":
            row = stage["canonical"]["sections"][1]
            row.update(sequence=2000, raw=F.b64(F.replace(base64.b64decode(row["raw"]), 2, 2000)))
        if stage["journal"]["active"]: stage["journal"]["active"]["checkpoint"] = copy.deepcopy(checkpoint)
        preview = stage["preview"]
        if preview["panel"]:
            preview.update(split=[dict(kind="original", text=c.seed), dict(kind="parent", text=c.seed), dict(kind="child", text="")], missing=[],
                           current=[dict(id="title", deleted=False, text="Fixture title"), dict(id="neighbor", deleted=False, text="Another paragraph"),
                                    dict(id="paragraph", deleted=False, text=c.seed), dict(id="empty-final", deleted=False, text="")],
                           warning=" ".join((*prior.WARNINGS, "Reuse the existing empty paragraph after this paragraph", "Empty paragraph", "Empty paragraph")))
    terminal.update(splitMode="reuse", corpus=corpus, canonical=copy.deepcopy(original), journal=copy.deepcopy(stages[-1]["journal"]))
    return stages, terminal, checkpoint


class ReuseRecoveryOracle(unittest.TestCase):
    def rejects(self, mode, stages, terminal, checkpoint, corpus="ascii"):
        with self.assertRaises((RuntimeError, ValueError, TypeError, KeyError)): R.validate(mode, stages, terminal, checkpoint, corpus)

    def test_all_five_workflows_both_corpora(self):
        for corpus in ("ascii", "unicode"):
            for mode in R.STAGES:
                with self.subTest(corpus=corpus, mode=mode):
                    self.assertEqual(R.validate(mode, *packets(mode, corpus), corpus), "empty-final")

    def test_v5_is_not_v4_new_child_or_v3_split(self):
        for key, value in (("version", 4), ("version", 3), ("action", {"End": {}}), ("request", []), ("sections", [])):
            stages, terminal, checkpoint = packets("restart"); checkpoint[key] = value
            self.rejects("restart", stages, terminal, checkpoint)
        stages, terminal, checkpoint = packets("restart", "unicode")
        self.assertEqual(checkpoint["action"]["EndReuse"]["offset_utf16"], 11)
        checkpoint["action"]["EndReuse"]["offset_utf16"] = 15
        self.rejects("restart", stages, terminal, checkpoint, "unicode")

    def test_current_existing_empty_child_cannot_be_missing_or_duplicated(self):
        stages, terminal, checkpoint = packets("review"); stages[0]["preview"]["missing"] = ["child"]
        self.rejects("review", stages, terminal, checkpoint)
        for mutation in ("remove", "duplicate", "change"):
            stages, terminal, checkpoint = packets("review"); rows = stages[0]["preview"]["current"]
            if mutation == "remove": rows.pop()
            elif mutation == "duplicate": rows.append(copy.deepcopy(rows[-1]))
            else: rows[-1]["text"] = "New paragraph"
            self.rejects("review", stages, terminal, checkpoint)

    def test_two_empty_markers_and_unknown_outcome_are_required(self):
        stages, terminal, checkpoint = packets("review"); preview = stages[0]["preview"]
        preview["warning"] = preview["warning"].replace(" Empty paragraph", "", 1)
        self.rejects("review", stages, terminal, checkpoint)
        stages, terminal, checkpoint = packets("review"); stages[0]["preview"]["split"][2]["text"] = "\n"
        self.rejects("review", stages, terminal, checkpoint)

    def test_all_refusal_paths_preserve_exact_whole_existing_pair(self):
        for mode, name in (("review", "cancelled"), ("review", "stale"), ("review", "account-changed"), ("storage-failure", "storage-failed")):
            stages, terminal, checkpoint = packets(mode)
            next(s for s in stages if s["stage"] == name)["journal"]["active"] = None
            self.rejects(mode, stages, terminal, checkpoint)
        stages, terminal, checkpoint = packets("restart"); terminal["dispatches"] = 1
        self.rejects("restart", stages, terminal, checkpoint)

    def test_account_change_cannot_retain_old_review_or_recycle_read(self):
        stages, terminal, checkpoint = packets("review"); stages[4]["preview"] = copy.deepcopy(stages[3]["preview"])
        self.rejects("review", stages, terminal, checkpoint)
        stages, terminal, checkpoint = packets("review"); terminal["accountGuard"]["restoredRead"] = terminal["accountGuard"]["beforeRead"]
        self.rejects("review", stages, terminal, checkpoint)

    def test_archive_and_current_original_inventory_are_exact(self):
        stages, terminal, checkpoint = packets("review"); stages[-2]["journal"]["archivedExact"] = False
        self.rejects("review", stages, terminal, checkpoint)
        stages, terminal, checkpoint = packets("reviewed"); terminal["splitMode"] = "end"
        self.rejects("reviewed", stages, terminal, checkpoint)
        stages, terminal, checkpoint = packets("review"); stages[2]["canonical"]["sections"].pop()
        self.rejects("review", stages, terminal, checkpoint)


if __name__ == "__main__": unittest.main()
