import copy
import json
from pathlib import Path
import runpy
import unittest

import workflow_activity_files as F
import workflow_activity_mixed_navigation as M
from test_workflow_activity_files import request, require

H = runpy.run_path(str(Path(__file__).with_name("check-workflow-activity-native.py")))
VIEWPORT = H["expected_viewport"]("workflow-activity", "win32")
RECT = dict(left=10, top=10, right=110, bottom=30, width=100, height=20)


def files(case, index):
    count, _ = M.counts(case, index)
    reverse = case == "mixed-navigation-reverse"
    pending = reverse and index in (3, 4)
    calls = [request(1, F.LIST), request(2, F.CANVAS)][:min(count, 2)]
    if count == 3:
        row = request(3, F.LIST, "navigation", "malformed" if reverse else "offline")
        if reverse:
            row.update(mode="held-malformed", responseText='{"ok":true,"file":false}')
        calls.append(row)
    completed = count - int(pending)
    return dict(originals=copy.deepcopy(F.ORIGINALS), networkCalls=0, httpCapability=False,
                requests=calls, completed=completed, cancelled=0, held=3 if pending else None,
                settlements=[dict(serial=n, returned=True, cancelled=False)
                             for n in range(1, completed + 1)],
                releases=[dict(serial=3, receiverRetired=False, delivered=True)]
                if reverse and index >= 5 else [])


class MixedFileLedgerTests(unittest.TestCase):
    def test_all_three_exact_sequences(self):
        for case, stages in M.STAGES.items():
            for index, stage in enumerate(stages):
                with self.subTest(case=case, stage=stage):
                    M.validate_files(files(case, index), case, index, require)

    def reject(self, mutate, case="mixed-navigation-reverse", index=5):
        value = files(case, index)
        mutate(value)
        with self.assertRaises(RuntimeError):
            M.validate_files(value, case, index, require)

    def test_late_file_reply_must_be_delivered_not_cancelled(self):
        self.reject(lambda v: v["releases"][0].update(delivered=False, receiverRetired=True))

    def test_no_replacement_or_replay_request(self):
        self.reject(lambda v: v["requests"].append(request(4, F.LIST, "navigation")))

    def test_held_failure_cannot_be_changed_to_success_at_release(self):
        self.reject(lambda v: v["requests"][2].update(
            raw=copy.deepcopy(F.LIST_RAW), responseText=json.dumps(F.LIST_RAW)))

    def test_original_malformed_bytes_are_exact(self):
        self.reject(lambda v: v["requests"][2].update(responseText='{"file":false,"ok":true}'))

    def test_no_early_settlement_while_held(self):
        self.reject(lambda v: v.update(completed=3), index=4)

    def test_offline_result_does_not_fabricate_received_bytes(self):
        self.reject(lambda v: v["requests"][2].update(responseText="null"),
                    case="mixed-navigation")

    def test_navigation_cannot_use_metadata_or_canvas(self):
        self.reject(lambda v: v["requests"][2].update(purpose="metadata"))
        self.reject(lambda v: v["requests"][2].update(id=F.CANVAS))

    def test_exact_held_factory_and_query(self):
        self.reject(lambda v: v["requests"][2].update(fields=[["file", F.CANVAS]]))
        self.reject(lambda v: v["requests"][2].update(url="https://example.test/api/files.delete"))

    def test_no_live_transport_or_changed_originals(self):
        self.reject(lambda v: v.update(httpCapability=True))
        self.reject(lambda v: v["originals"]["list"]["file"].update(title="Replacement"))


def custody(revision=4):
    return dict(account=M.ACCOUNT, epoch=0, channel="C2", channelGeneration=0,
                documents=[dict(id=2, revision=revision, title=H["TITLE"],
                                description=H["DESCRIPTION"], dirty=True, pending=None,
                                workflow={"opaque": [False, None, 17]}, trigger={"opaque": 19},
                                preferences={}, copyPermissions={}, runPermissions={}, restriction={})])


