"""Independent custody checks for overlapping Activity channel/List navigation."""
import re
from pathlib import Path
from urllib.parse import parse_qsl, urlsplit

import workflow_activity_files as F

STAGES = {
    "mixed-navigation": ("about", "draft", "detail", "channel-held", "list-failed",
                         "late-channel-failed", "back"),
    "mixed-navigation-reverse": ("about", "draft", "detail", "list-held", "channel-failed",
                                 "late-list-failed", "back"),
    "mixed-navigation-success": ("about", "draft", "detail", "channel-held", "list-failed",
                                 "late-channel-success", "back"),
}
MISSING = "CACTMISS01"
ACCOUNT = "synthetic-workflow-activity-file"
CHANNEL_OFFLINE = "Connect to Slack to open this conversation."
LIST_OFFLINE = "Connect to Slack to open this List."
CHANNEL_OPENING = "Opening conversation\u2026"
CHANNEL_SUCCESS = {
    "ok": True,
    "channel": {"id": MISSING, "name": "activity-fetched", "is_channel": True,
                "is_member": True, "is_archived": False, "is_private": False},
}


def counts(case, index):
    reverse = case == "mixed-navigation-reverse"
    files = (0, 0, 2, 3 if reverse else 2, 3, 3, 3)[index]
    channels = (0, 0, 0, 0 if reverse else 1, 1, 1, 1)[index]
    return files, channels


def file_request(row, serial, require):
    """Validate held-malformed bytes without rewriting the submitted receipt."""
    if row.get("mode") != "held-malformed":
        F.validate_request(row, serial, require)
        return
    require(row.get("serial") == serial and serial == 3
            and row.get("id") == F.LIST and row.get("purpose") == "navigation",
            "Held malformed file request lost original identity")
    require(row.get("method") == "files.info" and row.get("httpMethod") == "POST"
            and row.get("fields") == [["file", F.LIST]]
            and row.get("args") == {"file": F.LIST},
            "Held file used a different request factory")
    url = urlsplit(row.get("url", ""))
    require(url.scheme == "https" and url.netloc == "example.test"
            and url.path == "/api/files.info" and not url.fragment,
            "Held file escaped the original endpoint")
    pairs = parse_qsl(url.query, keep_blank_values=True)
    query = dict(pairs)
    require(len(pairs) == len(query) and set(query) == {
        "_x_id", "_x_csid", "_x_desktop_ia", "_x_foreground", "_x_frontend_build_type",
        "_x_gantry", "_x_num_retries", "_x_version_ts", "fp", "slack_route",
    }, "Held file request query changed")
    require(all(re.fullmatch("[0-9a-f]{32}", query[key])
                for key in ("_x_id", "_x_csid", "fp"))
            and all(query[key] == "true" for key in
                    ("_x_desktop_ia", "_x_foreground", "_x_gantry"))
            and query["_x_num_retries"] == "0" and query["slack_route"] == "default"
            and query["_x_frontend_build_type"] == "current" and query["_x_version_ts"],
            "Held file request trace or retry policy changed")
    require(row.get("raw") == {"ok": True, "file": False}
            and row.get("responseText") == '{"ok":true,"file":false}',
            "Held file did not retain its original malformed bytes")


