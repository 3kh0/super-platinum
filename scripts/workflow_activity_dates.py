"""Strict offline Activity date-calendar receipts, using actual DOM and request evidence."""
from datetime import date, timedelta
from calendar import monthrange
import json
from pathlib import Path

STAGES = {
    "date-range": (
        "about", "draft", "list", "default-open", "default-saved", "range-draft",
        "cancelled", "boundary", "reversed", "same-day", "keyboard", "tab-closed",
        "escape-closed", "malformed", "retried", "returned",
    ),
    "date-range-retirement": ("about", "draft", "list", "held", "account-replaced"),
}
COUNTS = {
    "date-range": (0, 0, 1, 1, 2, 2, 2, 3, 4, 5, 6, 6, 6, 7, 8, 8),
    "date-range-retirement": (0, 0, 1, 2, 2),
}


def calendar_date(value, require):
    require(isinstance(value, str) and len(value) == 10, "Missing exact calendar date")
    try:
        parsed = date.fromisoformat(value)
    except ValueError:
        require(False, "Invalid calendar date")
    require(parsed.isoformat() == value, "Noncanonical calendar date")
    return parsed


def measured(control, viewport, helpers, *, hit=False):
    require = helpers["require"]
    require(isinstance(control, dict) and control.get("visible") is True,
            "Calendar control was not visibly measured")
    if hit:
        require(control.get("hit") is True, "Calendar control was obscured")
    helpers["rectangle"](control.get("rect"), viewport)


def unchanged(exercise, require):
    before, after = exercise.get("before"), exercise.get("after")
    require(isinstance(before, dict) and isinstance(after, dict), "Missing original transaction snapshots")
    for key in ("filters", "query", "requests", "metadata"):
        require(key in before and key in after and before[key] == after[key],
                f"Uncommitted calendar action changed {key}")


def epochs(exercise, start, end, require):
    value = exercise.get("epochs", {})
    require(value.get("rawFrom") == start and value.get("rawTo") == end,
            "Epoch proof does not describe the committed pair")
    calendar_date(start, require)
    calendar_date(end, require)
    require(type(value.get("from")) is int and type(value.get("to")) is int
            and value["from"] < value["to"], "Missing renderer-local day boundaries")
    require(isinstance(value.get("timezone"), str) and value["timezone"],
            "Missing renderer timezone observation")
    return value


