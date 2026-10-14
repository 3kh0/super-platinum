"""Pure validation only. Never launches a native process or reads account storage."""
import copy
from pathlib import Path
import runpy
import unittest

M = runpy.run_path(str(Path(__file__).with_name("check-draft-remote-review-native.py")))
NONCE = "a" * 32
DB = Path("C:/isolated/draft-review.sqlite")
URL = ("https://fixture.slack.com/api/drafts.info?"
       "_x_id=0123456789ab4def8123456789abcdef&_x_csid=1123456789ab4def8123456789abcdef"
       "&_x_desktop_ia=true&_x_foreground=true&_x_frontend_build_type=current"
       "&_x_gantry=true&_x_num_retries=0&_x_version_ts=capture-required"
       "&fp=2123456789ab4def8123456789abcdef&slack_route=default")


def local(text, target):
    return {"composer": {"text": text, "reply_broadcast": target == "thread"}, "attachments": []}


def sample(scenario="remote-channel", phase="exercise"):
    target = "thread" if scenario.endswith("thread") else "channel"
    other = "channel" if target == "thread" else "thread"
    scope = {"account_id": "00000000-0000-4000-8000-000000000081", "key": {"team": "T1", "channel": "C2", "thread": "1719800400.000000" if target == "thread" else None}}
    initial = {"scope": scope, "canonical": local(M["BEFORE"], target), "stored": {"revision": 1},
               "blocked": {"token": "old", "local": local(M["BLOCKED"], target)},
               "alternates": [{"writer": "one"}, {"writer": "two"}]}
    expected = M["ALTERNATE"] if scenario == "alternate-thread" else M["NEWER"] if scenario in (
        "typing-during-read-channel", "typing-during-commit-thread", "changed-thread") else M["AFTER"]
    final = {"scope": scope, "canonical": local(expected, target), "stored": {"revision": 2}, "blocked": None, "alternates": []}
    native = {"text": expected, "display": expected, "codePoints": [ord(c) for c in expected],
              "utf16Length": len(expected.encode("utf-16-le")) // 2}
    count = M["REQUESTS"][scenario] if phase == "exercise" else 0
    remote_id = "DrReviewThread" if target == "thread" else "DrReviewChannel"
    request = {"method": "POST", "url": URL, "retrySafe": False,
               "form": [["token", "fixture-only-token"], ["draft_id", remote_id]],
               "rawResponse": {"ok": True, "draft": {"id": remote_id,
                "client_msg_id": "00000000-0000-4000-8000-000000000083" if target == "thread" else "00000000-0000-4000-8000-000000000082",
                "last_updated_ts": "12.000001", "team_id": "T1", "user_id": "U00000001", "is_deleted": False,
                "is_sent": False, "date_scheduled": 0, "file_ids": [],
                "blocks": [{"type": "rich_text", "elements": [{"type": "rich_text_section", "elements": [{"type": "text", "text": M["AFTER"]}]}]}],
                "destinations": [{"channel_id": "C2", **({"thread_ts": "1719800400.000000", "broadcast": True} if target == "thread" else {})}]}}, "inventory": {"scope": scope}}
    result = {"initialNative": native, "native": native, "initialReview": {"selected": 0, "applyDisabled": True,
        "readableRows": [{"ok": True, "index": i} for i in range(6)], "rows": [{"text": t} for t in (M["BEFORE"], M["BLOCKED"], M["BLOCKED"], M["ALTERNATE"],
                                      "Previously observed Slack version", M["AFTER"])]},
        "preservedBeforeChoice": {target: {"alternates": [{"local": local(M["NEWER"], target)}]}},
        "finishAfterRemount": {"noAdditionalRead": True, "before": {"text": M["BEFORE"], "visit": "old"},
                                "after": {"text": M["BEFORE"], "visit": "new"}}}
    if scenario == "deferred-composition-channel":
        result["preservedBeforeChoice"] = {target: copy.deepcopy(final), other: {"unrelated": "exact"}}
    return {"ok": True, "fixture": M["FIXTURE"], "scenario": scenario, "phase": phase, "nonce": NONCE, "pid": 10,
            "target": target, "isolatedDatabase": str(DB), "noSession": True, "noTransport": True,
            "canonicalMessagesUnchanged": True, "syntheticInput": True, "syntheticIME": True, "noProductionRoute": True,
            "preparedRequests": count, "attempts": [copy.deepcopy(request) for _ in range(count)],
            "initial": {target: final if phase == "restart" else initial, other: {"unrelated": "exact"}},
            "durable": {target: final, other: {"unrelated": "exact"}}, "result": result, "routedInputs": 1,
            "viewport": [1280, 780] if __import__("sys").platform == "win32" else [1280, 800]}


def validate(value):
    return M["validate"](M["FIXTURE"], value["scenario"], value["phase"], NONCE, 10, DB, value)


class ReviewReceipts(unittest.TestCase):
    def test_actual_rest_xparams_url_is_preserved_and_validated(self):
        receipt = sample()
        validate(receipt)
        self.assertEqual(receipt["attempts"][0]["url"], URL)
        for url in (URL.split("?")[0], URL + "&_x_id=extra", URL + "&token=secret",
                    URL + "#fragment", URL.replace("_x_num_retries=0", "_x_num_retries=1"),
                    URL.replace("0123456789ab4def8123456789abcdef", "not-a-uuid"),
                    URL.replace("fixture.slack.com/", "fixture.slack.com.evil/"),
                    URL.replace("https://", "http://")):
            with self.subTest(url=url), self.assertRaises(RuntimeError):
                bad = sample()
                bad["attempts"][0]["url"] = url
                validate(bad)

    def test_each_case_and_restart_contract(self):
        for scenario in M["SCENARIOS"]:
            for phase in ("exercise", "restart"):
                validate(sample(scenario, phase))

    def test_ready_exact_pid_phase_nonce_and_no_extras(self):
        expected = M["ready_tuple"](NONCE, 10, "remote-channel", "exercise")
        M["validate_ready"](expected, expected)
        for key, value in (("pid", 11), ("nonce", "b" * 32), ("phase", "restart"), ("scenario", "changed-thread"), ("extra", 1)):
            bad = {**expected, key: value}
            with self.assertRaises(RuntimeError):
                M["validate_ready"](bad, expected)

    def test_actual_request_cannot_be_replaced_by_expected_flag(self):
        for mutate in (lambda r: r.update(attempts=[]), lambda r: r["attempts"][0]["form"].append(["blocks", "invented"]),
                       lambda r: r["attempts"][0].update(retrySafe=True), lambda r: r["attempts"][0].update(url="https://other/api/drafts.info")):
            value = sample()
            mutate(value)
            with self.assertRaises(RuntimeError):
                validate(value)

    def test_full_inventory_no_default_is_required(self):
        for key, value in (("selected", 1), ("applyDisabled", False), ("rows", [])):
            receipt = sample()
            receipt["result"]["initialReview"][key] = value
            with self.assertRaises(RuntimeError):
                validate(receipt)

    def test_unrelated_inventory_must_remain_exact(self):
        value = sample()
        value["durable"]["thread"]["unrelated"] = "changed"
        with self.assertRaises(RuntimeError):
            validate(value)

    def test_newer_typing_requires_durable_alternate_before_second_choice(self):
        value = sample("changed-thread")
        value["result"]["preservedBeforeChoice"]["thread"]["alternates"] = []
        with self.assertRaises(RuntimeError):
            validate(value)

    def test_composition_finish_cannot_reread_or_reuse_mount(self):
        value = sample("deferred-composition-channel")
        value["result"]["finishAfterRemount"]["after"]["visit"] = "old"
        with self.assertRaises(RuntimeError):
            validate(value)
        value = sample("deferred-composition-channel")
        value["preparedRequests"] = 2
        with self.assertRaises(RuntimeError):
            validate(value)

    def test_restart_cannot_mutate_canonical_blocked_or_alternates(self):
        for key, value in (("canonical", local("unexpected", "channel")), ("blocked", {"token": "new"}), ("alternates", [{"writer": "new"}])):
            receipt = sample(phase="restart")
            receipt["initial"] = copy.deepcopy(receipt["initial"])
            receipt["initial"]["channel"][key] = value
            with self.assertRaises(RuntimeError):
                validate(receipt)

    def test_unicode_scalar_and_utf16_oracles_are_independent(self):
        value = sample()
        value["result"]["native"]["codePoints"] = [63]
        with self.assertRaises(RuntimeError):
            validate(value)


if __name__ == "__main__":
    unittest.main()