def validate_files(value, case, index, require):
    count, _ = counts(case, index)
    reverse = case == "mixed-navigation-reverse"
    pending = reverse and index in (3, 4)
    require(isinstance(value, dict) and value.get("originals") == F.ORIGINALS
            and value.get("networkCalls") == 0 and value.get("httpCapability") is False,
            "Mixed file source or closed transport changed")
    requests = value.get("requests")
    require(isinstance(requests, list) and len(requests) == count,
            "Mixed file reads repeated or disappeared")
    for serial, request in enumerate(requests, 1):
        file_request(request, serial, require)
    if count:
        require({r.get("id") for r in requests[:2]} == {F.LIST, F.CANVAS}
                and all(r.get("purpose") == "metadata" and r.get("mode") == "success"
                        for r in requests[:2]),
                "Duplicate file occurrences did not share two original metadata reads")
    if count == 3:
        require(requests[2].get("id") == F.LIST
                and requests[2].get("purpose") == "navigation"
                and requests[2].get("mode") == ("held-malformed" if reverse else "offline"),
                "Mixed file navigation outcome or purpose changed")
    completed = count - int(pending)
    require(value.get("completed") == completed and value.get("cancelled") == 0
            and value.get("settlements") == [
                dict(serial=n, returned=True, cancelled=False)
                for n in range(1, completed + 1)]
            and value.get("held") == (3 if pending else None),
            "Original file callback did not settle exactly once")
    releases = [dict(serial=3, receiverRetired=False, delivered=True)] if reverse and index >= 5 else []
    require(value.get("releases") == releases,
            "Late file reply did not reach its original receiver exactly once")
    return requests


def validate_canonical(value, index, require):
    if index == 0:
        require(value is None, "File catalog installed before workflow entry")
        return
    require(isinstance(value, dict), "Missing actual file catalog")
    entries = value.get("entries")
    if index == 1:
        require(entries == [] and value.get("pendingFile") is None,
                "File reads occurred before actual Activity detail")
        return
    require(isinstance(entries, list) and len(entries) == 2
            and {entry.get("id") for entry in entries} == {F.LIST, F.CANVAS}
            and value.get("pendingFile") is None,
            "Original file catalog lost its two ready observations")
    for entry in entries:
        raw = F.ORIGINALS["list" if entry["id"] == F.LIST else "canvas"]
        require(entry.get("phase") == "ready" and entry.get("current") is True
                and entry.get("source") == raw["file"]
                and entry.get("receipt") == raw and entry.get("response") == raw
                and all(entry.get("effective", {}).get(key) == raw["file"][key]
                        for key in ("id", "mode", "filetype", "title", "name", "permalink")),
                "Mixed navigation fabricated or retired the canonical file title")


def geometry(value, viewport, h):
    require = h["require"]
    require(isinstance(value, dict) and value.get("ok") is True
            and value.get("noOverflow") is True and value.get("viewport") == viewport,
            "Missing actual mixed navigation layout")
    controls, notices = value.get("controls"), value.get("notices")
    require(isinstance(controls, list) and controls and isinstance(notices, list),
            "Missing original mixed controls or notice inventory")
    for control in controls + notices:
        require(control.get("visible") is True, "Mixed navigation feedback/control is clipped")
        h["rectangle"](control.get("rect"), viewport)
    return notices


def validate_channel(value, case, index, require):
    _, count = counts(case, index)
    reverse = case == "mixed-navigation-reverse"
    pending = not reverse and index in (3, 4)
    require(isinstance(value, dict) and isinstance(value.get("ledger"), dict),
            "Missing original channel endpoint and selection")
    ledger = value["ledger"]
    require(ledger.get("networkCalls") == 0
            and ledger.get("requests") == [
                dict(method="conversations.info",
                     args=dict(channel=MISSING, include_num_members="false"))] * count
            and ledger.get("aboutRequests") == []
            and ledger.get("completed") == count - int(pending)
            and ledger.get("cancelled") == 0 and ledger.get("held") is pending,
            "Channel request, held receiver or selected-About read changed")
    response = (dict(serial=1, raw=None, error="offline", cancelled=False) if reverse else
                dict(serial=1, raw=CHANNEL_SUCCESS if case.endswith("-success")
                     else dict(ok=True, channel=False), error=None, cancelled=False))
    require(ledger.get("responses") == ([response] if count and not pending else []),
            "Channel completion lost its original raw response or Offline result")
    require(isinstance(ledger.get("original"), str) and ledger["original"]
            and ledger["original"] not in (MISSING, "CACTKNOWN01")
            and value.get("selected") == ledger["original"],
            "An old navigation selected a destination")
    require(type(value.get("missingPresent")) is bool
            and (not value["missingPresent"] or case.endswith("-success") and index >= 5),
            "A failed read installed the missing channel")
    source = value.get("sourceRows")
    ids = {ledger["original"], "CACTKNOWN01"} | ({MISSING} if value["missingPresent"] else set())
    require(isinstance(source, list) and len(source) == len(ids)
            and {r.get("id") for r in source} == ids,
            "Mixed navigation changed the original channel source inventory")
    if value["missingPresent"]:
        fetched = next(r for r in source if r["id"] == MISSING)
        require(fetched == dict(id=MISSING, name="activity-fetched", isMember=True, isOpen=None,
                                unreadCount=None, unreadDisplay=None, mentionCount=None, hasUnreads=False),
                "Late successful channel publication invented source fields")
    return ledger


