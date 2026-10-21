"""Independent evidence checks for ordinary Activity file entities and native Lists."""
import copy
import json
import re
from pathlib import Path
from urllib.parse import parse_qsl, urlsplit

LIST = "FCANON003"
CANVAS = "FCANON002"
LIST_LABEL = "Révision & 中 List"
CANVAS_LABEL = "Canvas & café 中"
LIST_URL = "https://example.test/files/FCANON003/original?view=retained"
ROW = "Original Activity List destination row"
STAGES = {
    "file": ("about", "draft", "detail", "filter-refused", "search-refused",
             "composition-refused", "calendar-refused",
             "malformed", "wrong-id", "unsupported", "deleted", "offline",
             "destination", "returned", "back"),
    "file-retirement": ("about", "draft", "detail", "held", "retired",
                        "late-refused", "destination", "returned", "back"),
}
STAGES["file-emoji"] = STAGES["file"][:3] + ("label-refused",) + STAGES["file"][3:]
STAGES["file-emoji-retirement"] = STAGES["file-retirement"]
STAGES["file-emoji-cold"] = STAGES["file"]
STAGES["file-encoded"] = STAGES["file"]
BASE_FILE = dict(created=101, updated=102, is_deleted=False,
                opaque={"keep": [False, 0, None]})
LIST_RAW = dict(
    ok=True,
    file=dict(BASE_FILE, id=LIST, mode="list", filetype="list",
              title="Révision &amp; 中 List", name="Original List name is not its title",
              permalink=LIST_URL, access="read",
              list_metadata=dict(schema=[dict(id="C_ACTIVITY_NAME", key="NAME", name="Item",
                                              type="text", is_primary_column=True)], views=[])),
    comments=[], paging=dict(page=1),
)
CANVAS_RAW = dict(
    ok=True,
    file=dict(BASE_FILE, id=CANVAS, mode="quip", filetype="quip", created=103, updated=104,
              title="Canvas &amp; café 中", name="Original Canvas name is not its title",
              permalink="https://example.test/docs/FCANON002"),
    comments=[], paging=dict(page=1),
)
ORIGINALS = dict(list=LIST_RAW, canvas=CANVAS_RAW)


def expected_response(file_id, mode, *, originals=ORIGINALS):
    value = copy.deepcopy(originals["list" if file_id == LIST else "canvas"])
    if mode == "offline":
        return None
    if mode == "malformed":
        return dict(ok=True, file=False)
    if mode == "wrong-id":
        value["file"]["id"] = "FFOREIGN01"
        value["file"]["permalink"] = value["file"]["permalink"].replace(LIST, "FFOREIGN01")
    if mode == "unsupported":
        value["file"].update(mode="hosted", filetype="pdf")
    if mode == "deleted":
        value["file"]["is_deleted"] = True
    return value


def validate_request(row, serial, require, *, metadata_modes=("success",), originals=ORIGINALS):
    require(isinstance(row, dict) and row.get("serial") == serial,
            "File request serial changed")
    file_id, purpose, mode = (row.get(k) for k in ("id", "purpose", "mode"))
    require(file_id in (LIST, CANVAS) and purpose in ("metadata", "navigation"),
            "Foreign file or purpose")
    require(purpose != "navigation" or file_id == LIST,
            "Static Canvas unexpectedly acquired navigation")
    require(mode in ("success", "malformed", "wrong-id", "unsupported", "deleted", "offline", "held")
            and (purpose != "metadata" or mode in metadata_modes),
            "Unexpected file fixture mode")
    require(row.get("method") == "files.info" and row.get("httpMethod") == "POST"
            and row.get("fields") == [["file", file_id]]
            and row.get("args") == dict(file=file_id), "Wrong original file request")
    url = urlsplit(row.get("url", ""))
    require(url.scheme == "https" and url.netloc == "example.test"
            and url.path == "/api/files.info" and not url.fragment, "Foreign file endpoint")
    pairs = parse_qsl(url.query, keep_blank_values=True)
    values = dict(pairs)
    require(len(pairs) == len(values) and set(values) == {
        "_x_id", "_x_csid", "_x_desktop_ia", "_x_foreground", "_x_frontend_build_type",
        "_x_gantry", "_x_num_retries", "_x_version_ts", "fp", "slack_route",
    }, "Wrong or duplicate file endpoint query")
    require(all(re.fullmatch("[0-9a-f]{32}", values[key]) for key in ("_x_id", "_x_csid", "fp"))
            and all(values[key] == "true" for key in ("_x_desktop_ia", "_x_foreground", "_x_gantry"))
            and values["_x_num_retries"] == "0" and values["slack_route"] == "default"
            and values["_x_frontend_build_type"] == "current" and values["_x_version_ts"],
            "File endpoint trace or request policy changed")
    expected = expected_response(file_id, mode, originals=originals)
    require(row.get("raw") == expected, "Original file response changed")
    if expected is None:
        require(row.get("responseText") is None, "Offline read fabricated received bytes")
    else:
        try:
            received = json.loads(row.get("responseText"))
        except (ValueError, TypeError):
            require(False, "Original file response bytes missing")
        require(received == expected, "File raw envelope differs from original bytes")


