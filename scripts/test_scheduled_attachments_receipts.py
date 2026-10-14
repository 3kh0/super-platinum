"""Pure fixture receipt checks; no native launch or account storage."""
import copy
import json
from pathlib import Path
import runpy
import sys
import tempfile
import unittest

M = runpy.run_path(str(Path(__file__).with_name("check-scheduled-attachments-native.py")))
NONCE = "a" * 32
QUERY = ("_x_id=0123456789ab4def8123456789abcdef&_x_csid=1123456789ab4def8123456789abcdef"
         "&_x_desktop_ia=true&_x_foreground=true&_x_frontend_build_type=current"
         "&_x_gantry=true&_x_num_retries=0&_x_version_ts=capture-required"
         "&fp=2123456789ab4def8123456789abcdef&slack_route=default")


def raw(ids, empty=False, revision="10.000001"):
    return {"id": M["EMPTY"] if empty else M["DRAFT"],
            "client_msg_id": "00000000-0000-4000-8000-000000000093" if empty else "00000000-0000-4000-8000-000000000092",
            "date_scheduled": 0 if empty else 2100000000, "last_updated_ts": revision,
            "is_deleted": False, "is_sent": False, "file_ids": ids.copy(),
            "blocks": [] if empty else M["blocks"](), "destinations": M["destination"]()}


def sample(scenario="ordered-rich", narrow=False):
    stages, count, writes = M["CASES"][scenario]
    methods = ["drafts.info"] if scenario == "empty-body-cancel" else (
        ["drafts.info", "drafts.update", "drafts.info"] if scenario == "uncertain-owner" else
        ["drafts.info", "drafts.update", "drafts.info", "drafts.update"])
    server, attempts, write_index = raw(M["FILES"]), [], 0
    for index, method in enumerate(methods):
        generation = 2 if ((scenario in ("known-unsent", "rejected") and index >= 2)
                           or (scenario == "uncertain-owner" and index == 2)) else 1
        form = [["token", f"fixture-only-token-{generation}"]]
        if method == "drafts.info":
            form.append(["draft_id", M["DRAFT"]])
            result = {"raw": {"ok": True, "draft": copy.deepcopy(server)}}
        else:
            ids = [] if scenario == "ordered-rich" and write_index == 1 else M["FILES"][1:]
            form.extend([["destinations", json.dumps(M["destination"]())], ["blocks", json.dumps(M["blocks"]())],
                         ["file_ids", json.dumps(ids)], ["date_scheduled", "2100000000"],
                         ["is_from_composer", "true"], ["draft_id", M["DRAFT"]],
                         ["client_last_updated_ts", "1800000000000"]])
            if scenario == "known-unsent" and write_index == 0:
                result = {"error": "known-not-dispatched"}
            elif scenario == "rejected" and write_index == 0:
                result = {"raw": {"ok": False, "error": "token_expired"}}
            else:
                server = raw(ids, revision=f"{20 + index}.000001")
                result = ({"error": "synthetic-response-lost", "serverApplied": True} if scenario == "uncertain-owner"
                          else {"raw": {"ok": True, "draft": copy.deepcopy(server)}})
            write_index += 1
        attempts.append({"method": "POST", "url": f"https://fixture.slack.com/api/{method}?{QUERY}",
                         "form": form, "retrySafe": False, "fixtureGeneration": generation,
                         "headers": [["Content-Type", "application/x-www-form-urlencoded"]], "result": result})
    facts = ("actualLegacyEditorReserved orderedRemoveUndo nonemptyBodyLastFile emptyBodyGuard cancelBeforeDispatch noMutation "
             "uncertainNoReplay stableOwnerRetained foreignAccountHidden competingReservationBlocked positiveReadbackOnly "
             "syntheticCredentialGenerationRotation explicitRetryOnly stagedSelectionRetained knownUnsent definiteRawRejection")
    return {"ok": True, "fixture": M["FIXTURE"] + ("-narrow" if narrow else ""), "scenario": scenario,
            "nonce": NONCE, "pid": 10, "preparedRequests": count, "updateRequests": writes,
            "noSession": True, "noTransport": True, "syntheticInput": True, "noFileDeletion": True,
            "noReplay": True, "actualPreparedRequests": True, "attempts": attempts,
            "server": {M["DRAFT"]: server, M["EMPTY"]: raw(M["FILES"][:1], empty=True)},
            "facts": dict.fromkeys(facts.split(), True),
            "layouts": [{"stage": s, "layout": {"noOverflow": True}} for s in stages],
            "viewport": [668, 380 if sys.platform == "win32" else 400] if narrow else
                        [1280, 780 if sys.platform == "win32" else 800]}