def channel(case, index, kept):
    _, count = M.counts(case, index)
    reverse = case.endswith("-reverse")
    pending = not reverse and index in (3, 4)
    response = (dict(serial=1, raw=None, error="offline", cancelled=False) if reverse else
                dict(serial=1, raw=copy.deepcopy(M.CHANNEL_SUCCESS) if case.endswith("-success")
                     else dict(ok=True, channel=False), error=None, cancelled=False))
    return dict(ledger=dict(original="C2", networkCalls=0, requests=[
        dict(method="conversations.info", args=dict(channel=M.MISSING, include_num_members="false"))
    ] * count, aboutRequests=[], completed=count - int(pending), cancelled=0, held=pending,
                            responses=[response] if count and not pending else []),
                selected="C2", generation=0, documents=copy.deepcopy(kept["documents"]),
                aboutVisit=1, managementVisit=3 if index else None, missingPresent=False,
                sourceRows=[dict(id="C2", name="Review requests", isMember=None, isOpen=True,
                                 unreadCount=None, unreadDisplay=None, mentionCount=None, hasUnreads=False),
                            dict(id="CACTKNOWN01", name="activity-known", isMember=True, isOpen=None,
                                 unreadCount=None, unreadDisplay=None, mentionCount=None, hasUnreads=False)])


def canonical(case, index):
    if not index:
        return None
    entries = []
    if index >= 2:
        for file_id, key in [(F.LIST, "list"), (F.CANVAS, "canvas")]:
            raw = copy.deepcopy(F.ORIGINALS[key])
            entries.append(dict(id=file_id, phase="ready", current=True, source=raw["file"],
                                effective=raw["file"], receipt=raw, response=raw))
    return dict(entries=entries, pendingFile=None,
                navigating=F.LIST if case.endswith("-reverse") and index in (3, 4) else None)


def model(index):
    activity = None
    if index:
        activity = dict(active=2 <= index <= 5, filters=dict(revision=7))
    if index >= 2:
        activity["detail"] = dict(workflow_event=dict(
            workflow_id="WF_REVIEW", workflow_execution_id="E_REVIEW",
            trigger_message=dict(template="Open {list} then {list}",
                                 variables=dict(list=dict(type="LIST", value=F.LIST))),
            step_events=[
                dict(step_execution_id="STEP_REVIEW", future=dict(preserved=17),
                     outcome_message=dict(template="Check {missing}",
                                          variables=dict(missing=dict(type="CHANNEL", value=M.MISSING)))),
                dict(step_execution_id="STEP_MIXED_CANVAS", future=dict(preserved=17),
                     outcome_message=dict(template="Read {canvas}",
                                          variables=dict(canvas=dict(type="CANVAS", value=F.CANVAS)))),
            ]))
    requests = [] if index < 2 else [
        dict(serial=1, method="workflows.activity.list",
             args=dict(workflow_id="WF_REVIEW", limit="100", filters="{}",
                       _x_reason="multi-click-activity-list-fetcher")),
        dict(serial=2, method="workflows.activity.get",
             args=dict(workflow_id="WF_REVIEW", workflow_execution_id="E_REVIEW",
                       _x_reason="Load instance steps for activity tab"))]
    return dict(account=M.ACCOUNT, epoch=0, activity=activity,
                metadata=dict(title=H["TITLE"] if index else None,
                              description=H["DESCRIPTION"] if index else None,
                              visit=3 if index else None, dirty=bool(index), writes=0,
                              networkCalls=0, pending=False, busy=False),
                ledger=dict(requests=requests, finished=len(requests), cancelled=0,
                            held=None, networkCalls=0))