def validate_ledger(value, require, *, metadata_modes=("success",), originals=ORIGINALS):
    require(isinstance(value, dict) and value.get("networkCalls") == 0
            and value.get("httpCapability") is False, "File fixture is not closed")
    require(value.get("originals") == originals, "Original literal file fixtures changed")
    requests, settlements = value.get("requests"), value.get("settlements")
    require(isinstance(requests, list) and isinstance(settlements, list),
            "Missing file request/settlement ledger")
    for serial, row in enumerate(requests, 1):
        validate_request(row, serial, require, metadata_modes=metadata_modes, originals=originals)
    require(value.get("completed") == len(settlements)
            and value.get("cancelled") == sum(s.get("cancelled") is True for s in settlements),
            "File settlement count changed")
    serials = [s.get("serial") for s in settlements]
    require(len(set(serials)) == len(serials)
            and all(type(n) is int and 1 <= n <= len(requests) for n in serials),
            "File callback duplicated or fabricated")
    for row in settlements:
        require(type(row.get("returned")) is bool and type(row.get("cancelled")) is bool
                and row["returned"] is not row["cancelled"], "File settlement lost original outcome")
    held = value.get("held")
    require(held is None or type(held) is int and 1 <= held <= len(requests)
            and requests[held - 1]["mode"] == "held", "Wrong held file owner")
    releases = value.get("releases")
    require(isinstance(releases, list), "Missing held file release ledger")
    require(len({r.get("serial") for r in releases}) == len(releases), "Duplicate held file release")
    for release in releases:
        serial = release.get("serial")
        require(type(serial) is int and 1 <= serial <= len(requests)
                and requests[serial - 1]["mode"] == "held"
                and type(release.get("receiverRetired")) is bool
                and type(release.get("delivered")) is bool
                and release["receiverRetired"] is not release["delivered"],
                "Missing actual original held receiver outcome")
    return requests


