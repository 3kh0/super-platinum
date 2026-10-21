import copy
import json
import unittest
from urllib.parse import urlencode

import workflow_activity_files as F


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


def request(serial=1, file_id=F.LIST, purpose="metadata", mode="success"):
    raw = F.expected_response(file_id, mode)
    query = dict(_x_id="0" * 32, _x_csid="1" * 32, fp="2" * 32,
                 _x_desktop_ia="true", _x_foreground="true",
                 _x_frontend_build_type="current", _x_gantry="true",
                 _x_num_retries="0", _x_version_ts="capture-required", slack_route="default")
    return dict(serial=serial, id=file_id, purpose=purpose, mode=mode,
                method="files.info", httpMethod="POST", fields=[["file", file_id]],
                args=dict(file=file_id), url="https://example.test/api/files.info?" + urlencode(query),
                raw=raw, responseText=None if raw is None else json.dumps(raw, ensure_ascii=False))


def sample():
    return dict(originals=copy.deepcopy(F.ORIGINALS), networkCalls=0, httpCapability=False,
                requests=[request(), request(2, F.CANVAS)],
                settlements=[dict(serial=n, returned=True, cancelled=False) for n in (1, 2)],
                completed=2, cancelled=0, held=None, releases=[])


class FileLedgerTests(unittest.TestCase):
    def test_original_metadata_and_all_navigation_failures(self):
        value = sample()
        for mode in ("malformed", "wrong-id", "unsupported", "deleted", "offline", "success"):
            serial = len(value["requests"]) + 1
            value["requests"].append(request(serial, purpose="navigation", mode=mode))
            value["settlements"].append(dict(serial=serial, returned=True, cancelled=False))
            value["completed"] += 1
            F.validate_ledger(value, require)

    def test_real_held_receiver_cancel_then_release(self):
        value = sample()
        value["requests"].append(request(3, purpose="navigation", mode="held"))
        value["held"] = 3
        F.validate_ledger(value, require)
        value["settlements"].append(dict(serial=3, returned=False, cancelled=True))
        value.update(completed=3, cancelled=1, held=None,
                     releases=[dict(serial=3, receiverRetired=True, delivered=False)])
        F.validate_ledger(value, require)

    def rejected(self, mutate):
        value = sample()
        mutate(value)
        with self.assertRaises(RuntimeError):
            F.validate_ledger(value, require)

    def test_live_capability_or_network_rejected(self):
        for key, replacement in (("networkCalls", 1), ("httpCapability", True)):
            with self.subTest(key=key):
                self.rejected(lambda v: v.update({key: replacement}))

    def test_canvas_is_display_only(self):
        self.rejected(lambda v: v["requests"][1].update(purpose="navigation"))

    def test_original_title_not_replaced_with_raw_id(self):
        self.rejected(lambda v: v["originals"]["list"]["file"].update(title=F.LIST))

    def test_exact_carrier_and_received_bytes_required(self):
        self.rejected(lambda v: v["requests"][0]["raw"].pop("paging"))
        self.rejected(lambda v: v["requests"][0].update(responseText='{"ok":true}'))
        self.rejected(lambda v: v["requests"][0].update(responseText=None))

    def test_request_cannot_cross_file_or_endpoint(self):
        self.rejected(lambda v: v["requests"][0].update(id="FFOREIGN"))
        self.rejected(lambda v: v["requests"][0].update(url=v["requests"][0]["url"].replace("example.test", "slack.com")))
        self.rejected(lambda v: v["requests"][0].update(url=v["requests"][0]["url"].replace("files.info", "files.delete")))

    def test_duplicate_or_secret_request_fields_rejected(self):
        self.rejected(lambda v: v["requests"][0]["fields"].append(["file", F.LIST]))
        self.rejected(lambda v: v["requests"][0]["fields"].append(["token", "synthetic"]))
        self.rejected(lambda v: v["requests"][0]["args"].update(token="synthetic"))

    def test_duplicate_query_and_missing_trace_rejected(self):
        self.rejected(lambda v: v["requests"][0].update(url=v["requests"][0]["url"] + "&_x_id=" + "0" * 32))
        self.rejected(lambda v: v["requests"][0].update(url=v["requests"][0]["url"].replace("fp=" + "2" * 32, "fp=bad")))

    def test_callback_cannot_settle_twice(self):
        self.rejected(lambda v: (v["settlements"].append(copy.deepcopy(v["settlements"][0])), v.update(completed=3)))

    def test_callback_requires_exact_original_serial_and_outcome(self):
        self.rejected(lambda v: v["settlements"][0].update(serial=9))
        self.rejected(lambda v: v["settlements"][0].update(returned=False))
        self.rejected(lambda v: v["settlements"][0].update(cancelled=True))

    def test_no_fake_held_owner_or_release(self):
        self.rejected(lambda v: v.update(held=1))
        self.rejected(lambda v: v["releases"].append(dict(serial=1, receiverRetired=False, delivered=True)))

    def test_offline_response_cannot_fabricate_bytes(self):
        value = request(purpose="navigation", mode="offline")
        value["responseText"] = "{}"
        with self.assertRaises(RuntimeError):
            F.validate_request(value, 1, require)

    def test_metadata_failure_requires_explicit_recovery_contract(self):
        value = request(mode="offline")
        with self.assertRaises(RuntimeError):
            F.validate_request(value, 1, require)
        F.validate_request(value, 1, require, metadata_modes=("offline",))
        value["responseText"] = "{}"
        with self.assertRaises(RuntimeError):
            F.validate_request(value, 1, require, metadata_modes=("offline",))


