"""Synthetic receipt negatives; these do not claim native or Automerge execution."""
import base64
import copy
import unittest

import canvas_split_recovery_oracle as R
import test_canvas_split_oracle as prior


def source(corpus="ascii"):
    packet = prior.receipt(corpus=corpus)
    return packet["stages"][2]["journal"]["receipt"]["checkpoint"], packet["original"]["fresh"]


def geometry(checked=False):
    item = dict(rect=[20, 40, 60, 70], clip=[0, 0, 400, 600], point=[40, 55], visible=True, hit=True, hitTested=True)
    return dict(ok=True, viewport=[400, 600], checked=checked, submitDisabled=not checked,
                cancelDisabled=False, checkboxDisabled=False, checkbox=copy.deepcopy(item),
                cancel=copy.deepcopy(item), remove=copy.deepcopy(item), label=dict(item, hitTested=False))


def packets(mode, corpus="ascii"):
    expected = prior.fixture_corpus(corpus)
    checkpoint, original = source(corpus)
    active = dict(operation="retained-operation", checkpoint=checkpoint)
    stages = []
    for ordinal, name in enumerate(R.STAGES[mode]):
        canonical = copy.deepcopy(original)
        if name == "stale":
            row = canonical["sections"][-1]
            row.update(sequence=2000, raw=prior.b64(prior.replace(base64.b64decode(row["raw"]), 2, 2000)))
        panel = name in ("review-pair", "review-controls", "stale", "review-confirmation", "storage-failed", "retry-confirmation")
        checked = name not in ("review-pair", "review-controls")
        disabled = name in ("stale", "storage-failed")
        preview = dict(ok=True, editorAbsent=True, viewport=[400, 600], panel=panel, reviewActionPresent=name != "account-changed",
                       split=[dict(kind="original", text=expected.seed), dict(kind="parent", text=expected.left), dict(kind="child", text=expected.right)] if panel else [],
                       current=[dict(id="title", deleted=False, text="Fixture title"), dict(id="paragraph", deleted=False, text=expected.seed), dict(id="neighbor", deleted=False, text="Another paragraph")] if panel else [],
                       missing=["child"] if panel else [], warning=" ".join(R.WARNINGS) if panel else None,
                       checked=checked if panel else None, checkboxDisabled=disabled if panel else None,
                       submitDisabled=(disabled or not checked) if panel else None, cancelDisabled=False if panel else None,
                       error=R.STORAGE if name == "storage-failed" else R.CHANGED if disabled else None)
        archived = name in ("removed", "refreshed", "archive-restored")
        journal = dict(active=None if archived else copy.deepcopy(active), archiveCount=int(archived), archivedExact=None if mode == "reviewed" else archived)
        stages.append(dict(ok=True, mode=mode, stage=name, ordinal=ordinal, noSession=True, noTransport=True,
                           dispatches=0, ownerValid=name != "account-changed", canonical=canonical, preview=preview, journal=journal,
                           owner=dict(account=R.OTHER_ACCOUNT if name == "account-changed" else R.ACCOUNT,
                                      epoch=101 if name == "account-changed" else 102 if mode == "review" and ordinal > 4 else 100,
                                      workspace="T1", user="U1"),
                           reviewGeometry=geometry(name != "review-controls") if name in ("review-controls", "review-confirmation", "retry-confirmation") else None))
    terminal = dict(ok=True, mode=mode, noSession=True, noTransport=True, noReplay=True, outcomeUnknown=True,
                    canonicalUnchanged=True, dispatches=0, reservations=int(mode == "seed"), retirements=0,
                    accountGuard=dict(beforeEpoch=100, changedEpoch=101, changedAccount=R.OTHER_ACCOUNT,
                                      oldReviewRemoved=True, oldReviewActionAbsent=True, receiptPreserved=True,
                                      restoredEpoch=102, restoredAccount=R.ACCOUNT, beforeRead=1000, restoredRead=1001) if mode == "review" else None,
                    canonical=copy.deepcopy(original), journal=copy.deepcopy(stages[-1]["journal"]), layout=dict(ok=True, viewport=[400, 600]))
    if corpus != "ascii":
        terminal["corpus"] = corpus
        for stage in stages: stage["corpus"] = corpus
    return stages, terminal, checkpoint