def validate_labels(value, viewport, h, *, inert=False, emoji=False, cold=False, encoded=False):
    if encoded:
        from workflow_activity_file_encoded import LIST_TEXT, CANVAS_TEXT, validate_literal
    elif emoji:
        from workflow_activity_file_emoji import LIST_TEXT, CANVAS_TEXT, validate_paint
        if cold:
            from workflow_activity_file_emoji import LIST_COLD_TEXT as LIST_TEXT
    else:
        LIST_TEXT, CANVAS_TEXT = LIST_LABEL, CANVAS_LABEL
    require, rectangle = h["require"], h["rectangle"]
    require(isinstance(value, dict) and value.get("ok") is True
            and value.get("inert") is inert and value.get("measurement") == "individual-scroll-samples"
            and isinstance(value.get("detailRevision"), str) and value["detailRevision"].isdigit(),
            "Missing measured original file occurrences")
    rows = value.get("rows")
    expected = [
        ("workflow-activity-trigger-message", 0, LIST, "list", "1", LIST_TEXT),
        ("workflow-activity-trigger-message", 1, LIST, "list", "3", LIST_TEXT),
        ("workflow-activity-step-message-0", 0, CANVAS, "canvas", "1", CANVAS_TEXT),
    ]
    require(isinstance(rows, list) and len(rows) == 3, "Missing or duplicate file occurrence")
    for row, identity in zip(rows, expected):
        require(tuple(row.get(k) for k in ("container", "ordinal", "file", "kind", "part", "text")) == identity,
                "Original file occurrence identity or canonical text changed")
        anchor = identity[3] == "list" and not inert
        require(row.get("ok") is True and row.get("visible") is True and row.get("inline") is True
                and row.get("interactive") is anchor
                and row.get("name") == identity[-1]
                and row.get("tag") == ("A" if anchor else "SPAN")
                and row.get("id") == ("workflow-activity-file-trigger-" + identity[4] if anchor else "")
                and row.get("href") == (LIST_URL if anchor else None),
                "File label lost its exact native action/display boundary")
        icon = row.get("icon", {})
        require(icon.get("material") is True and icon.get("pathMatches") is True
                and icon.get("ariaHidden") == "true" and icon.get("viewBox") == "0 -960 960 960"
                and icon.get("fill") == "currentColor" and icon.get("visible") is True,
                "File icon is not the measured official Material symbol")
        for rect in (row.get("rect"), row.get("nameRect"), icon.get("rect")):
            rectangle(rect, viewport)
        a, b = icon["rect"], row["nameRect"]
        require(a["right"] <= b["left"] + 1 and abs(a["top"] + a["bottom"] - b["top"] - b["bottom"]) <= 6,
                "File name and icon overlap or lose alignment")
        if emoji:
            validate_paint(row, viewport, h, cold=cold)
        if encoded:
            validate_literal(row, h["require"])
    if emoji and not cold:
        require(rows[0]["fragments"][3]["image"]["src"] == rows[1]["fragments"][3]["image"]["src"],
                "Duplicate List occurrences did not share the original emoji asset")


def validate_destination(value, opened, stage, require, *, originals=ORIGINALS):
    require(isinstance(value, dict) and value.get("networkCalls") == 0
            and value.get("httpCapability") is False and value.get("fixtureView") is False
            and value.get("mutations") == 0 and value.get("commentsOpen") is False,
            "List destination used live capability, seeded view, comments or mutations")
    requests, responses, settlements = (value.get(k) for k in ("requests", "responses", "settlements"))
    count = int(opened)
    require(all(isinstance(v, list) and len(v) == count for v in (requests, responses, settlements))
            and value.get("completed") == count, "List actual read missing or repeated")
    require(value.get("overlay") is (stage == "destination"), "Wrong native List visibility")
    if not opened:
        require(value.get("loaded") is None, "List destination installed before accepted navigation")
        return
    request, response, settled = requests[0], responses[0], settlements[0]
    require(request.get("serial") == response.get("serial") == settled.get("serial") == 1
            and request.get("id") == LIST
            and all(type(request.get(k)) is int and request[k] > 0 for k in ("generation", "read"))
            and type(request.get("revision")) is int and request["revision"] >= 0
            and settled.get("current") is True and settled.get("parsed") is True
            and settled.get("refreshAgain") is False, "List read did not settle its original parser attempt")
    expected = [
        ("files.info", [["file", LIST]]),
        ("lists.records.list", [["list_id", LIST], ["include_subtasks", "true"], ["archived", "false"],
                                ["include_suggested", "false"], ["_x_reason", "get-list-records"]]),
        ("team.prefs.get", [["target_team", "TWORKFLOW"], ["_x_reason", "native-list-permission"]]),
        ("users.prefs.get", [["prefs", "task_lists,assigned_to_list"], ["_x_reason", "native-list-completion-permission"]]),
        ("experiments.getByUser", None),
    ]
    calls = request.get("requests")
    require(isinstance(calls, list) and len(calls) == 5, "List destination bypassed original request factories")
    for call, (endpoint, fields) in zip(calls, expected):
        url = urlsplit(call.get("url", ""))
        require(call.get("method") == "POST" and url.scheme == "https" and url.netloc == "example.test"
                and url.path == "/api/" + endpoint and not url.fragment, "Foreign native List endpoint")
        if fields is None:
            require(call.get("fields") in (
                [["ssb_platform", "windows"], ["ssb_release_channel", "prod"]],
                [["ssb_platform", "darwin"], ["ssb_release_channel", "prod"]],
                [["ssb_platform", "linux"], ["ssb_release_channel", "prod"]],
            ), "Wrong List experiment request")
        else:
            require(call.get("fields") == fields, "List read lost exact ordered arguments")
    records = [dict(id="R_ACTIVITY_DESTINATION", list_id=LIST, position=0,
                    fields=[dict(key="NAME", text=ROW)],
                    activity_navigation_opaque={"keep": [False, 0, None]})]
    require(response.get("responses") == [
        dict(ok=True, file=originals["list"]["file"]), dict(ok=True, list_id=LIST, records=records),
        dict(ok=True, prefs=dict(allow_lists="full")),
        dict(ok=True, prefs=dict(task_lists="[]", assigned_to_list="")),
        dict(ok=True, assignments={}),
    ], "Original List parser responses changed")
    if stage == "destination":
        loaded = value.get("loaded", {})
        require(loaded.get("id") == LIST and loaded.get("loading") is False and loaded.get("error") is None
                and loaded.get("document") == originals["list"]["file"] and loaded.get("records") == records
                and loaded.get("generation") == request["generation"] and loaded.get("read") == 0
                and type(loaded.get("revision")) is int and loaded["revision"] > request["revision"],
                "Native List did not use the actual admitted document and records")