def validate(fixture, receipt, captures, scenario, helpers, *, platform, png=True):
    require = helpers["require"]
    require(receipt.get("ok") is True, f"Native date assertion failed: {receipt.get('error')}")
    require(receipt.get("fixture") == fixture and receipt.get("case") == scenario,
            "Wrong date receipt identity")
    require(all(receipt.get(key) is True for key in
                ("aboutConfiguration", "actualActivityEntry", "noLiveNetwork")),
            "Missing real Activity entry or isolation")
    viewport = helpers["expected_viewport"](fixture, platform)
    require(receipt.get("viewport") == viewport, "Wrong date viewport")
    rows, paths = receipt.get("stages"), receipt.get("captures")
    require(isinstance(rows, list) and [row.get("stage") for row in rows] == list(STAGES[scenario]),
            "Missing or reordered date stages")
    require(isinstance(paths, dict) and set(paths) == set(STAGES[scenario]),
            "Wrong date capture inventory")
    prior_requests = []
    original_button = None
    saved_pairs = {}
    for row, count in zip(rows, COUNTS[scenario]):
        stage = row["stage"]
        model, geometry, proof = row.get("model", {}), row.get("geometry", {}), row.get("proof", {})
        metadata, activity, ledger = model.get("metadata", {}), model.get("activity"), model.get("ledger", {})
        require(metadata.get("networkCalls") == ledger.get("networkCalls") == 0,
                "Date fixture used live network")
        requests = ledger.get("requests")
        helpers["validate_requests"](requests)
        require(len(requests) == count and requests[:len(prior_requests)] == prior_requests,
                "Date action replayed, skipped or rewrote requests")
        prior_requests = requests
        held = stage == "held"
        require(ledger.get("finished") == count - int(held)
                and ledger.get("cancelled") == int(stage == "account-replaced"),
                "Wrong date callback settlement")
        require((type(ledger.get("held")) is int) is held and metadata.get("busy") is held,
                "Wrong date read ownership")
        require(metadata.get("writes") == 0 and metadata.get("pending") is False,
                "Date action wrote workflow metadata")
        hidden = stage in ("about", "account-replaced")
        active = not hidden and stage not in ("draft", "returned")
        if hidden:
            require(metadata.get("route") == "about" and activity is None,
                    "Retired Activity remains visible")
        else:
            require(metadata.get("title") == helpers["TITLE"]
                    and metadata.get("description") == helpers["DESCRIPTION"]
                    and metadata.get("dirty") is True,
                    "Date action lost exact dirty metadata")
            require(isinstance(activity, dict) and activity.get("active") is active,
                    "Wrong date Activity route")
        require(geometry.get("ok") is True and geometry.get("noOverflow") is True
                and geometry.get("viewport") == viewport and geometry.get("activity") is active,
                "Invalid date layout evidence")
        controls, notices = geometry.get("controls"), geometry.get("notices")
        require(isinstance(controls, list) and controls and isinstance(notices, list),
                "Missing date controls or feedback")
        require(geometry.get("controlMeasurement") == "individual-scroll-samples",
                "Date geometry lost scroll-sample scope")
        for control in controls:
            require(control.get("scrollSample") is True, "Missing date control sample")
            measured(control, viewport, helpers)
        for notice in notices:
            measured(notice, viewport, helpers)
        observed = proof.get("dateRange")
        exercise = proof.get("exercise", {})
        if active:
            require(isinstance(observed, dict) and observed.get("pending") is False,
                    "Calendar snapshot missing or native event still pending")
            identity = observed.get("buttonIdentity", {})
            require(identity.get("id") == "workflow-activity-date-range"
                    and identity.get("tag") == "BUTTON" and identity.get("label") == "Date Range"
                    and identity.get("visit") and identity.get("document")
                    and type(identity.get("nodeId")) is int and identity["nodeId"] > 0,
                    "Wrong real Date Range trigger")
            if original_button is None:
                original_button = identity
            require(identity == original_button, "Original calendar owner/button replaced")
            require(type(observed.get("open")) is bool
                    and geometry.get("calendarOpen") is observed["open"]
                    and type(geometry.get("filterOpen")) is bool,
                    "Calendar/Filter visibility was not observed")
            filters = activity.get("filters", {})
            require(observed.get("from") == filters.get("from")
                    and observed.get("to") == filters.get("to")
                    and str(observed.get("revision")) == str(filters.get("revision")),
                    "DOM committed pair differs from original filters")
            calendar = observed.get("calendar")
            ids = {control.get("id") for control in controls}
            if observed["open"]:
                require(geometry["filterOpen"] is True and isinstance(calendar, dict),
                        "Calendar is outside its actual Filter")
                helpers["rectangle"](calendar.get("rect"), viewport)
                require(isinstance(calendar.get("month"), str)
                        and len(calendar["month"]) == 7, "Missing actual calendar month")
                for key in ("save", "cancel"):
                    measured(calendar.get(key), viewport, helpers, hit=True)
                days = calendar.get("days")
                require(isinstance(days, list) and days, "No actual calendar cells")
                month_start = calendar_date(calendar["month"] + "-01", require)
                expected_days = [(month_start + timedelta(days=n)).isoformat()
                                 for n in range(monthrange(month_start.year, month_start.month)[1])]
                require([day.get("date") for day in days] == expected_days,
                        "Calendar grid omitted, duplicated or reordered in-month days")
                for day in days:
                    calendar_date(day.get("date"), require)
                    require(type(day.get("disabled")) is bool, "Unobserved calendar eligibility")
                    measured(day, viewport, helpers, hit=not day["disabled"])
            else:
                require(calendar is None, "Closed calendar leaked mounted draft controls")
                require({"workflow-activity-back", "workflow-activity-filter"} <= ids,
                        "Closed calendar lost navigation")
                if geometry["filterOpen"]:
                    require({"workflow-activity-date-range", "workflow-activity-status",
                             "workflow-activity-limit", "workflow-activity-apply"} <= ids,
                            "Open Filter controls not visibly measured")
            if stage in ("list", "default-open"):
                require(filters.get("from") == filters.get("to") == "",
                        "Display defaults silently committed request dates")
                args = json.loads(requests[0]["args"]["filters"])
                require("from_timestamp" not in args and "to_timestamp" not in args,
                        "Initial request invented date filters")
            if stage == "default-open":
                today = calendar_date(exercise.get("epochs", {}).get("today"), require)
                require(calendar["start"] == (today - timedelta(days=90)).isoformat()
                        and calendar["end"] == today.isoformat()
                        and calendar["save"].get("disabled") is False,
                        "Initial fallback pair or Save enablement changed")
            if stage == "range-draft":
                unchanged(exercise, require)
                require(observed["open"] and calendar["start"] and not calendar["end"]
                        and calendar["save"].get("disabled") is True,
                        "Incomplete new pair must remain temporary")
            if stage in ("cancelled", "tab-closed", "escape-closed"):
                unchanged(exercise, require)
                require(not observed["open"], "Dismissal left calendar open")
                require(observed.get("dirty") is (stage == "tab-closed"),
                        "Tab retention and Cancel/Escape rollback were conflated")
                if stage == "cancelled":
                    reopened = exercise.get("reopened", {}).get("calendar", {})
                    require(reopened.get("save", {}).get("disabled") is True,
                            "Cancel/reopen retained actionable temporary dates")
                if stage == "escape-closed":
                    reopened = exercise.get("reopened", {}).get("calendar", {})
                    retained = exercise.get("tabPending", {}).get("calendar", {})
                    require(retained.get("start") and all(reopened.get(key) == retained.get(key)
                                                        for key in ("start", "end")),
                            "Actual reopen lost the temporary dates retained by Tab")
            if stage in ("default-saved", "boundary", "reversed", "same-day", "keyboard",
                         "malformed", "retried", "held"):
                start, end = observed["from"], observed["to"]
                first, last = calendar_date(start, require), calendar_date(end, require)
                separation = (last - first).days
                require(0 <= separation <= (90 if stage == "default-saved" else 89),
                        "Committed selection exceeds its actual calendar bound")
                converted = epochs(exercise, start, end, require)
                wire = json.loads(requests[-1]["args"]["filters"])
                require(wire.get("from_timestamp") == converted["from"]
                        and wire.get("to_timestamp") == converted["to"],
                        "Actual request lost renderer-local committed day boundaries")
                saved_pairs[stage] = (start, end)
                if stage == "default-saved":
                    require(separation == 90, "Untouched default was shortened")
                if stage == "boundary":
                    boundary = exercise.get("boundary", {})
                    origin = calendar_date(boundary.get("start"), require)
                    allowed, blocked = boundary.get("allowed", {}), boundary.get("blocked", {})
                    require((calendar_date(allowed.get("date"), require) - origin).days == 89
                            and (calendar_date(blocked.get("date"), require) - origin).days == 90
                            and allowed.get("disabled") is False and blocked.get("disabled") is True
                            and separation == 89, "Exclusive new-selection boundary not exercised")
                    activation = exercise.get("disabledActivation", {})
                    require(isinstance(activation.get("before"), dict)
                            and activation["before"] == activation.get("after")
                            and activation.get("observedMs") == 100
                            and activation.get("event", {}).get("ok") is True,
                            "Disabled-day activation changed or lacked observed selection custody")
                if stage == "reversed":
                    order = exercise.get("clickOrder")
                    require(isinstance(order, list) and order == [end, start] and start < end,
                            "Backward second click was not sorted")
                if stage == "same-day":
                    require(start == end, "Same-day selection did not include whole local day")
                if stage == "keyboard":
                    keyboard = exercise.get("keyboard", {})
                    shifts = {"ArrowLeft": -1, "ArrowRight": 1, "ArrowUp": -7, "ArrowDown": 7}
                    require(keyboard.get("key") in shifts, "Actual calendar arrow was not observed")
                    require((calendar_date(keyboard.get("afterDate"), require)
                             - calendar_date(keyboard.get("beforeDate"), require)).days
                            == shifts[keyboard["key"]]
                            and keyboard.get("selectedDate") == keyboard["afterDate"],
                            "Keyboard focus/selection did not follow actual day")
                    require(keyboard.get("activation") == "focused-button-click-after-synthetic-Enter",
                            "Synthetic Enter must distinguish explicit focused-button activation")
                if stage == "retried":
                    require(saved_pairs["malformed"] == (start, end),
                            "Retry changed committed dates")
            if stage == "malformed":
                require(activity.get("notice") == helpers["FAILURE"]
                        and helpers["FAILURE"] in [n.get("text") for n in notices],
                        "Malformed response lost visible failure")
            if stage == "retried":
                require(activity.get("notice") is None
                        and activity.get("page", {}).get("workflow_events"),
                        "Explicit Retry did not recover")
        else:
            require(observed is None, "Calendar witness survived outside Activity")
        if stage in ("draft", "returned"):
            require(geometry.get("fields") == dict(title=helpers["TITLE"], description=helpers["DESCRIPTION"],
                                                   titleReadonly=False, descriptionReadonly=False),
                    "Returning from calendar lost original native metadata")
        dimensions = geometry.get("pngDimensions")
        require(isinstance(dimensions, list) and len(dimensions) == 2
                and all(type(n) is int and n > 0 for n in dimensions), "Missing date PNG dimensions")
        expected_path = captures / f"{fixture}-{stage}.png"
        require(Path(paths[stage]) == expected_path, "Foreign date screenshot path")
        if png:
            helpers["validate_png"](expected_path, dimensions)
    require(receipt.get("final") == rows[-1]["model"], "Date final state differs from capture")
    if scenario == "date-range-retirement":
        replacement = receipt.get("replacement", {})
        require(replacement.get("foreignDocumentHidden") is True
                and replacement.get("retainedDraftExact") is True
                and replacement.get("currentEpoch", 0) > replacement.get("previousEpoch", 0)
                and replacement.get("lateReply") == dict(delivered=False, cancelledBeforeReply=True),
                "Actual held date read retirement not established")
    return receipt
