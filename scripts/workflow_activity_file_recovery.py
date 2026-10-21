"""Evidence checks for cold Activity metadata recovery through original requests."""
from pathlib import Path

import workflow_activity_files as F

STAGES = {
    "file-recovery": ("about", "draft", "metadata-failed", "retry-canvas-held",
                      "canvas-recovered", "retry-list-held", "recovered", "back"),
    "file-recovery-retirement": ("about", "draft", "metadata-held", "retired",
                                 "late-refused", "recovered", "back"),
}
COUNTS = {
    "file-recovery": ((0, 0), (0, 0), (2, 2), (3, 2), (3, 3), (4, 3), (4, 4), (4, 4)),
    "file-recovery-retirement": ((0, 0), (0, 0), (1, 0), (1, 1), (1, 1), (3, 3), (3, 3)),
}
OFFLINE = "Connect to Slack to load file details. Retry file details when connected."
MALFORMED = "File details could not load. Retry file details to try again."
LOADING = "Loading file details\u2026"


def validate_files(value, case, stage, require):
    calls = F.validate_ledger(value, require,
                              metadata_modes=("success", "offline", "malformed", "held"))
    index = STAGES[case].index(stage)
    count, settled = COUNTS[case][index]
    require(len(calls) == count and value["completed"] == settled,
            "Metadata acquisition repeated, skipped or settled the wrong read")
    expected = ([(F.CANVAS, "offline"), (F.LIST, "malformed"),
                 (F.CANVAS, "held"), (F.LIST, "held")] if case == "file-recovery"
                else [(F.CANVAS, "held"), (F.CANVAS, "success"), (F.LIST, "success")])
    require([(r["id"], r["mode"]) for r in calls] == expected[:count]
            and all(r["purpose"] == "metadata" for r in calls),
            "Cold metadata used a navigation read or changed original per-file sequence")
    retired = case == "file-recovery-retirement" and stage in (
        "retired", "late-refused", "recovered", "back")
    require(value["cancelled"] == int(retired), "Original metadata receiver did not retire exactly once")
    expected_held = (3 if stage == "retry-canvas-held" else
                     4 if stage == "retry-list-held" else
                     1 if stage in ("metadata-held", "retired") else None)
    require(value["held"] == expected_held, "Held metadata sender lost original identity")
    releases = []
    if case == "file-recovery":
        if stage in ("canvas-recovered", "retry-list-held", "recovered", "back"):
            releases.append(dict(serial=3, receiverRetired=False, delivered=True))
        if stage in ("recovered", "back"):
            releases.append(dict(serial=4, receiverRetired=False, delivered=True))
    elif stage in ("late-refused", "recovered", "back"):
        releases.append(dict(serial=1, receiverRetired=True, delivered=False))
    require(value["releases"] == releases, "Metadata receiver delivery was fabricated or repeated")
    for settlement in value["settlements"]:
        cancelled = retired and settlement["serial"] == 1
        require(settlement["cancelled"] is cancelled and settlement["returned"] is not cancelled,
                "Original metadata settlement attached to a different serial")
    return calls


def phases(case, stage):
    if stage in ("about", "draft"):
        return {}
    if case == "file-recovery-retirement":
        return ({F.CANVAS: "loading"} if stage in ("metadata-held", "retired", "late-refused")
                else {F.CANVAS: "ready", F.LIST: "ready"})
    return {
        "metadata-failed": {F.CANVAS: "unavailable", F.LIST: "unavailable"},
        "retry-canvas-held": {F.CANVAS: "loading", F.LIST: "unavailable"},
        "canvas-recovered": {F.CANVAS: "ready", F.LIST: "unavailable"},
        "retry-list-held": {F.CANVAS: "ready", F.LIST: "loading"},
        "recovered": {F.CANVAS: "ready", F.LIST: "ready"},
        "back": {F.CANVAS: "ready", F.LIST: "ready"},
    }[stage]