def validate_refusal(stage, exercise, viewport, h, *, emoji=False, cold=False, encoded=False):
    if encoded:
        from workflow_activity_file_encoded import LIST_TEXT
    elif emoji:
        from workflow_activity_file_emoji import LIST_TEXT
        if cold:
            from workflow_activity_file_emoji import LIST_COLD_TEXT as LIST_TEXT
    else:
        LIST_TEXT = LIST_LABEL
    require = h["require"]
    attempt, native = exercise.get("attempt", {}), exercise.get("native", {})
    refusal = "Finish editing the current text before opening this List. Your draft is unchanged."
    count = attempt.get("refusals")
    require(attempt.get("ok") is True and type(count) is int and count >= 0,
            "File refusal lacks original native activation/counter")
    if stage == "composition-refused":
        validate_labels(exercise.get("originalLabels"), viewport, h, emoji=emoji, cold=cold, encoded=encoded)
        validate_labels(exercise.get("inert"), viewport, h, inert=True, emoji=emoji, cold=cold, encoded=encoded)
        require(attempt.get("value") == "" and exercise.get("observedMs") == 150
                and exercise.get("activation") == dict(ok=True, tag="SPAN", text=LIST_TEXT),
                "Composition did not exercise the actual inert file occurrence")
        return
    if stage == "calendar-refused":
        validate_labels(exercise.get("originalLabels"), viewport, h, emoji=emoji, cold=cold, encoded=encoded)
        validate_labels(exercise.get("restoredLabels"), viewport, h, emoji=emoji, cold=cold, encoded=encoded)
        require(exercise.get("mode") == "calendar-outside-consumed"
                and all(exercise.get(k) is True for k in
                        ("noFileRead", "noDestination", "filterClosedBeforeCapture"))
                and attempt.get("file") == LIST and attempt.get("tag") == "SPAN"
                and attempt.get("href") is None and attempt.get("text") == LIST_TEXT
                and attempt.get("calendarOpen") is True
                and attempt.get("activation") == "outside-dom-click"
                and native.get("ok") is True and native.get("refusals") == count
                and native.get("notice") == "" and native.get("calendarOpen") is False
                and native.get("pending") == "false",
                "Calendar outside click escaped capture-phase dismissal")
        epochs = [attempt.get("epoch"), native.get("epoch")]
        require(all(isinstance(n, str) and n.isdigit() for n in epochs)
                and int(epochs[1]) > int(epochs[0]), "Calendar dismissal did not retire original epoch")
    else:
        require(native.get("ok") is True and native.get("refusals") == count + 1
                and native.get("notice") == refusal, "File refusal did not reach the original scoped native notice")
        value = "ERROR" if stage == "filter-refused" else "Uncommitted native search"
        require(exercise.get("kind") == stage and attempt.get("value") == native.get("value") == value,
                "Refused file action altered native-only text/filter buffer")


