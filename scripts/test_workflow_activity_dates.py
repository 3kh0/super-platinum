"""Corruption regressions for the executed date-calendar acceptance contract."""
from calendar import monthrange
import copy
from datetime import date, datetime, time, timedelta, timezone
import json
from pathlib import Path
import runpy
import unittest

import workflow_activity_dates as dates

R = runpy.run_path(str(Path(__file__).with_name("check-workflow-activity-native.py")))
DIRECTORY = Path("/private/activity-captures")
BOX = dict(left=10, top=10, right=210, bottom=30, width=200, height=20)


def measured(id="", **extra):
    return dict(id=id, visible=True, hit=True, disabled=False, rect=copy.deepcopy(BOX),
                scrollSample=True, **extra)


def epoch_proof(start, end):
    return dict(rawFrom=start, rawTo=end,
                **{"from": int(datetime.combine(date.fromisoformat(start), time.min, timezone.utc).timestamp()),
                   "to": int(datetime.combine(date.fromisoformat(end), time(23, 59, 59), timezone.utc).timestamp())},
                today="2026-10-20", defaultFrom="2026-07-22", timezone="UTC")


def calendar(start, end, disabled_save=False):
    month = start[:7]
    year, number = map(int, month.split("-"))
    days = [measured(date=f"{month}-{n:02}", tag="BUTTON", text=str(n), tabIndex=-1)
            for n in range(1, monthrange(year, number)[1]+1)]
    save = measured("workflow-activity-calendar-save")
    save["disabled"] = disabled_save
    return dict(month=month, start=start, end=end, epoch="2", rect=copy.deepcopy(BOX),
                days=days, save=save, cancel=measured("workflow-activity-calendar-cancel"),
                focusDate=start)


