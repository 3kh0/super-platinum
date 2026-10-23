"""Portable inert v4 End review checks; no live/native or history execution."""
import base64
import copy
import unittest

import canvas_end_recovery_oracle as R
import canvas_split_recovery_oracle as prior
import test_canvas_end_split_oracle as F
from test_canvas_split_recovery_oracle import packets as prior_packets


def packets(mode, corpus="ascii"):
    stages, terminal, _ = prior_packets(mode, corpus)
    original = F.initial(corpus); write, _ = F.request(original); checkpoint = F.checkpoint(original, write, corpus)
    c = F.expected(corpus)
    for stage in stages:
        stage["splitMode"] = "end"; stage["corpus"] = corpus
        stage["canonical"] = copy.deepcopy(original)
        if stage["stage"] == "stale":
            row = stage["canonical"]["sections"][-1]
            row.update(sequence=2000, raw=F.b64(F.replace(base64.b64decode(row["raw"]), 2, 2000)))
        if stage["journal"]["active"]:
            stage["journal"]["active"]["checkpoint"] = copy.deepcopy(checkpoint)
        preview = stage["preview"]
        if preview["panel"]:
            preview["split"] = [dict(kind="original", text=c.seed), dict(kind="parent", text=c.seed), dict(kind="child", text="")]
            preview["current"][1]["text"] = c.seed
            preview["warning"] = " ".join((*prior.WARNINGS, "Add an empty paragraph after this paragraph", "Empty paragraph"))
    terminal.update(splitMode="end", corpus=corpus, canonical=copy.deepcopy(original), journal=copy.deepcopy(stages[-1]["journal"]))
    return stages, terminal, checkpoint


class EndRecoveryOracle(unittest.TestCase):
    def rejects(self, mode, stages, terminal, checkpoint, corpus="ascii"):
        with self.assertRaises((RuntimeError, ValueError, TypeError, KeyError)):
            R.validate(mode, stages, terminal, checkpoint, corpus)

    def test_all_five_phases_ascii_and_unicode(self):
        for corpus in ("ascii", "unicode"):
            for mode in R.STAGES:
                with self.subTest(mode=mode, corpus=corpus):
                    self.assertEqual(R.validate(mode, *packets(mode, corpus), corpus), F.CHILD)

    def test_v4_retained_status_pair_cannot_be_middle_checkpoint(self):
        for key, value in (("version", 3), ("action", {"Split": {}}), ("request", []), ("sections", [])):
            stages, terminal, checkpoint = packets("restart"); checkpoint[key] = value
            self.rejects("restart", stages, terminal, checkpoint)
        stages, terminal, checkpoint = packets("restart", "unicode")
        self.assertEqual(checkpoint["action"]["End"]["offset_utf16"], 11)
        checkpoint["action"]["End"]["offset_utf16"] = 15
        self.rejects("restart", stages, terminal, checkpoint, "unicode")

    def test_empty_child_is_explained_without_fabricated_content(self):
        for value in ("\n", "New paragraph", "Second paragraph"):
            stages, terminal, checkpoint = packets("review"); stages[0]["preview"]["split"][2]["text"] = value
            self.rejects("review", stages, terminal, checkpoint)
        stages, terminal, checkpoint = packets("review"); stages[0]["preview"]["warning"] = " ".join(prior.WARNINGS)
        self.rejects("review", stages, terminal, checkpoint)

    def test_no_replay_or_partial_unlock_before_deliberate_local_archive(self):
        stages, terminal, checkpoint = packets("restart"); terminal["dispatches"] = 1
        self.rejects("restart", stages, terminal, checkpoint)
        for mode, name in (("review", "cancelled"), ("review", "stale"), ("review", "account-changed"), ("storage-failure", "storage-failed")):
            stages, terminal, checkpoint = packets(mode)
            next(s for s in stages if s["stage"] == name)["journal"]["active"] = None
            self.rejects(mode, stages, terminal, checkpoint)

    def test_account_remount_discards_old_panel_and_preserves_pair(self):
        stages, terminal, checkpoint = packets("review"); stages[4]["preview"] = copy.deepcopy(stages[3]["preview"])
        self.rejects("review", stages, terminal, checkpoint)
        stages, terminal, checkpoint = packets("review"); terminal["accountGuard"]["restoredRead"] = terminal["accountGuard"]["beforeRead"]
        self.rejects("review", stages, terminal, checkpoint)

    def test_current_inventory_and_archive_remain_exact(self):
        stages, terminal, checkpoint = packets("review"); stages[0]["preview"]["current"][1]["text"] = "changed"
        self.rejects("review", stages, terminal, checkpoint)
        stages, terminal, checkpoint = packets("review"); stages[-2]["journal"]["archivedExact"] = False
        self.rejects("review", stages, terminal, checkpoint)
        stages, terminal, checkpoint = packets("reviewed"); terminal["splitMode"] = "middle"
        self.rejects("reviewed", stages, terminal, checkpoint)


if __name__ == "__main__": unittest.main()