def validate_messages(model, require):
    event = model.get("activity", {}).get("detail", {}).get("workflow_event", {})
    require(event.get("workflow_id") == "WF_REVIEW"
            and event.get("workflow_execution_id") == "E_REVIEW"
            and event.get("trigger_message") == {
                "template": "Open {list} then {list}",
                "variables": {"list": {"type": "LIST", "value": F.LIST}}},
            "Original mixed trigger message changed")
    steps = event.get("step_events")
    require(isinstance(steps, list) and len(steps) == 2
            and [s.get("step_execution_id") for s in steps] == ["STEP_REVIEW", "STEP_MIXED_CANVAS"]
            and steps[0].get("outcome_message") == {
                "template": "Check {missing}",
                "variables": {"missing": {"type": "CHANNEL", "value": MISSING}}}
            and steps[1].get("outcome_message") == {
                "template": "Read {canvas}",
                "variables": {"canvas": {"type": "CANVAS", "value": F.CANVAS}}}
            and all(s.get("future") == {"preserved": 17} for s in steps),
            "Original mixed source rows or opaque fields changed")


def validate_labels(value, fetched, viewport, h):
    require = h["require"]
    require(isinstance(value, dict) and value.get("ok") is True
            and value.get("measurement") == "individual-scroll-samples",
            "Missing measured original mixed occurrences")
    expected = [
        ("workflow-activity-file-trigger-1", "list", "A", F.LIST, F.LIST_LABEL, F.LIST_URL),
        ("workflow-activity-file-trigger-3", "list", "A", F.LIST, F.LIST_LABEL, F.LIST_URL),
        ("workflow-activity-channel-step-0-1", "channel", "BUTTON", MISSING,
         "activity-fetched" if fetched else "Channel unavailable", None),
        ("", "canvas", "SPAN", F.CANVAS, F.CANVAS_LABEL, None),
    ]
    rows = value.get("rows")
    require(isinstance(rows, list) and len(rows) == 4, "Missing or duplicate mixed occurrences")
    for row, identity in zip(rows, expected):
        require(tuple(row.get(k) for k in ("id", "kind", "tag", "identity", "text", "href")) == identity
                and row.get("ok") is True and row.get("visible") is True,
                "Mixed occurrence identity, title or action boundary changed")
        h["rectangle"](row.get("rect"), viewport)