def validate_canonical(value, case, stage, require):
    if stage == "about":
        require(value is None, "About preinstalled a workflow file metadata state")
        return
    require(isinstance(value, dict), "Missing original workflow file metadata state")
    expected = phases(case, stage)
    rows = value.get("entries", [])
    require(isinstance(rows, list) and len(rows) == len(expected)
            and {r.get("id") for r in rows} == set(expected),
            "Missing or duplicate original canonical metadata entry")
    require(value.get("navigating") is None and value.get("lastNavigation") is None,
            "Metadata Retry initiated file navigation")
    pending = next((file_id for file_id, phase in expected.items() if phase == "loading"), None)
    require(value.get("pendingFile") == pending, "Canonical pending slot changed original file")
    for row in rows:
        phase, file_id = expected[row["id"]], row["id"]
        require(row.get("phase") == phase, "Wrong canonical recovery phase")
        if phase == "ready":
            raw = F.CANVAS_RAW if file_id == F.CANVAS else F.LIST_RAW
            require(row.get("current") is True and row.get("source") == raw["file"]
                    and row.get("receipt") == raw and row.get("response") == raw
                    and row.get("notice") is None,
                    "Recovery fabricated a canonical row or lost original response bytes")
            require(all(row.get("effective", {}).get(k) == raw["file"][k]
                        for k in ("id", "mode", "filetype", "title", "name", "permalink")),
                    "Sibling publication lost canonical display facts")
        else:
            require(row.get("current") is False
                    and all(row.get(k) is None for k in ("source", "effective", "receipt", "response")),
                    "Unavailable/loading metadata installed a guessed canonical row")
            notice = (OFFLINE if file_id == F.CANVAS else MALFORMED) if phase == "unavailable" else None
            require(row.get("notice") == notice, "Metadata failure lost scoped recovery guidance")


def validate_labels(value, case, stage, viewport, h):
    require = h["require"]
    require(isinstance(value, dict) and value.get("ok") is True
            and value.get("measurement") == "individual-scroll-samples"
            and isinstance(value.get("detailRevision"), str) and value["detailRevision"].isdigit(),
            "Missing measured original recovery occurrences")
    state = phases(case, stage)
    rows = value.get("rows")
    spec = [("workflow-activity-trigger-message", 0, F.LIST, "list", "1"),
            ("workflow-activity-trigger-message", 1, F.LIST, "list", "3"),
            ("workflow-activity-step-message-0", 0, F.CANVAS, "canvas", "1")]
    require(isinstance(rows, list) and len(rows) == 3, "Recovery lost original duplicate occurrences")
    for row, identity in zip(rows, spec):
        container, ordinal, file_id, kind, part = identity
        ready = state.get(file_id) == "ready"
        label = (F.LIST_LABEL if kind == "list" else F.CANVAS_LABEL) if ready else kind.title() + " unavailable"
        anchor = kind == "list" and ready
        require(tuple(row.get(k) for k in ("container", "ordinal", "file", "kind", "part")) == identity
                and row.get("text") == row.get("name") == label
                and row.get("tag") == ("A" if anchor else "SPAN")
                and row.get("href") == (F.LIST_URL if anchor else None)
                and row.get("id") == ("workflow-activity-file-trigger-" + part if anchor else "")
                and row.get("interactive") is anchor
                and all(row.get(k) is True for k in ("ok", "visible", "inline")),
                "Recovery changed original label identity, text or action boundary")
        icon = row.get("icon", {})
        require(all(icon.get(k) is True for k in ("material", "pathMatches", "visible"))
                and icon.get("ariaHidden") == "true" and icon.get("viewBox") == "0 -960 960 960"
                and icon.get("fill") == "currentColor", "Recovery lost official Material icon")
        for rectangle in (row.get("rect"), row.get("nameRect"), icon.get("rect")):
            h["rectangle"](rectangle, viewport)
        a, b = icon["rect"], row["nameRect"]
        require(a["right"] <= b["left"] + 1
                and abs(a["top"] + a["bottom"] - b["top"] - b["bottom"]) <= 6,
                "Recovery icon overlaps its original name")
    retries = value.get("retries")
    expected = (spec if stage == "metadata-failed" else
                spec[:2] if stage == "canvas-recovered" else [])
    require(isinstance(retries, list) and len(retries) == len(expected),
            "Missing, duplicated or active pending Retry control")
    for row, (container, _, file_id, kind, part) in zip(retries, expected):
        require(tuple(row.get(k) for k in ("container", "file", "kind", "part")) == (container, file_id, kind, part)
                and row.get("text") == "Retry file details" and row.get("disabled") is False
                and all(row.get(k) is True for k in ("connected", "visible", "originalOccurrence")),
                "Retry control no longer belongs to its original file occurrence")
        h["rectangle"](row.get("rect"), viewport)
    expected_notice = (OFFLINE if stage == "metadata-failed" else
                       MALFORMED if stage == "canvas-recovered" else
                       None if stage == "recovered" else LOADING)
    notice = value.get("notice")
    if expected_notice is None:
        require(notice is None, "Recovered metadata retained obsolete failure guidance")
    else:
        require(isinstance(notice, dict) and notice.get("id") == "workflow-activity-file-notice"
                and notice.get("text") == expected_notice and notice.get("role") == "status"
                and notice.get("visible") is True, "Missing readable scoped metadata recovery guidance")
        h["rectangle"](notice.get("rect"), viewport)