def destination():
    specs = [
        ("files.info", [["file", F.LIST]]),
        ("lists.records.list", [["list_id", F.LIST], ["include_subtasks", "true"], ["archived", "false"],
                               ["include_suggested", "false"], ["_x_reason", "get-list-records"]]),
        ("team.prefs.get", [["target_team", "TWORKFLOW"], ["_x_reason", "native-list-permission"]]),
        ("users.prefs.get", [["prefs", "task_lists,assigned_to_list"], ["_x_reason", "native-list-completion-permission"]]),
        ("experiments.getByUser", [["ssb_platform", "windows"], ["ssb_release_channel", "prod"]]),
    ]
    records = [dict(id="R_ACTIVITY_DESTINATION", list_id=F.LIST, position=0,
                    fields=[dict(key="NAME", text=F.ROW)], activity_navigation_opaque={"keep": [False, 0, None]})]
    responses = [dict(ok=True, file=copy.deepcopy(F.LIST_RAW["file"])),
                 dict(ok=True, list_id=F.LIST, records=copy.deepcopy(records)),
                 dict(ok=True, prefs=dict(allow_lists="full")),
                 dict(ok=True, prefs=dict(task_lists="[]", assigned_to_list="")),
                 dict(ok=True, assignments={})]
    return dict(networkCalls=0, httpCapability=False, fixtureView=False, mutations=0, commentsOpen=False,
                overlay=True, completed=1,
                requests=[dict(serial=1, id=F.LIST, generation=11, read=12, revision=10,
                               requests=[dict(method="POST", url="https://example.test/api/" + endpoint,
                                              fields=fields) for endpoint, fields in specs])],
                responses=[dict(serial=1, responses=responses)],
                settlements=[dict(serial=1, current=True, parsed=True, refreshAgain=False)],
                loaded=dict(id=F.LIST, generation=11, read=0, revision=13, loading=False, error=None,
                            document=copy.deepcopy(F.LIST_RAW["file"]), records=records))


class FileDestinationTests(unittest.TestCase):
    def test_actual_read_consumes_attempt_and_advances_revision(self):
        F.validate_destination(destination(), True, "destination", require)
        value = destination()
        value["requests"][0]["revision"] = 0
        value["loaded"]["revision"] = 1
        F.validate_destination(value, True, "destination", require)

    def rejected(self, mutate):
        value = destination()
        mutate(value)
        with self.assertRaises(RuntimeError):
            F.validate_destination(value, True, "destination", require)

    def test_seeded_view_and_mutation_are_not_read_acceptance(self):
        self.rejected(lambda v: v.update(fixtureView=True))
        self.rejected(lambda v: v.update(mutations=1))
        self.rejected(lambda v: v.update(httpCapability=True))

    def test_original_parser_read_cannot_be_skipped_or_replayed(self):
        self.rejected(lambda v: v["requests"][0]["requests"].pop())
        self.rejected(lambda v: v["settlements"][0].update(parsed=False))
        self.rejected(lambda v: v["settlements"][0].update(refreshAgain=True))

    def test_foreign_document_or_record_rejected(self):
        self.rejected(lambda v: v["loaded"]["document"].update(id="FFOREIGN"))
        self.rejected(lambda v: v["loaded"]["records"][0].update(list_id="FFOREIGN"))
        self.rejected(lambda v: v["loaded"]["records"][0].update(fields=[dict(key="NAME", text="Guessed row")]))

    def test_display_decoding_cannot_rewrite_canonical_title(self):
        self.rejected(lambda v: v["loaded"]["document"].update(title=F.LIST_LABEL))
        self.rejected(lambda v: v["responses"][0]["responses"][0]["file"].update(title=F.LIST_LABEL))

    def test_pending_or_wrong_generation_not_accepted_as_loaded(self):
        self.rejected(lambda v: v["loaded"].update(read=12))
        self.rejected(lambda v: v["loaded"].update(revision=10))
        self.rejected(lambda v: v["loaded"].update(generation=100))
        self.rejected(lambda v: v["loaded"].update(loading=True))
        self.rejected(lambda v: v["requests"][0].update(revision=-1))