def validate(receipt):
    M["validate"](receipt["fixture"], receipt["scenario"], NONCE, 10, receipt)


class AttachmentReceipts(unittest.TestCase):
    def test_all_five_cases_both_sizes(self):
        for scenario in M["CASES"]:
            for narrow in (False, True):
                validate(sample(scenario, narrow))

    def test_ready_tuple_rejects_unknown_owner_or_case(self):
        self.assertEqual(M["ready_tuple"](M["FIXTURE"], "ordered-rich", NONCE, 10)["pid"], 10)
        for args in (("other", "ordered-rich", NONCE, 10), (M["FIXTURE"], "unknown", NONCE, 10),
                     (M["FIXTURE"], "ordered-rich", "../bad", 10), (M["FIXTURE"], "ordered-rich", NONCE, True)):
            with self.assertRaises(RuntimeError):
                M["ready_tuple"](*args)

    def test_request_routes_queries_and_replay_policy_are_exact(self):
        for key, value in (("url", "https://fixture.slack.com/api/drafts.info"),
                           ("url", f"https://fixture.slack.com/api/drafts.delete?{QUERY}"),
                           ("url", f"https://other/api/drafts.info?{QUERY}"),
                           ("url", f"https://fixture.slack.com/api/drafts.info?{QUERY}&token=extra"),
                           ("retrySafe", True), ("fixtureGeneration", 9)):
            receipt = sample()
            receipt["attempts"][0][key] = value
            with self.subTest(key=key, value=value), self.assertRaises(RuntimeError):
                validate(receipt)

    def test_ordered_files_and_rich_body_cannot_change(self):
        for field, value in (("file_ids", '["F00000003","F00000002"]'),
                             ("blocks", "[]"), ("destinations", '[{"channel_id":"C1"}]'),
                             ("date_scheduled", "0")):
            receipt = sample()
            for pair in receipt["attempts"][1]["form"]:
                if pair[0] == field:
                    pair[1] = value
            with self.subTest(field=field), self.assertRaises(RuntimeError):
                validate(receipt)

    def test_definite_rejection_is_not_an_arbitrary_api_error(self):
        receipt = sample("rejected")
        receipt["attempts"][1]["result"]["raw"]["error"] = "permission_denied"
        with self.assertRaises(RuntimeError):
            validate(receipt)

    def test_uncertain_review_must_not_replay_mutation(self):
        receipt = sample("uncertain-owner")
        receipt["attempts"][2] = copy.deepcopy(receipt["attempts"][1])
        with self.assertRaises(RuntimeError):
            validate(receipt)

    def test_cancel_has_no_mutation_and_preserves_other_draft(self):
        for change in (lambda r: r.update(updateRequests=1),
                       lambda r: r["server"][M["EMPTY"]].update(file_ids=[]),
                       lambda r: r["facts"].update(cancelBeforeDispatch=False)):
            receipt = sample("empty-body-cancel")
            change(receipt)
            with self.assertRaises(RuntimeError):
                validate(receipt)

    def test_stages_are_ordered_and_each_geometry_is_required(self):
        for change in (lambda r: r["layouts"].reverse(), lambda r: r["layouts"][0]["layout"].update(noOverflow=False)):
            receipt = sample()
            change(receipt)
            with self.assertRaises(RuntimeError):
                validate(receipt)

    def test_fresh_output_refuses_existing_evidence_before_launch(self):
        with tempfile.TemporaryDirectory() as root:
            root = Path(root)
            destination = root / (M["FIXTURE"] + "-ordered-rich")
            destination.mkdir()
            evidence = destination / "keep.txt"
            evidence.write_text("prior evidence")
            with self.assertRaises(FileExistsError):
                M["run"](M["FIXTURE"], "ordered-rich", root / "absent.exe", root)
            self.assertEqual(evidence.read_text(), "prior evidence")


if __name__ == "__main__":
    unittest.main()