def sample(scenario="date-range", narrow=False):
    fixture = "workflow-activity" + ("-narrow" if narrow else "")
    viewport = [668, 380] if narrow else [1280, 780]
    pair_by_count = {
        1: ("", ""), 2: ("2026-07-22", "2026-10-20"),
        3: ("2026-01-01", "2026-03-31"), 4: ("2026-06-15", "2026-06-16"),
        5: ("2026-06-15", "2026-06-15"), 6: ("2026-06-15", "2026-06-16"),
        7: ("2026-06-18", "2026-06-19"), 8: ("2026-06-18", "2026-06-19"),
    }
    if scenario.endswith("retirement"):
        pair_by_count[2] = ("2026-06-15", "2026-06-16")
    all_requests = []
    for serial in range(1, max(dates.COUNTS[scenario])+1):
        start, end = pair_by_count[serial]
        filters = {}
        if start:
            epochs = epoch_proof(start, end)
            filters = dict(from_timestamp=epochs["from"], to_timestamp=epochs["to"])
        all_requests.append(dict(serial=serial, method="workflows.activity.list",
                                 args=dict(workflow_id="WF_REVIEW", limit="100", filters=json.dumps(filters),
                                           _x_reason="multi-click-activity-list-fetcher")))
    rows = []
    for stage, count in zip(dates.STAGES[scenario], dates.COUNTS[scenario]):
        hidden = stage in ("about", "account-replaced")
        active = not hidden and stage not in ("draft", "returned")
        held = stage == "held"
        start, end = pair_by_count.get(count, ("", ""))
        filters = dict(revision=max(1, count), from_=start, to=end, status="", user="", limit="100")
        filters["from"] = filters.pop("from_")
        requests = copy.deepcopy(all_requests[:count])
        query = dict(workflow_id="WF_REVIEW", limit=100,
                     filters=json.loads(requests[-1]["args"]["filters"]) if requests else {})
        metadata = dict(route="about" if hidden else "editor", networkCalls=0, writes=0,
                        pending=False, busy=held, dirty=not hidden,
                        title=None if hidden else R["TITLE"], description=None if hidden else R["DESCRIPTION"])
        activity = None if hidden else dict(active=active, filters=filters, query=query,
                                            notice=R["FAILURE"] if stage == "malformed" else None,
                                            page=None if stage == "malformed" else
                                            dict(workflow_events=[dict(workflow_execution_id="E_REVIEW")]))
        model = dict(metadata=metadata, activity=activity,
                     ledger=dict(networkCalls=0, requests=requests, finished=count-int(held),
                                 cancelled=int(stage=="account-replaced"), held=2 if held else None))
        open_ = stage in ("default-open", "range-draft")
        observed = None
        exercise = {}
        if active:
            observed = dict(from_=start, to=end, revision=filters["revision"], open=open_, dirty=stage in ("range-draft", "tab-closed"),
                            pending=False, epoch="2", calendar=None,
                            buttonIdentity=dict(nodeId=1, id="workflow-activity-date-range",
                                                tag="BUTTON", label="Date Range", visit="1", document="2"))
            observed["from"] = observed.pop("from_")
            if stage == "default-open":
                observed["calendar"] = calendar("2026-07-22", "2026-10-20")
                exercise["epochs"] = epoch_proof("2026-07-22", "2026-10-20")
            if stage == "range-draft":
                observed["calendar"] = calendar("2026-06-15", "", True)
            custody = dict(filters=copy.deepcopy(filters), query=copy.deepcopy(query), requests=count,
                           metadata=copy.deepcopy(metadata))
            exercise.update(before=copy.deepcopy(custody), after=copy.deepcopy(custody))
            if start:
                exercise["epochs"] = epoch_proof(start, end)
            if stage == "cancelled":
                exercise["reopened"] = dict(calendar=calendar(start, end, True))
            if stage == "boundary":
                exercise["boundary"] = dict(start=start, allowed=dict(date=end, disabled=False),
                                            blocked=dict(date="2026-04-01", disabled=True))
                exercise["disabledActivation"] = dict(
                    before=dict(start=start, end=""), after=dict(start=start, end=""),
                    observedMs=100, event=dict(ok=True))
            if stage == "reversed":
                exercise["clickOrder"] = [end, start]
            if stage == "keyboard":
                exercise["keyboard"] = dict(beforeDate=start, key="ArrowRight", afterDate=end, selectedDate=end,
                                            activation="focused-button-click-after-synthetic-Enter")
            if stage == "escape-closed":
                exercise["tabPending"] = dict(calendar=dict(start="2026-06-17", end=""))
                exercise["reopened"] = copy.deepcopy(exercise["tabPending"])
        controls = [measured("workflow-configuration")] if hidden else [measured("workflow-management-activity")]
        if active:
            controls = [measured("workflow-activity-" + name) for name in
                        ("back", "filter", "status", "date-range", "limit", "apply")]
        geometry = dict(ok=True, noOverflow=True, viewport=viewport, activity=active,
                        calendarOpen=open_, filterOpen=active,
                        controls=controls, notices=[measured(text=R["FAILURE"])] if stage=="malformed" else [],
                        controlMeasurement="individual-scroll-samples", pngDimensions=viewport,
                        fields=dict(title=R["TITLE"], description=R["DESCRIPTION"], titleReadonly=False, descriptionReadonly=False)
                        if stage in ("draft", "returned") else None)
        rows.append(dict(stage=stage, model=model, geometry=geometry,
                         proof=dict(dateRange=observed, exercise=exercise)))
    receipt = dict(ok=True, fixture=fixture, case=scenario, viewport=viewport, aboutConfiguration=True,
                   actualActivityEntry=True, noLiveNetwork=True, stages=rows,
                   captures={stage: str(DIRECTORY / f"{fixture}-{stage}.png") for stage in dates.STAGES[scenario]},
                   final=copy.deepcopy(rows[-1]["model"]))
    if scenario.endswith("retirement"):
        receipt["replacement"] = dict(foreignDocumentHidden=True, retainedDraftExact=True,
                                      previousEpoch=1, currentEpoch=2,
                                      lateReply=dict(delivered=False, cancelledBeforeReply=True))
    return fixture, receipt, scenario