def labels(inert=False):
    icon_rect = dict(left=10, top=20, right=25, bottom=35, width=15, height=15)
    name_rect = dict(left=27, top=20, right=190, bottom=35, width=163, height=15)
    rect = dict(left=10, top=20, right=190, bottom=35, width=180, height=15)
    rows = []
    for container, ordinal, file_id, kind, part, text in [
        ("workflow-activity-trigger-message", 0, F.LIST, "list", "1", F.LIST_LABEL),
        ("workflow-activity-trigger-message", 1, F.LIST, "list", "3", F.LIST_LABEL),
        ("workflow-activity-step-message-0", 0, F.CANVAS, "canvas", "1", F.CANVAS_LABEL),
    ]:
        anchor = kind == "list" and not inert
        rows.append(dict(ok=True, container=container, ordinal=ordinal, file=file_id, kind=kind,
                         part=part, text=text, name=text, visible=True, inline=True,
                         interactive=anchor, tag="A" if anchor else "SPAN",
                         id="workflow-activity-file-trigger-" + part if anchor else "",
                         href=F.LIST_URL if anchor else None, rect=copy.deepcopy(rect),
                         nameRect=copy.deepcopy(name_rect),
                         icon=dict(material=True, pathMatches=True, ariaHidden="true",
                                   viewBox="0 -960 960 960", fill="currentColor", visible=True,
                                   rect=copy.deepcopy(icon_rect))))
    return dict(ok=True, inert=inert, measurement="individual-scroll-samples", detailRevision="4", rows=rows)


class FileLabelTests(unittest.TestCase):
    def validate(self, value, inert=False):
        def rectangle(rect, viewport):
            require(all(key in rect for key in ("left", "top", "right", "bottom", "width", "height")),
                    "Missing measured rectangle")
            require(0 <= rect["left"] < rect["right"] <= viewport[0]
                    and 0 <= rect["top"] < rect["bottom"] <= viewport[1], "Offscreen label")
        F.validate_labels(value, [1280, 800], dict(require=require, rectangle=rectangle), inert=inert)

    def test_exact_duplicate_occurrences_and_static_canvas(self):
        self.validate(labels())
        self.validate(labels(True), True)

    def rejected(self, mutate):
        value = labels()
        mutate(value)
        with self.assertRaises(RuntimeError):
            self.validate(value)

    def test_canvas_cannot_gain_an_invented_action(self):
        self.rejected(lambda v: v["rows"][2].update(interactive=True))
        self.rejected(lambda v: v["rows"][2].update(tag="A", href="https://example.test/docs/" + F.CANVAS))

    def test_duplicate_ids_still_need_exact_occurrence(self):
        self.rejected(lambda v: v["rows"][1].update(part="1"))
        self.rejected(lambda v: v["rows"].pop(1))

    def test_list_requires_original_title_and_permalink(self):
        self.rejected(lambda v: v["rows"][0].update(text=F.LIST))
        self.rejected(lambda v: v["rows"][0].update(href="https://example.test/files/" + F.LIST))
        self.rejected(lambda v: v["rows"][0].update(name="Révision &amp; 中 List"))

    def test_forged_or_unmeasured_icon_is_rejected(self):
        self.rejected(lambda v: v["rows"][0]["icon"].update(pathMatches=False))
        self.rejected(lambda v: v["rows"][0]["icon"].update(fill="red"))
        self.rejected(lambda v: v["rows"][0]["icon"].update(visible=False))