def validate_entry(entry, draft, admitted, require):
    require(isinstance(entry, dict) and entry.get("draft") == draft
            and entry.get("admitted") == admitted and entry.get("revisionAdvanced") is True,
            "Missing actual Activity entry custody transition")
    before, after = draft.get("documents"), admitted.get("documents")
    require(isinstance(before, list) and isinstance(after, list) and len(before) == len(after) == 1,
            "Activity entry lost original single document")
    old, new = before[0].get("revision"), after[0].get("revision")
    require(type(old) is int and type(new) is int and 0 <= old < new,
            "Activity entry did not advance original buffer revision")
    expected = copy.deepcopy(draft)
    expected["documents"][0]["revision"] = new
    require(expected == admitted, "Activity entry changed facts beyond original buffer revision")


def validate(fixture, receipt, captures, scenario, h, *, platform, png=True):
    emoji = scenario in ("file-emoji", "file-emoji-retirement", "file-emoji-cold")
    cold = scenario == "file-emoji-cold"
    encoded = scenario == "file-encoded"
    case = "file-retirement" if scenario.endswith("retirement") else "file"
    if encoded:
        from workflow_activity_file_encoded import ORIGINALS as originals
    elif emoji:
        from workflow_activity_file_emoji import ORIGINALS as originals, validate_refused_paint
        if cold:
            from workflow_activity_file_emoji import COLD_ORIGINALS as originals
    else:
        originals = ORIGINALS
    require = h["require"]
    require(receipt.get("ok") is True, f"Native file interaction failed: {receipt.get('error')}")
    require(receipt.get("fixture") == fixture and receipt.get("case") == scenario
            and all(receipt.get(k) is True for k in
                    ("actualActivityEntry", "aboutConfiguration", "nativeListDestination", "draftRetained", "readOnly"))
            and receipt.get("networkCalls") == 0, "File fixture entry, destination or isolation missing")
    viewport = h["expected_viewport"](fixture, platform)
    rows, paths = receipt.get("stages"), receipt.get("captures")
    require(isinstance(rows, list) and [r.get("stage") for r in rows] == list(STAGES[scenario])
            and isinstance(paths, dict) and set(paths) == set(STAGES[scenario]), "Wrong file stage/capture inventory")
    original = None
    prior_files = prior_activity = []
    detail_revision = None
    failure_modes = ("malformed", "wrong-id", "unsupported", "deleted", "offline")
    for row in rows:
        stage, model, proof, geometry = (row.get(k) for k in ("stage", "model", "proof", "geometry"))
        require(isinstance(model, dict) and isinstance(proof, dict) and isinstance(geometry, dict),
                "Missing real file stage observations")
        metadata, activity, ledger = model.get("metadata", {}), model.get("activity"), model.get("ledger", {})
        require(model.get("account") == "synthetic-workflow-activity-file"
                and metadata.get("networkCalls") == ledger.get("networkCalls") == metadata.get("writes") == 0
                and metadata.get("pending") is False and metadata.get("busy") is False,
                "File action escaped its account or mutated workflow metadata")
        custody = proof.get("custody", {})
        if stage == "about":
            require(custody.get("documents") == [], "File fixture preinstalled a workflow document")
        else:
            docs = custody.get("documents")
            require(isinstance(docs, list) and len(docs) == 1, "Lost or duplicated retained workflow document")
            doc = docs[0]
            require(doc.get("title") == h["TITLE"] and doc.get("description") == h["DESCRIPTION"]
                    and doc.get("dirty") is True and doc.get("pending") is None
                    and all(k in doc for k in ("workflow", "trigger", "preferences", "copyPermissions", "runPermissions", "restriction")),
                    "List navigation lost original dirty workflow/editor facts")
            if original is None:
                original = custody
            elif stage == "detail":
                validate_entry(proof.get("exercise", {}).get("entry"), original, custody, require)
                original = custody
            require(custody == original, "File action replaced retained workflow ownership")
            require(metadata.get("title") == h["TITLE"] and metadata.get("description") == h["DESCRIPTION"]
                    and metadata.get("dirty") is True, "Visible workflow metadata changed")
        calls = ledger.get("requests")
        h["validate_requests"](calls)
        expected_activity = 0 if stage in ("about", "draft") else (
            3 if case == "file-retirement" and stage in ("destination", "returned", "back") else 2)
        require(len(calls) == expected_activity and calls[:len(prior_activity)] == prior_activity
                and ledger.get("finished") == len(calls) and ledger.get("cancelled") == 0
                and ledger.get("held") is None, "Activity detail read repeated or changed ownership")
        prior_activity = calls
        files = proof.get("files")
        file_calls = validate_ledger(files, require, originals=originals)
        require(file_calls[:len(prior_files)] == prior_files, "File request history was rewritten")
        prior_files = file_calls
        count = 0 if stage in ("about", "draft") else 2
        if case == "file":
            if stage in failure_modes:
                count += failure_modes.index(stage) + 1
            elif stage in ("destination", "returned", "back"):
                count = 8
        elif stage in ("held", "retired", "late-refused"):
            count = 3
        elif stage in ("destination", "returned", "back"):
            count = 4
        require(len(file_calls) == count, "File acquisition or navigation replayed/skipped")
        if count:
            require({r["id"] for r in file_calls[:2]} == {LIST, CANVAS}
                    and all(r["purpose"] == "metadata" for r in file_calls[:2]),
                    "Duplicate occurrences did not share exact canonical metadata reads")
            require(all(r["purpose"] == "navigation" and r["id"] == LIST for r in file_calls[2:]),
                    "List navigation used the metadata path or wrong target")
        expected_modes = list(failure_modes) + ["success"] if case == "file" else ["held", "success"]
        require([r["mode"] for r in file_calls[2:]] == expected_modes[:max(count - 2, 0)],
                "File failure/retry sequence changed")
        held = stage == "held"
        cancelled = int(case == "file-retirement" and stage in ("retired", "late-refused", "destination", "returned", "back"))
        require(files["completed"] == count - int(held) and files["cancelled"] == cancelled,
                "Original file callback was not settled exactly once")
        expected_held = 3 if stage in ("held", "retired") else None
        require(files["held"] == expected_held, "Original held file sender changed")
        canonical = proof.get("canonical", {})
        if count:
            entries = canonical.get("entries")
            require(isinstance(entries, list) and len(entries) == 2
                    and {e.get("id") for e in entries} == {LIST, CANVAS}
                    and canonical.get("pendingFile") is None
                    and canonical.get("navigating") == (LIST if held else None),
                    "Missing, duplicate or unsettled original canonical file observations")
            for entry in entries:
                raw = originals["list" if entry["id"] == LIST else "canvas"]
                effective = entry.get("effective", {})
                require(entry.get("phase") == "ready" and entry.get("current") is True
                        and entry.get("source") == raw["file"] and entry.get("receipt") == raw
                        and entry.get("response") == raw
                        and all(effective.get(key) == raw["file"][key]
                                for key in ("id", "mode", "filetype", "title", "name", "permalink")),
                        "File display was fabricated from an ID, typed row or stale canonical receipt")
        opened = stage in ("destination", "returned", "back")
        validate_destination(proof.get("destination"), opened, stage, require, originals=originals)
        require(geometry.get("ok") is True and geometry.get("noOverflow") is True
                and geometry.get("viewport") == viewport, "File geometry is missing or overflowing")
        controls, notices = geometry.get("controls"), geometry.get("notices")
        require(isinstance(controls, list) and controls and isinstance(notices, list), "Missing native file controls")
        for control in controls + notices:
            require(control.get("visible") is True, "File control or notice is clipped")
            h["rectangle"](control.get("rect"), viewport)
        if stage == "destination":
            require(geometry.get("list") == LIST
                    and ROW in geometry.get("row", "") and geometry.get("readOnly") is True,
                    "Wrong real List destination or editable target")
            from workflow_activity_list_header import validate as validate_header
            validate_header(geometry, viewport, h, emoji=emoji, cold=cold, encoded=encoded)
        elif stage in ("draft", "back"):
            require(geometry.get("fields") == dict(title=h["TITLE"], description=h["DESCRIPTION"],
                                                   titleReadonly=False, descriptionReadonly=False),
                    "Original native draft was not restored")
        detailed = stage not in ("about", "draft", "retired", "late-refused", "destination", "calendar-refused", "back")
        if detailed:
            validate_labels(proof.get("fileLabels"), viewport, h, inert=stage == "composition-refused", emoji=emoji, cold=cold, encoded=encoded)
            revision = proof["fileLabels"]["detailRevision"]
            if detail_revision is None:
                detail_revision = revision
            require(revision == detail_revision or case == "file-retirement" and stage == "returned",
                    "Original file detail occurrence changed unexpectedly")
        else:
            require(proof.get("fileLabels") is None, "File labels claimed outside captured detail")
        if stage in failure_modes:
            notice = "Connect to Slack to open this List." if stage == "offline" else "Could not open this List. It may no longer be accessible. Try again."
            require(proof.get("exercise", {}).get("notice") == notice
                    and notice in [n.get("text") for n in notices], "File failure lacks visible scoped feedback")
        exercise = proof.get("exercise", {})
        if stage == "label-refused":
            require(emoji and exercise.get("before") == model and exercise.get("filesBefore") == files
                    and exercise.get("destinationBefore") == proof.get("destination"),
                    "Mutated file title dispatched or changed original state")
            validate_refused_paint(exercise, notices, require)
        if stage in ("filter-refused", "search-refused", "composition-refused", "calendar-refused"):
            require(exercise.get("unchangedRustState") is True and exercise.get("before") == model
                    and exercise.get("filesBefore") == files, "Refused file action changed the original source")
            validate_refusal(stage, exercise, viewport, h, emoji=emoji, cold=cold, encoded=encoded)
            if stage == "calendar-refused":
                require(exercise.get("destinationBefore") == proof.get("destination"),
                        "Calendar dismissal dispatched a List read")
        if stage == "held":
            require(exercise == dict(duplicateClick=True, observedMs=150), "Duplicate held click was not exercised")
        if stage == "late-refused":
            require(exercise == dict(serial=3, receiverRetired=True, delivered=False)
                    and files["releases"] == [exercise], "Retired resolver delivered into successor")
        if opened:
            require(exercise.get("originalCustody") == original, "Destination/return lacks original draft comparison")
        if stage == "returned":
            require(exercise.get("navigationNotice") == ""
                    and all(not n.get("text") for n in notices),
                    "Successful List return retained obsolete navigation guidance")
        dimensions = geometry.get("pngDimensions")
        require(isinstance(dimensions, list) and len(dimensions) == 2
                and all(type(n) is int and n > 0 for n in dimensions), "Missing original file screenshot dimensions")
        expected_path = captures / f"{fixture}-{stage}.png"
        require(Path(paths[stage]) == expected_path, "Foreign file screenshot")
        if png:
            h["validate_png"](expected_path, dimensions)
    require(receipt.get("final") == rows[-1]["model"] and receipt.get("files") == rows[-1]["proof"]["files"]
            and receipt.get("destination") == rows[-1]["proof"]["destination"], "File terminal receipt changed after capture")
    return receipt