class CalendarReceiptTests(unittest.TestCase):
    def validate(self, fixture, receipt, scenario):
        return dates.validate(fixture, receipt, DIRECTORY, scenario, R, platform="win32", png=False)

    def corrupt(self, stage, edit, scenario="date-range"):
        fixture, receipt, scenario = sample(scenario)
        edit(next(row for row in receipt["stages"] if row["stage"] == stage))
        with self.assertRaises(RuntimeError):
            self.validate(fixture, receipt, scenario)

    def test_complete_normal_narrow_and_retirement(self):
        for scenario in dates.STAGES:
            for narrow in (False, True):
                self.validate(*sample(scenario, narrow))

    def test_defaults_do_not_commit_on_open(self):
        self.corrupt("default-open", lambda r: r["model"]["activity"]["filters"].update(
            **{"from": "2026-07-22", "to": "2026-10-20"}))

    def test_cancel_cannot_change_original_query(self):
        self.corrupt("cancelled", lambda r: r["proof"]["exercise"]["after"]["query"].update(cursor="other"))

    def test_cancel_reopen_must_disable_save(self):
        self.corrupt("cancelled", lambda r: r["proof"]["exercise"]["reopened"]["calendar"]["save"].update(disabled=False))

    def test_tab_preserves_temporary_pair(self):
        self.corrupt("tab-closed", lambda r: r["proof"]["dateRange"].update(dirty=False))

    def test_escape_rolls_back_temporary_pair(self):
        self.corrupt("escape-closed", lambda r: r["proof"]["dateRange"].update(dirty=True))

    def test_native_button_replacement_cannot_impersonate_same_attributes(self):
        self.corrupt("boundary", lambda r: r["proof"]["dateRange"]["buttonIdentity"].update(nodeId=2))

    def test_incomplete_pair_cannot_save(self):
        self.corrupt("range-draft", lambda r: r["proof"]["dateRange"]["calendar"]["save"].update(disabled=False))

    def test_grid_cannot_omit_in_month_day(self):
        self.corrupt("default-open", lambda r: r["proof"]["dateRange"]["calendar"]["days"].pop())

    def test_calendar_button_cannot_be_obscured(self):
        self.corrupt("default-open", lambda r: r["proof"]["dateRange"]["calendar"]["cancel"].update(hit=False))

    def test_invalid_calendar_date_rejected(self):
        self.corrupt("boundary", lambda r: r["proof"]["exercise"]["boundary"]["blocked"].update(date="2026-02-30"))

    def test_exclusive_boundary_requires_disabled_cell(self):
        self.corrupt("boundary", lambda r: r["proof"]["exercise"]["boundary"]["blocked"].update(disabled=False))

    def test_disabled_activation_cannot_change_selected_pair(self):
        self.corrupt("boundary", lambda r: r["proof"]["exercise"]["disabledActivation"]["after"].update(end="2026-04-01"))

    def test_tab_reopen_must_retain_observed_pair(self):
        self.corrupt("escape-closed", lambda r: r["proof"]["exercise"]["reopened"]["calendar"].update(start="2026-06-18"))

    def test_synthetic_enter_does_not_claim_trusted_activation(self):
        self.corrupt("keyboard", lambda r: r["proof"]["exercise"]["keyboard"].update(activation="trusted-Enter"))

    def test_backward_order_must_be_observed(self):
        self.corrupt("reversed", lambda r: r["proof"]["exercise"]["clickOrder"].reverse())

    def test_keyboard_requires_actual_day_shift(self):
        self.corrupt("keyboard", lambda r: r["proof"]["exercise"]["keyboard"].update(afterDate="2026-06-18"))

    def test_wire_must_match_actual_local_day_edges(self):
        self.corrupt("default-saved", lambda r: r["proof"]["exercise"]["epochs"].update(to=1))

    def test_retry_cannot_issue_duplicate_read(self):
        self.corrupt("retried", lambda r: r["model"]["ledger"]["requests"].append(copy.deepcopy(r["model"]["ledger"]["requests"][-1])))

    def test_metadata_draft_cannot_change(self):
        self.corrupt("same-day", lambda r: r["model"]["metadata"].update(description="lost"))

    def test_retired_callback_cannot_remain_deliverable(self):
        fixture, receipt, scenario = sample("date-range-retirement")
        receipt["replacement"]["lateReply"]["delivered"] = True
        with self.assertRaises(RuntimeError):
            self.validate(fixture, receipt, scenario)


if __name__ == "__main__":
    unittest.main()