def native(case, index):
    reverse = case.endswith("-reverse")
    text = "" if index == 2 or reverse and index == 3 else (
        M.CHANNEL_OPENING if index == 3 else M.CHANNEL_OFFLINE if reverse else M.LIST_OFFLINE)
    kind = ("list" if reverse else "channel") if index == 3 else (
        "channel" if reverse else "list")
    owner = None if index == 2 else dict(
        chain=9 if index == 3 else 10, key="original-attempt-" + str(min(index, 4)), detail="8",
        phase=("admitted" if reverse else "pending") if index == 3 else "failure",
        button="workflow-activity-file-trigger-1" if kind == "list" else "workflow-activity-channel-step-0-1",
        file=F.LIST if kind == "list" else None, channel=M.MISSING if kind == "channel" else None)
    return dict(ok=True, originalNodes=True, scope=dict(visit="3", document="2", revision="5", detail="8"),
                buffers=dict(search=dict(value="", start=0, end=0, generation="7"),
                             status="", user="", limit="100",
                             date=dict(from_="", to="", revision="7", epoch="1", open="false",
                                       dirty="false", pending="false", origin="", defaultFrom="", defaultTo="")),
                owner=owner, notice=dict(text=text, role="status", visible=bool(text),
                                         rect=copy.deepcopy(RECT), count=int(bool(text))),
                native=dict(composing=False, leaseAbsent=True, refusals=0))


def labels():
    specs = [
        ("workflow-activity-file-trigger-1", "list", "A", F.LIST, F.LIST_LABEL, F.LIST_URL),
        ("workflow-activity-file-trigger-3", "list", "A", F.LIST, F.LIST_LABEL, F.LIST_URL),
        ("workflow-activity-channel-step-0-1", "channel", "BUTTON", M.MISSING, "Channel unavailable", None),
        ("", "canvas", "SPAN", F.CANVAS, F.CANVAS_LABEL, None),
    ]
    return dict(ok=True, measurement="individual-scroll-samples",
                rows=[dict(zip(("id", "kind", "tag", "identity", "text", "href"), values),
                           ok=True, visible=True, rect=copy.deepcopy(RECT)) for values in specs])


def sample(case="mixed-navigation", fixture="workflow-activity"):
    rows, paths = [], {}
    folder = Path("captures")
    for index, stage in enumerate(M.STAGES[case]):
        kept = custody(4 if index == 1 else 5)
        if not index:
            kept["documents"] = []
        source = model(index)
        dom = native(case, index) if 2 <= index <= 5 else None
        if dom:
            dom["buffers"]["date"]["from"] = dom["buffers"]["date"].pop("from_")
        proof = dict(model=copy.deepcopy(source), custody=kept, files=files(case, index), channel=channel(case, index, kept),
                     canonical=canonical(case, index), native=dom,
                     labels=labels() if dom else None,
                     destination=dict(networkCalls=0, httpCapability=False, fixtureView=False,
                                      mutations=0, commentsOpen=False, requests=[], responses=[],
                                      settlements=[], completed=0, overlay=False, loaded=None),
                     exercise={})
        if index == 2:
            proof["exercise"]["entry"] = dict(draft=custody(4), admitted=kept, revisionAdvanced=True)
        if index == 3:
            observed = {k: copy.deepcopy(proof[k])
                        for k in ("model", "channel", "files", "canonical", "custody", "native")}
            proof["exercise"] = dict(before=observed, after=copy.deepcopy(observed), observedMs=500)
        if index == 5:
            before = {k: copy.deepcopy(rows[4]["proof"][k])
                      for k in ("channel", "files", "canonical", "custody", "native")}
            before["model"] = copy.deepcopy(rows[4]["model"])
            after = {k: copy.deepcopy(proof[k])
                     for k in ("channel", "files", "canonical", "custody", "native")}
            after["model"] = copy.deepcopy(source)
            release = (dict(serial=3, delivered=True, receiverRetired=False) if case.endswith("-reverse") else
                       dict(serial=1, requestCount=1, delivered=True, receiverRetired=False,
                            cancelledBeforeReply=False,
                            raw=copy.deepcopy(M.CHANNEL_SUCCESS) if case.endswith("-success")
                            else dict(ok=True, channel=False)))
            proof["exercise"] = dict(before=before, release=release, after=after, observedMs=500,
                                     cachePublication=dict(before=before["channel"]["sourceRows"],
                                                           after=after["channel"]["sourceRows"],
                                                           beforeMissing=False, afterMissing=False))
        if index == 6:
            proof["exercise"] = dict(originalCustody=copy.deepcopy(kept))
        geometry = dict(ok=True, noOverflow=True, viewport=H["expected_viewport"](fixture, "win32"),
                        pngDimensions=[1024, 768], controls=[dict(visible=True, rect=copy.deepcopy(RECT))],
                        notices=[dict(text=dom["notice"]["text"], visible=True, rect=copy.deepcopy(RECT))]
                        if dom and dom["notice"]["text"] else [])
        if index in (1, 6):
            geometry["fields"] = dict(title=H["TITLE"], description=H["DESCRIPTION"],
                                      titleReadonly=False, descriptionReadonly=False)
        rows.append(dict(stage=stage, model=source, proof=proof, geometry=geometry))
        paths[stage] = str(folder / f"{fixture}-{stage}.png")
    return dict(ok=True, fixture=fixture, case=case,
                actualActivityEntry=True, aboutConfiguration=True, draftRetained=True,
                readOnly=True, noLiveNetwork=True, networkCalls=0,
                viewport=H["expected_viewport"](fixture, "win32"), stages=rows,
                captures=paths, final=copy.deepcopy(rows[-1]["model"]),
                **{k: copy.deepcopy(rows[-1]["proof"][k]) for k in ("files", "channel", "destination")})