class RecoveryOracle(unittest.TestCase):
    def rejects(self, callback):
        with self.assertRaises((RuntimeError, ValueError, TypeError, KeyError)):
            callback()

    def test_retained_pair_is_exact_original_and_both_halves(self):
        checkpoint, original = source()
        self.assertEqual(R.retained_pair(checkpoint, original), prior.CHILD)
        for key, value in (("version", 2), ("draft", "lost"), ("request", [0]), ("sections", [])):
            bad = copy.deepcopy(checkpoint); bad[key] = value
            self.rejects(lambda: R.retained_pair(bad, original))
        bad = copy.deepcopy(checkpoint); bad["action"]["Split"]["child_draft"] = "wrong"
        self.rejects(lambda: R.retained_pair(bad, original))

    def test_current_inventory_is_complete_and_byte_exact(self):
        _, original = source(); R.canonical(original, original)
        for key, value in (("document", "other"), ("sections", original["sections"][:2]),
                           ("sections", list(reversed(original["sections"])))):
            bad = copy.deepcopy(original); bad[key] = value
            self.rejects(lambda: R.canonical(bad, original))
        bad = copy.deepcopy(original)
        bad["sections"][-1]["raw"] = prior.b64(base64.b64decode(bad["sections"][-1]["raw"])[:-4])
        self.rejects(lambda: R.canonical(bad, original))

    def test_stale_revision_does_not_invent_changed_content(self):
        _, original = source(); changed = copy.deepcopy(original); row = changed["sections"][-1]
        row.update(sequence=2000, raw=prior.b64(prior.replace(base64.b64decode(row["raw"]), 2, 2000)))
        R.canonical(changed, original, True)
        self.rejects(lambda: R.canonical(changed, original))
        row["raw"] = prior.b64(prior.replace(base64.b64decode(row["raw"]), 12, prior.body("wrong")))
        self.rejects(lambda: R.canonical(changed, original, True))

    def test_remove_requires_visible_enabled_acknowledgement(self):
        for checked in (False, True):
            proof = geometry(checked); R.controls(proof, checked, [400, 600])
            for key in ("ok", "checked", "submitDisabled", "cancelDisabled", "checkboxDisabled"):
                bad = copy.deepcopy(proof); bad[key] = int(bad[key])
                self.rejects(lambda: R.controls(bad, checked, [400, 600]))
            for key, value in (("visible", False), ("hit", False), ("point", [30, 55]),
                               ("rect", [20, -1, 60, 70]), ("rect", [True, 40, 60, 70])):
                bad = copy.deepcopy(proof); bad["remove"][key] = value
                self.rejects(lambda: R.controls(bad, checked, [400, 600]))

    def test_all_five_original_recovery_phases(self):
        for mode in R.STAGES:
            with self.subTest(mode=mode):
                self.assertEqual(R.validate(mode, *packets(mode)), prior.CHILD)

    def test_unicode_all_five_recovery_phases_preserve_ascii_compatibility(self):
        for mode in R.STAGES:
            with self.subTest(mode=mode):
                self.assertEqual(R.validate(mode, *packets(mode, "unicode"), corpus="unicode"), prior.CHILD)
                R.validate(mode, *packets(mode))

    def test_unicode_recovery_corpus_cannot_be_echo_selected(self):
        for mode in ("seed", "review", "reviewed"):
            self.rejects(lambda: R.validate(mode, *packets(mode, "unicode")))
            self.rejects(lambda: R.validate(mode, *packets(mode), corpus="unicode"))
            stages, terminal, checkpoint = packets(mode, "unicode"); del stages[0]["corpus"]
            self.rejects(lambda: R.validate(mode, stages, terminal, checkpoint, "unicode"))
            stages, terminal, checkpoint = packets(mode, "unicode"); terminal["corpus"] = "ascii"
            self.rejects(lambda: R.validate(mode, stages, terminal, checkpoint, "unicode"))

    def test_unicode_review_shows_visible_text_without_normalization(self):
        for value in ("Café &amp; 日本😀", "Café & 日本😀", "Keep this"):
            stages, terminal, checkpoint = packets("review", "unicode")
            stages[0]["preview"]["split"][1]["text"] = value
            self.rejects(lambda: R.validate("review", stages, terminal, checkpoint, "unicode"))
        stages, terminal, checkpoint = packets("review", "unicode")
        stages[0]["preview"]["current"][1]["text"] = "Café &amp; 日本😀Second paragraph"
        self.rejects(lambda: R.validate("review", stages, terminal, checkpoint, "unicode"))

    def test_unicode_recovery_retains_visible_caret_offset_and_escaped_drafts(self):
        stages, terminal, checkpoint = packets("restart", "unicode")
        self.assertEqual(checkpoint["action"]["Split"]["offset_utf16"], 11)
        R.validate("restart", stages, terminal, checkpoint, "unicode")
        for key, value in (("offset_utf16", 15), ("parent_draft", "Café & 日本😀"), ("parent_draft", "Café &amp; 日本😀")):
            stages, terminal, checkpoint = packets("restart", "unicode")
            checkpoint["action"]["Split"][key] = value
            self.rejects(lambda: R.validate("restart", stages, terminal, checkpoint, "unicode"))

    def test_preview_cannot_substitute_intended_pair_for_current_content(self):
        stages, terminal, checkpoint = packets("review")
        stages[0]["preview"]["current"][1]["text"] = prior.C.LEFT
        self.rejects(lambda: R.validate("review", stages, terminal, checkpoint))
        stages, terminal, checkpoint = packets("review"); stages[0]["preview"]["current"][0]["deleted"] = 0
        self.rejects(lambda: R.validate("review", stages, terminal, checkpoint))
        for key, value in (("split", []), ("current", []), ("missing", []), ("editorAbsent", False), ("warning", "Saved successfully")):
            stages, terminal, checkpoint = packets("review"); stages[0]["preview"][key] = value
            self.rejects(lambda: R.validate("review", stages, terminal, checkpoint))

    def test_cancel_stale_account_and_storage_failure_preserve_whole_lock(self):
        for mode, name in (("review", "cancelled"), ("review", "stale"), ("review", "account-changed"), ("storage-failure", "storage-failed")):
            stages, terminal, checkpoint = packets(mode)
            row = next(s for s in stages if s["stage"] == name); row["journal"]["active"] = None
            self.rejects(lambda: R.validate(mode, stages, terminal, checkpoint))
            if name != "cancelled":
                stages, terminal, checkpoint = packets(mode)
                row = next(s for s in stages if s["stage"] == name); row["preview"]["submitDisabled"] = False
                self.rejects(lambda: R.validate(mode, stages, terminal, checkpoint))

    def test_recovery_cannot_dispatch_or_infer_sealed_retirement(self):
        for key, value in (("dispatches", 1), ("reservations", 1), ("retirements", 1), ("outcomeUnknown", False), ("dispatches", False)):
            stages, terminal, checkpoint = packets("restart"); terminal[key] = value
            self.rejects(lambda: R.validate("restart", stages, terminal, checkpoint))
        stages, terminal, checkpoint = packets("review"); stages[0]["dispatches"] = 1
        self.rejects(lambda: R.validate("review", stages, terminal, checkpoint))

    def test_account_remount_discards_old_review_and_acknowledgement(self):
        stages, terminal, checkpoint = packets("review")
        # The previous oracle incorrectly required the stale old-owner panel.
        stages[4]["preview"] = copy.deepcopy(stages[3]["preview"])
        self.rejects(lambda: R.validate("review", stages, terminal, checkpoint))
        for key, value in (("reviewActionPresent", True), ("checked", True), ("editorAbsent", False), ("current", [dict(id="paragraph")])):
            stages, terminal, checkpoint = packets("review"); stages[4]["preview"][key] = value
            self.rejects(lambda: R.validate("review", stages, terminal, checkpoint))

    def test_account_remount_cannot_reset_owner_epoch_or_reuse_old_review(self):
        for index, key, value in ((4, "account", R.ACCOUNT), (4, "epoch", 100), (5, "epoch", 100), (5, "account", R.OTHER_ACCOUNT)):
            stages, terminal, checkpoint = packets("review"); stages[index]["owner"][key] = value
            self.rejects(lambda: R.validate("review", stages, terminal, checkpoint))
        for key, value in (("oldReviewRemoved", False), ("oldReviewActionAbsent", False), ("receiptPreserved", False), ("restoredEpoch", 100),
                           ("beforeRead", 0), ("restoredRead", 1000), ("beforeRead", True)):
            stages, terminal, checkpoint = packets("review"); terminal["accountGuard"][key] = value
            self.rejects(lambda: R.validate("review", stages, terminal, checkpoint))

    def test_archive_requires_exact_decision_and_terminal_association(self):
        for key, value in (("archivedExact", False), ("archiveCount", 0), ("active", {"operation": "stale"})):
            stages, terminal, checkpoint = packets("review"); stages[-2]["journal"][key] = value
            self.rejects(lambda: R.validate("review", stages, terminal, checkpoint))
        stages, terminal, checkpoint = packets("review"); terminal["journal"]["archiveCount"] = 2
        self.rejects(lambda: R.validate("review", stages, terminal, checkpoint))

    def test_missing_reordered_duplicate_stages_and_changed_receipt_rejected(self):
        stages, terminal, checkpoint = packets("review"); stages[1], stages[2] = stages[2], stages[1]
        self.rejects(lambda: R.validate("review", stages, terminal, checkpoint))
        stages, terminal, checkpoint = packets("review"); stages.pop(1)
        self.rejects(lambda: R.validate("review", stages, terminal, checkpoint))
        stages, terminal, checkpoint = packets("review"); stages.insert(1, copy.deepcopy(stages[0]))
        self.rejects(lambda: R.validate("review", stages, terminal, checkpoint))
        stages, terminal, checkpoint = packets("restart"); stages[1]["journal"]["active"]["operation"] = "replacement"
        self.rejects(lambda: R.validate("restart", stages, terminal, checkpoint))
        stages, terminal, checkpoint = packets("restart"); stages[1]["journal"]["active"]["checkpoint"]["draft"] = "changed"
        self.rejects(lambda: R.validate("restart", stages, terminal, checkpoint))


if __name__ == "__main__": unittest.main()