def validate(fixture, receipt, captures, case, h, *, platform, png=True):
    require = h["require"]
    require(receipt.get("ok") is True, f"Native recovery failed: {receipt.get('error')}")
    require(receipt.get("fixture") == fixture and receipt.get("case") == case
            and all(receipt.get(k) is True for k in
                    ("actualActivityEntry", "aboutConfiguration", "metadataOnly", "noDestination",
                     "draftRetained", "readOnly", "noLiveNetwork"))
            and receipt.get("networkCalls") == 0, "Missing original recovery entry or isolation")
    viewport = h["expected_viewport"](fixture, platform)
    require(receipt.get("viewport") == viewport, "Wrong native recovery viewport")
    rows, paths = receipt.get("stages"), receipt.get("captures")
    require(isinstance(rows, list) and [r.get("stage") for r in rows] == list(STAGES[case])
            and isinstance(paths, dict) and set(paths) == set(STAGES[case]),
            "Wrong recovery stage or screenshot inventory")
    original = None
    prior_files, prior_activity = [], []
    first_detail = "metadata-failed" if case == "file-recovery" else "metadata-held"
    for row in rows:
        stage, model, proof, geometry = (row.get(k) for k in ("stage", "model", "proof", "geometry"))
        require(all(isinstance(v, dict) for v in (model, proof, geometry)), "Missing original recovery observations")
        metadata, activity = model.get("metadata", {}), model.get("ledger", {})
        require(model.get("account") == "synthetic-workflow-activity-file"
                and metadata.get("networkCalls") == activity.get("networkCalls") == metadata.get("writes") == 0
                and metadata.get("pending") is False and metadata.get("busy") is False,
                "Recovery escaped original synthetic account or wrote workflow metadata")
        custody, exercise = proof.get("custody", {}), proof.get("exercise") or {}
        if stage == "about":
            require(custody.get("documents") == [], "Recovery preseeded a workflow document")
        else:
            docs = custody.get("documents")
            require(isinstance(docs, list) and len(docs) == 1, "Lost original workflow document")
            doc = docs[0]
            require(doc.get("title") == metadata.get("title") == h["TITLE"]
                    and doc.get("description") == metadata.get("description") == h["DESCRIPTION"]
                    and doc.get("dirty") is True and metadata.get("dirty") is True and doc.get("pending") is None
                    and all(k in doc for k in ("workflow", "trigger", "preferences", "copyPermissions",
                                              "runPermissions", "restriction")),
                    "Metadata recovery changed original dirty draft or permissions")
            if stage == "draft":
                original = custody
            elif stage == first_detail:
                F.validate_entry(exercise.get("entry"), original, custody, require)
                original = custody
            require(custody == original, "Recovery replaced admitted workflow custody")
        calls = activity.get("requests")
        h["validate_requests"](calls)
        count = (0 if stage in ("about", "draft") else
                 3 if case.endswith("-retirement") and stage in ("recovered", "back") else 2)
        require(len(calls) == count and calls[:len(prior_activity)] == prior_activity
                and activity.get("finished") == count and activity.get("cancelled") == 0
                and activity.get("held") is None, "Metadata recovery replayed the Activity read")
        prior_activity = calls
        files, catalog = proof.get("files"), proof.get("canonical", {})
        file_calls = validate_files(files, case, stage, require)
        require(file_calls[:len(prior_files)] == prior_files, "Metadata request history rewritten")
        prior_files = file_calls
        validate_canonical(catalog, case, stage, require)
        F.validate_destination(proof.get("destination"), False, stage, require)
        if stage not in ("about", "draft", "back"):
            quiet = exercise.get("quiet", {})
            require(quiet.get("observedMs") == 500 and quiet.get("noAutomaticRead") is True
                    and quiet.get("files") == files and quiet.get("canonical") == catalog,
                    "Recovery lacks original quiet-period evidence")
        if stage in ("retry-canvas-held", "retry-list-held"):
            file_id = F.CANVAS if stage == "retry-canvas-held" else F.LIST
            attempt = exercise.get("attempt", {})
            require(attempt.get("id") == file_id and attempt.get("text") == "Retry file details"
                    and attempt.get("part") == "1" and attempt.get("buttonCount") == (1 if file_id == F.CANVAS else 2)
                    and attempt.get("activation") == "original-dom-click"
                    and all(attempt.get(k) is True for k in ("ok", "originalConnected", "duplicateActivation"))
                    and attempt.get("detailRevision") == proof.get("fileLabels", {}).get("detailRevision"),
                    "Retry did not use the original visible native control")
        if case == "file-recovery" and stage in ("canvas-recovered", "recovered"):
            require(exercise.get("release") == files["releases"][-1], "Recovery lost exact released receiver")
        if stage == "late-refused":
            require(exercise.get("release") == files["releases"][0], "Late metadata release lost original receiver")
        if stage == "back":
            require(exercise.get("originalCustody") == original, "Back lacks exact admitted custody comparison")
        require(geometry.get("ok") is True and geometry.get("noOverflow") is True
                and geometry.get("viewport") == viewport, "Recovery geometry is missing or overflowing")
        controls, notices = geometry.get("controls"), geometry.get("notices")
        require(isinstance(controls, list) and controls and isinstance(notices, list), "Missing native recovery controls")
        for control in controls + notices:
            require(control.get("visible") is True, "Recovery feedback/control is clipped")
            h["rectangle"](control.get("rect"), viewport)
        if stage in ("draft", "back"):
            require(geometry.get("fields") == dict(title=h["TITLE"], description=h["DESCRIPTION"],
                                                   titleReadonly=False, descriptionReadonly=False),
                    "Recovery did not preserve actual native editor values")
        if stage in ("about", "draft", "retired", "late-refused", "back"):
            require(proof.get("fileLabels") is None, "File labels claimed outside original detail")
        else:
            validate_labels(proof.get("fileLabels"), case, stage, viewport, h)
        dimensions = geometry.get("pngDimensions")
        require(isinstance(dimensions, list) and len(dimensions) == 2
                and all(type(n) is int and n > 0 for n in dimensions), "Missing original screenshot dimensions")
        expected_path = captures / f"{fixture}-{stage}.png"
        require(Path(paths[stage]) == expected_path, "Foreign recovery screenshot")
        if png:
            h["validate_png"](expected_path, dimensions)
    require(receipt.get("final") == rows[-1]["model"]
            and receipt.get("files") == rows[-1]["proof"]["files"]
            and receipt.get("destination") == rows[-1]["proof"]["destination"],
            "Recovery terminal receipt changed after last capture")
    return receipt