class FileRefusalTests(unittest.TestCase):
    @staticmethod
    def exercise(stage):
        text = "ERROR" if stage == "filter-refused" else "Uncommitted native search"
        notice = "Finish editing the current text before opening this List. Your draft is unchanged."
        value = dict(kind=stage, attempt=dict(ok=True, refusals=3, value=text),
                     native=dict(ok=True, refusals=4, value=text, notice=notice))
        if stage == "calendar-refused":
            value = dict(mode="calendar-outside-consumed", noFileRead=True, noDestination=True,
                         filterClosedBeforeCapture=True, originalLabels=labels(), restoredLabels=labels(),
                         attempt=dict(ok=True, refusals=3, file=F.LIST, tag="SPAN", href=None,
                                      text=F.LIST_LABEL, epoch="4", calendarOpen=True, activation="outside-dom-click"),
                         native=dict(ok=True, refusals=3, notice="", calendarOpen=False,
                                     pending="false", epoch="5"))
        if stage == "composition-refused":
            value = dict(attempt=dict(ok=True, refusals=3, value=""), observedMs=150,
                         activation=dict(ok=True, tag="SPAN", text=F.LIST_LABEL),
                         originalLabels=labels(), inert=labels(True))
        return value

    def validate(self, stage, value):
        F.validate_refusal(stage, value, [1280, 800], dict(require=require, rectangle=lambda r, v: None))

    def test_four_actual_refusal_shapes(self):
        for stage in ("filter-refused", "search-refused", "composition-refused", "calendar-refused"):
            self.validate(stage, self.exercise(stage))

    def test_native_buffer_cannot_be_restored_while_claiming_refusal(self):
        for stage in ("filter-refused", "search-refused"):
            value = self.exercise(stage)
            value["native"]["value"] = ""
            with self.assertRaises(RuntimeError):
                self.validate(stage, value)

    def test_calendar_cannot_be_claimed_as_visible_pointer_activation(self):
        value = self.exercise("calendar-refused")
        value["attempt"]["activation"] = "pointer-click"
        with self.assertRaises(RuntimeError):
            self.validate("calendar-refused", value)

    def test_composition_cannot_activate_a_live_anchor(self):
        value = self.exercise("composition-refused")
        value["activation"]["tag"] = "A"
        with self.assertRaises(RuntimeError):
            self.validate("composition-refused", value)

    def test_calendar_dismissal_cannot_activate_or_leave_popup_open(self):
        for key, replacement in (("refusals", 4), ("calendarOpen", True), ("pending", "true"),
                                 ("epoch", "4"), ("notice", "Refused")):
            value = self.exercise("calendar-refused")
            value["native"][key] = replacement
            with self.assertRaises(RuntimeError):
                self.validate("calendar-refused", value)

    def test_missing_original_counter_or_unacknowledged_refusal_rejected(self):
        for key, replacement in (("ok", False), ("refusals", 3), ("notice", "")):
            value = self.exercise("filter-refused")
            value["native"][key] = replacement
            with self.assertRaises(RuntimeError):
                self.validate("filter-refused", value)


class FileEntryTests(unittest.TestCase):
    def test_only_original_document_revision_can_advance_at_entry(self):
        draft = dict(account="synthetic", channel="C1", epoch=3,
                     documents=[dict(id=1, revision=4, title="Dirty", dirty=True, pending=None)])
        admitted = copy.deepcopy(draft)
        admitted["documents"][0]["revision"] = 7
        def validate(value):
            entry = dict(draft=draft, admitted=value, revisionAdvanced=True)
            F.validate_entry(entry, draft, value, require)
        validate(admitted)
        for key, replacement in (("revision", 4), ("title", "Replacement"), ("id", 2), ("dirty", False)):
            value = copy.deepcopy(admitted)
            value["documents"][0][key] = replacement
            with self.assertRaises(RuntimeError):
                validate(value)
        for key, replacement in (("channel", "C2"), ("epoch", 4), ("account", "other")):
            value = copy.deepcopy(admitted)
            value[key] = replacement
            with self.assertRaises(RuntimeError):
                validate(value)


if __name__ == "__main__":
    unittest.main()
