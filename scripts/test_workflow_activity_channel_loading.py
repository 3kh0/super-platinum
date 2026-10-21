import copy
import unittest

import workflow_activity_channel_loading as L
import test_workflow_activity_native as baseline


def loading_sample(channel, box):
    doc = channel["documents"][0]
    native = dict(
        owner=dict(chain=81, detail="29", phase="pending", key="original-key",
                   button="workflow-activity-channel-step-0-1", channel="CACTMISS01"),
        scope=dict(visit=str(channel["managementVisit"]), document=str(doc["id"]),
                   revision=str(doc["revision"]), detail="29"),
        buffers=dict(search=dict(value="", start=0, end=0, generation="3"),
                     status="", user="", limit="100",
                     date=dict(from_="", to="", revision="1", epoch="1", open="false",
                               dirty="false", pending="false", origin="", defaultFrom="", defaultTo="")),
        native=dict(composing=False, refusals=2, leaseAbsent=True),
        notice=dict(text=L.OPENING, role="status", visible=True, outsideScroll=True,
                    visiblePending=1, rect=copy.deepcopy(box)),
        sameOriginalNodes=True,
    )
    native["buffers"]["date"]["from"] = native["buffers"]["date"].pop("from_")
    return dict(nativeBefore=native, nativeAfter=copy.deepcopy(native), observedMs=500,
                unchangedRustState=True, unchangedEndpoint=True)


class ChannelLoadingTests(unittest.TestCase):
    def validate(self, fixture, value, scenario):
        return baseline.R["validate"](fixture, value, baseline.DIRECTORY, scenario,
                                      platform="win32", png=False)

    def test_cancel_and_same_sender_success_at_both_widths(self):
        for narrow in (False, True):
            for spec in (baseline.R["ORDINARY"], L.HELD_SUCCESS):
                self.validate(*baseline.channel_sample(narrow, spec=spec), spec.case)

    def reject(self, stage, change, spec=baseline.R["ORDINARY"]):
        fixture, value = baseline.channel_sample(spec=spec)
        change(next(row for row in value["stages"] if row["stage"] == stage))
        with self.assertRaises(RuntimeError):
            self.validate(fixture, value, spec.case)

    def mutate_both(self, change):
        def apply(row):
            for key in ("nativeBefore", "nativeAfter"):
                change(row["proof"]["exercise"][key])
        return apply

    def test_old_offline_empty_or_duplicate_loading_is_rejected(self):
        for text in (L.OFFLINE, ""):
            self.reject("held", self.mutate_both(lambda v: v["notice"].update(text=text)))
        self.reject("held", self.mutate_both(lambda v: v["notice"].update(visiblePending=2)))
        self.reject("held", lambda r: r["geometry"]["notices"].append(
            dict(text=L.OFFLINE, visible=True, rect=r["geometry"]["notices"][0]["rect"])))

    def test_pending_status_must_be_visible_and_semantic(self):
        for key, value in (("visible", False), ("outsideScroll", False), ("role", "alert")):
            self.reject("held", self.mutate_both(lambda v: v["notice"].update({key: value})))

    def test_status_requires_same_original_document_detail_and_attempt(self):
        for key, value in (("phase", "settled"), ("channel", "FOREIGN"), ("chain", 0),
                           ("button", "workflow-activity-channel-trigger-1"), ("detail", "30")):
            self.reject("held", self.mutate_both(lambda v: v["owner"].update({key: value})))
        self.reject("held", self.mutate_both(lambda v: v["scope"].update(document="99")))
        self.reject("held", self.mutate_both(lambda v: v.update(sameOriginalNodes=False)))

    def test_held_read_does_not_retain_lease_or_block_metadata(self):
        self.reject("held", self.mutate_both(lambda v: v["native"].update(leaseAbsent=False)))
        self.reject("held", lambda r: r["model"]["metadata"].update(busy=True))
        self.reject("held", lambda r: r["proof"]["exercise"].update(unchangedEndpoint=False))

    def test_quiet_observation_must_preserve_exact_native_buffers(self):
        self.reject("held", lambda r: r["proof"]["exercise"]["nativeAfter"]["buffers"]["search"].update(value="new"))
        self.reject("held", lambda r: r["proof"]["exercise"].update(observedMs=0))
        self.reject("held", self.mutate_both(lambda v: v["buffers"].pop("date")))

    def test_retirement_clears_only_retired_pending_owner(self):
        for key, value in (("oldPhase", "pending"), ("oldDetailConnected", True),
                           ("visiblePending", 1), ("currentPending", True)):
            self.reject("retired", lambda r: r["proof"]["exercise"]["cleanup"].update({key: value}))

    def test_held_success_cannot_be_a_fresh_or_cancelled_request(self):
        for key, value in (("serial", 4), ("requestCount", 4), ("delivered", False),
                           ("cancelledBeforeReply", True)):
            self.reject("retried", lambda r: r["proof"]["exercise"]["heldCompletion"].update({key: value}), L.HELD_SUCCESS)
        self.reject("retried", lambda r: r["proof"]["channel"]["ledger"].update(cancelled=1), L.HELD_SUCCESS)
        self.reject("retried", lambda r: r["proof"]["exercise"]["before"].update(selected="FOREIGN"), L.HELD_SUCCESS)


if __name__ == "__main__":
    unittest.main()