def validate_native(value, case, index, model, custody, viewport, h):
    require = h["require"]
    require(isinstance(value, dict) and value.get("ok") is True
            and value.get("originalNodes") is True,
            "Mixed navigation replaced the original native nodes")
    scope, buffers = value.get("scope", {}), value.get("buffers", {})
    doc = custody["documents"][0]
    require(all(isinstance(scope.get(k), str) and scope[k].isdigit()
                for k in ("visit", "document", "revision", "detail"))
            and scope["visit"] == str(model["metadata"]["visit"])
            and scope["document"] == str(doc["id"])
            and scope["revision"] == str(doc["revision"]),
            "Mixed notice borrowed another workflow visit or document")
    search, date = buffers.get("search", {}), buffers.get("date", {})
    require(search.get("value") == "" and search.get("start") == search.get("end") == 0
            and isinstance(search.get("generation"), str) and search["generation"].isdigit()
            and buffers.get("status") == buffers.get("user") == ""
            and buffers.get("limit") == "100",
            "Mixed navigation changed original native filter/search buffers")
    require(set(date) == {"from", "to", "revision", "epoch", "open", "dirty", "pending",
                          "origin", "defaultFrom", "defaultTo"}
            and all(date[k] == "" for k in ("from", "to", "origin", "defaultFrom", "defaultTo"))
            and all(date[k] == "false" for k in ("open", "dirty", "pending"))
            and date["revision"] == str(model["activity"]["filters"]["revision"])
            and isinstance(date["epoch"], str) and date["epoch"].isdigit(),
            "Mixed navigation changed original date-range authority")
    native = value.get("native", {})
    require(native == dict(composing=False, leaseAbsent=True, refusals=0),
            "Mixed read held a native lease or changed original composition/refusal state")
    reverse = case == "mixed-navigation-reverse"
    owner = value.get("owner")
    text = "" if index == 2 or reverse and index == 3 else (
        CHANNEL_OPENING if index == 3 else CHANNEL_OFFLINE if reverse else LIST_OFFLINE)
    notice = value.get("notice", {})
    require(notice.get("text") == text and notice.get("role") == "status"
            and notice.get("count") == int(bool(text)),
            "Mixed navigation notice is stale, duplicated or missing")
    if text:
        require(notice.get("visible") is True, "Current mixed navigation notice is hidden")
        h["rectangle"](notice.get("rect"), viewport)
    if index == 2:
        require(owner is None, "Detail started with invented navigation ownership")
        return
    kind = ("list" if reverse else "channel") if index == 3 else (
        "channel" if reverse else "list")
    phase = ("admitted" if reverse else "pending") if index == 3 else "failure"
    require(isinstance(owner, dict) and type(owner.get("chain")) is int and owner["chain"] > 0
            and isinstance(owner.get("key"), str) and owner["key"]
            and owner.get("detail") == scope["detail"] and owner.get("phase") == phase,
            "Mixed notice does not retain its actual attempt phase")
    require(owner.get("button") == ("workflow-activity-file-trigger-1" if kind == "list"
                                   else "workflow-activity-channel-step-0-1")
            and owner.get("file") == (F.LIST if kind == "list" else None)
            and owner.get("channel") == (MISSING if kind == "channel" else None),
            "Mixed notice belongs to the wrong original occurrence")


def validate_late(exercise, proof, case, viewport, h):
    require = h["require"]
    require(isinstance(exercise, dict) and exercise.get("observedMs") == 500,
            "Missing bounded observation after the actual late reply")
    before, after = exercise.get("before"), exercise.get("after")
    require(isinstance(before, dict) and isinstance(after, dict),
            "Missing raw before/after late settlement observations")
    for key in ("model", "custody", "native"):
        require(key in before and before[key] == after.get(key),
                "Late completion changed current " + key)
    for key in ("model", "channel", "files", "canonical", "custody", "native"):
        require(after.get(key) == proof.get(key), "Late observation differs from captured " + key)
    reverse = case == "mixed-navigation-reverse"
    release = (dict(serial=3, receiverRetired=False, delivered=True) if reverse else
               dict(serial=1, requestCount=1, delivered=True, cancelledBeforeReply=False,
                    receiverRetired=False,
                    raw=CHANNEL_SUCCESS if case.endswith("-success") else dict(ok=True, channel=False)))
    require(exercise.get("release") == release,
            "Late reply was replaced, cancelled or not delivered to its original receiver")
    validate_files(before.get("files"), case, 4, require)
    validate_channel(before.get("channel"), case, 4, require)
    validate_native(before.get("native"), case, 4, before["model"], before["custody"], viewport, h)
    require(before["files"]["requests"] == after["files"]["requests"]
            and before["channel"]["ledger"]["requests"] == after["channel"]["ledger"]["requests"],
            "Late completion replayed or rewrote a request")
    for key in ("selected", "generation", "documents", "aboutVisit", "managementVisit"):
        require(before["channel"].get(key) == after["channel"].get(key),
                "Late completion changed the original channel/management route")
    require(exercise.get("cachePublication") == dict(
        before=before["channel"]["sourceRows"], after=after["channel"]["sourceRows"],
        beforeMissing=before["channel"]["missingPresent"], afterMissing=after["channel"]["missingPresent"]),
        "Late cache publication proof differs from the actual original source")