class MixedReceiptTests(unittest.TestCase):
    def validate(self, value):
        M.validate(value["fixture"], value, Path("captures"), value["case"], H, platform="win32", png=False)

    def reject(self, mutate, case="mixed-navigation"):
        value = sample(case)
        mutate(value)
        with self.assertRaises(RuntimeError):
            self.validate(value)

    def test_three_cases_both_widths(self):
        for case in M.STAGES:
            for fixture in ("workflow-activity", "workflow-activity-narrow"):
                with self.subTest(case=case, fixture=fixture):
                    self.validate(sample(case, fixture))

    def test_actual_outer_dispatch_and_fixture_pairing(self):
        value = sample()
        H["validate"](value["fixture"], value, Path("captures"), value["case"],
                      platform="win32", png=False)
        value["fixture"] = "workflow-activity-members"
        with self.assertRaises(RuntimeError):
            H["validate"](value["fixture"], value, Path("captures"), value["case"],
                          platform="win32", png=False)

    def test_original_receiver_delivery_not_replaced_by_cancel(self):
        self.reject(lambda v: v["stages"][5]["proof"]["exercise"]["release"].update(delivered=False))

    def test_newer_list_notice_cannot_be_overwritten_by_old_channel_failure(self):
        self.reject(lambda v: v["stages"][5]["proof"]["native"]["notice"].update(text=M.CHANNEL_OFFLINE))

    def test_newer_channel_notice_cannot_be_overwritten_by_old_list_failure(self):
        self.reject(lambda v: v["stages"][5]["proof"]["native"]["notice"].update(text=M.LIST_OFFLINE),
                    "mixed-navigation-reverse")

    def test_old_success_cannot_navigate(self):
        self.reject(lambda v: v["stages"][5]["proof"]["channel"].update(selected=M.MISSING),
                    "mixed-navigation-success")

    def test_shared_notice_must_advance_owner_on_newer_action(self):
        self.reject(lambda v: v["stages"][4]["proof"]["native"]["owner"].update(chain=9))

    def test_node_replacement_and_native_buffer_change_rejected(self):
        self.reject(lambda v: v["stages"][4]["proof"]["native"].update(originalNodes=False))
        self.reject(lambda v: v["stages"][5]["proof"]["native"]["buffers"]["search"].update(value="replacement"))

    def test_notice_duplicate_or_hidden_rejected(self):
        self.reject(lambda v: v["stages"][4]["proof"]["native"]["notice"].update(count=2))
        self.reject(lambda v: v["stages"][4]["proof"]["native"]["notice"].update(visible=False))

    def test_current_owner_cannot_be_a_different_occurrence(self):
        self.reject(lambda v: v["stages"][4]["proof"]["native"]["owner"].update(button="workflow-activity-file-trigger-3"))

    def test_late_release_raw_not_swapped(self):
        self.reject(lambda v: v["stages"][5]["proof"]["exercise"]["release"].update(raw=M.CHANNEL_SUCCESS))

    def test_reverse_requires_actual_offline_completion(self):
        self.reject(lambda v: v["stages"][4]["proof"]["channel"]["ledger"]["responses"][0].update(error=None),
                    "mixed-navigation-reverse")

    def test_quiet_before_state_must_match_the_actual_newer_failure(self):
        self.reject(lambda v: v["stages"][5]["proof"]["exercise"]["before"]["native"]["notice"].update(text=""))

    def test_no_list_target_read(self):
        self.reject(lambda v: v["stages"][5]["proof"]["destination"].update(completed=1))

    def test_draft_and_opaque_source_retained(self):
        self.reject(lambda v: v["stages"][5]["proof"]["custody"]["documents"][0]["workflow"].update(opaque=[]))
        self.reject(lambda v: v["stages"][2]["model"]["activity"]["detail"]["workflow_event"]["step_events"][0].update(future={}))

    def test_layout_must_show_current_notice(self):
        self.reject(lambda v: v["stages"][4]["geometry"].update(notices=[]))
        self.reject(lambda v: v["stages"][4]["geometry"]["notices"][0].update(visible=False))

    def test_original_list_and_static_canvas_boundaries(self):
        self.reject(lambda v: v["stages"][2]["proof"]["labels"]["rows"][0].update(href="https://foreign.test/"))
        self.reject(lambda v: v["stages"][2]["proof"]["labels"]["rows"][3].update(tag="A"))

    def test_foreign_capture_and_terminal_rewrite(self):
        self.reject(lambda v: v["captures"].update(back="elsewhere.png"))
        self.reject(lambda v: v["final"]["metadata"].update(title="replacement"))

    def test_held_quiet_window_is_actual_before_after_equality(self):
        self.reject(lambda v: v["stages"][3]["proof"]["exercise"]["after"]["native"]["native"].update(refusals=1))

    def test_all_seven_observations_required_and_bound_to_model(self):
        self.reject(lambda v: v["stages"].pop(5))
        self.reject(lambda v: v["stages"][4]["proof"]["model"].update(epoch=1))

    def test_terminal_raw_ledger_cannot_be_replaced(self):
        self.reject(lambda v: v["channel"]["ledger"]["responses"].clear())

    def test_cache_publication_is_reported_separately_not_synthetic_navigation(self):
        value = sample("mixed-navigation-success")
        fetched = dict(id=M.MISSING, name="activity-fetched", isMember=True, isOpen=None,
                       unreadCount=None, unreadDisplay=None, mentionCount=None, hasUnreads=False)
        for index in (5, 6):
            c = value["stages"][index]["proof"]["channel"]
            c["missingPresent"] = True
            c["sourceRows"].append(copy.deepcopy(fetched))
        late = value["stages"][5]["proof"]
        late["labels"]["rows"][2]["text"] = "activity-fetched"
        late["exercise"]["after"]["channel"] = copy.deepcopy(late["channel"])
        late["exercise"]["cachePublication"].update(after=copy.deepcopy(late["channel"]["sourceRows"]),
                                                     afterMissing=True)
        value["channel"] = copy.deepcopy(value["stages"][6]["proof"]["channel"])
        self.validate(value)
        value["stages"][5]["proof"]["channel"]["sourceRows"][-1]["name"] = "foreign"
        with self.assertRaises(RuntimeError):
            self.validate(value)


if __name__ == "__main__":
    unittest.main()