def validate(fixture, receipt, captures, case, h, *, platform, png=True):
    require = h["require"]
    require(case in STAGES and receipt.get("ok") is True
            and receipt.get("fixture") == fixture and receipt.get("case") == case,
            "Wrong or failed mixed navigation receipt")
    require(all(receipt.get(k) is True for k in ("actualActivityEntry", "aboutConfiguration",
                                                "draftRetained", "readOnly", "noLiveNetwork"))
            and receipt.get("networkCalls") == 0, "Missing actual mixed entry or closed transport")
    viewport = h["expected_viewport"](fixture, platform)
    require(receipt.get("viewport") == viewport, "Wrong mixed viewport")
    rows, paths = receipt.get("stages"), receipt.get("captures")
    require(isinstance(rows, list) and [r.get("stage") for r in rows] == list(STAGES[case])
            and isinstance(paths, dict) and set(paths) == set(STAGES[case]),
            "Mixed stage/capture inventory changed")
    original = detail_model = previous_files = previous_channels = previous_native = source_baseline = None
    for index, row in enumerate(rows):
        stage, model, proof = (row.get(k) for k in ("stage", "model", "proof"))
        require(isinstance(model, dict) and isinstance(proof, dict), "Missing actual mixed stage state")
        require(proof.get("model") == model, "Captured model differs from original observation")
        metadata, activity = model.get("metadata", {}), model.get("ledger", {})
        require(model.get("account") == ACCOUNT and metadata.get("writes") == 0
                and metadata.get("networkCalls") == activity.get("networkCalls") == 0
                and metadata.get("pending") is False and metadata.get("busy") is False,
                "Mixed navigation wrote metadata or escaped its closed account")
        custody = proof.get("custody", {})
        require(custody.get("account") == ACCOUNT and custody.get("epoch") == model.get("epoch"),
                "Mixed draft custody belongs to another account")
        if index == 0:
            require(custody.get("documents") == [], "Mixed workflow draft was preinstalled")
        else:
            docs = custody.get("documents")
            require(isinstance(docs, list) and len(docs) == 1, "Mixed navigation lost original document")
            doc = docs[0]
            require(doc.get("title") == metadata.get("title") == h["TITLE"]
                    and doc.get("description") == metadata.get("description") == h["DESCRIPTION"]
                    and doc.get("dirty") is True and metadata.get("dirty") is True
                    and doc.get("pending") is None
                    and all(k in doc for k in ("workflow", "trigger", "preferences",
                                              "copyPermissions", "runPermissions", "restriction")),
                    "Mixed navigation lost the exact unsaved workflow draft")
            if index == 1:
                original = custody
            elif index == 2:
                F.validate_entry(proof.get("exercise", {}).get("entry"), original, custody, require)
                original = custody
            require(custody == original, "Mixed navigation changed original document ownership")
        calls = activity.get("requests")
        h["validate_requests"](calls)
        require(len(calls) == (0 if index < 2 else 2)
                and activity.get("finished") == len(calls)
                and activity.get("cancelled") == 0 and activity.get("held") is None,
                "Mixed navigation reread or cancelled Activity")
        files = validate_files(proof.get("files"), case, index, require)
        channel = validate_channel(proof.get("channel"), case, index, require)
        source = {r["id"]: r for r in proof["channel"]["sourceRows"]}
        if source_baseline is None:
            source_baseline = source
        require(all(source.get(identity) == raw for identity, raw in source_baseline.items()),
                "Mixed navigation rewrote original channel rows")
        require(previous_files is None or files[:len(previous_files)] == previous_files,
                "Original file request history was rewritten")
        require(previous_channels is None or channel["requests"][:len(previous_channels)] == previous_channels,
                "Original channel request history was rewritten")
        previous_files, previous_channels = files, channel["requests"]
        require(proof["channel"]["selected"] == custody.get("channel")
                and proof["channel"]["generation"] == custody.get("channelGeneration"),
                "Late navigation changed selected conversation or generation")
        validate_canonical(proof.get("canonical"), index, require)
        if index:
            require(proof["canonical"].get("navigating") ==
                    (F.LIST if case == "mixed-navigation-reverse" and index in (3, 4) else None),
                    "Original held file attempt disappeared or leaked after completion")
        F.validate_destination(proof.get("destination"), False, stage, require)
        notices = geometry(row.get("geometry"), viewport, h)
        if index in (1, 6):
            require(row["geometry"].get("fields") == dict(title=h["TITLE"], description=h["DESCRIPTION"],
                                                          titleReadonly=False, descriptionReadonly=False),
                    "Native workflow fields changed on return")
        if 2 <= index <= 5:
            validate_messages(model, require)
            if index == 2:
                detail_model = model
            require(model == detail_model, "Overlapping navigation changed original Activity state")
            validate_labels(proof.get("labels"), proof["channel"]["missingPresent"], viewport, h)
            validate_native(proof.get("native"), case, index, model, custody, viewport, h)
            current = proof["native"]
            if previous_native is not None:
                require(all(current[k] == previous_native[k] for k in ("scope", "buffers", "native")),
                        "Overlapping navigation replaced original native buffers")
                if index == 4:
                    require(current["owner"]["chain"] > previous_native["owner"]["chain"],
                            "Newer action did not acquire newer shared notice ownership")
                if index == 5:
                    require(current == previous_native, "Late completion overwrote newer native feedback")
            previous_native = current
            text = current["notice"]["text"]
            require([n.get("text") for n in notices if n.get("text")] == ([text] if text else []),
                    "Layout does not show exactly the current mixed notice")
        else:
            require(proof.get("native") is None and proof.get("labels") is None,
                    "Mixed native ownership claimed outside original detail")
        if index == 5:
            combined = dict(proof, model=model)
            validate_late(proof.get("exercise"), combined, case, viewport, h)
        if index == 3:
            exercise = proof.get("exercise", {})
            before, after = exercise.get("before"), exercise.get("after")
            require(isinstance(before, dict) and before == after and exercise.get("observedMs") == 500
                    and all(after.get(k) == proof.get(k)
                            for k in ("model", "channel", "files", "canonical", "custody", "native")),
                    "Held observation changed native/source state or admitted additional work")
        if index == 6:
            require(proof.get("exercise", {}).get("originalCustody") == original,
                    "Back lost the original admitted draft comparison")
        dimensions = row["geometry"].get("pngDimensions")
        require(isinstance(dimensions, list) and len(dimensions) == 2
                and all(type(n) is int and n > 0 for n in dimensions), "Missing actual PNG dimensions")
        expected_path = captures / f"{fixture}-{stage}.png"
        require(Path(paths[stage]) == expected_path, "Foreign mixed screenshot")
        if png:
            h["validate_png"](expected_path, dimensions)
    require(receipt.get("final") == rows[-1]["model"]
            and all(receipt.get(k) == rows[-1]["proof"].get(k)
                    for k in ("files", "channel", "destination")),
            "Mixed terminal state or endpoint ledger changed after capture")
    return receipt
