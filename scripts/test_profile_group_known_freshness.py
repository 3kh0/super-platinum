"""Pure known-group oracle regressions. No processes, windows, or network."""
import copy
import json
from pathlib import Path
import runpy
import unittest

import profile_group_known_freshness as oracle

RUNNER = runpy.run_path(str(Path(__file__).with_name("check-profile-group-recovery-native.py")))
CAPTURES = Path("tmp/pure-known-group-tests").resolve()
VIEWPORT = [1280, 780]
ACCOUNT = "00000000-0000-4000-8000-000000000081"
FOREIGN = "00000000-0000-4000-8000-000000000082"
GROUPS = ["SMISSING01", "SMISSING02"]
TEAM, USER = "TINIT0001", "UINIT0001"


def rich(edited=False):
    return [{"type": "rich_text", "elements": [{
        "type": "rich_text_section", "elements": [
            {"type": "text", "text": ("Native edited \u4e2d\U0001f98a & description with "
                                     if edited else "Keep my unsaved description with ")},
            {"type": "usergroup", "usergroup_id": GROUPS[0], "fixture_occurrence": 1,
             "tab_id": "opaque-tab", "opaque": {"keep": True}, "style": {"bold": True}},
            {"type": "usergroup", "usergroup_id": GROUPS[0], "fixture_occurrence": 2,
             "tab_id": "opaque-tab", "opaque": {"keep": True}, "style": {"italic": True}},
            {"type": "usergroup", "usergroup_id": GROUPS[1], "fixture_occurrence": 3,
             "tab_id": "opaque-tab", "opaque": {"keep": True}},
        ]}]}]


def names(renamed=False):
    return ["@Renamed crew" if renamed else "@@ Cafe\u0301 crew  "] * 2 + ["@Review crew"]


def raw(group, renamed=False, poison=False):
    return dict(id=group, team_id=TEAM,
                handle=("Equal version must not replace" if poison else "Renamed crew")
                if renamed and group == GROUPS[0] else
                "@@ Cafe\u0301 crew  " if group == GROUPS[0] else "Review crew",
                name="Fixture crew", date_delete=0, is_visible=True,
                fixture_metadata={"keep": True},
                date_update=102 if renamed and group == GROUPS[0] else 101)


def source(renamed=False):
    return dict(current=True, membership=None, rows=[
        dict(source=raw(g, renamed), effective=dict(
            raw(g, renamed), user_count=0, _name_lc="fixture crew",
            is_usergroup=True, isNonExistent=False, users=[])) for g in GROUPS])


def groups(renamed=False):
    return {g: dict(source=raw(g, renamed), label=names(renamed)[0 if i == 0 else 2],
                    native=dict(id=g, label_source=raw(g, renamed),
                                users=[], members_loaded=False))
            for i, g in enumerate(GROUPS)}


def seed():
    user = dict(id=USER, name="fixture-self", team_id=TEAM, profile={})
    team = dict(id=TEAM, name="Known group freshness")
    return dict(boot=dict(ok=True, team=team, self=user, users=[user], prefs={},
                         channels=[dict(id="CINIT0001", name="warm-harbor",
                                        is_channel=True, is_member=True)]),
                init=dict(ok=True, self=user, team=team, workspaces=[dict(id=TEAM)],
                          onboarding=dict(user_data=dict(onboarding_complete=True)),
                          user_date_created=123),
                rows=[raw(g) for g in GROUPS], origin="Fetch", nativeMembers="Preserve",
                actualSourceSink=True)


def rect():
    return dict(left=10, top=10, right=110, bottom=30, width=100, height=20)


def tokens(renamed=False):
    return [dict(value=copy.deepcopy(t), label=label) for t, label in
            zip(rich()[0]["elements"][0]["elements"][1:], names(renamed))]


def custody(edited=False, renamed=False, remembered=False):
    value = dict(ok=True, blocks=rich(edited), tokens=tokens(renamed), start=2, end=8)
    if not remembered:
        value.update(sameNode=True, sameSelection=True, noToast=True)
    return value


def reset(ordinal):
    return dict(team=TEAM, actualAcceptedConnection=True, oldGeneration=ordinal + 10,
                generation=ordinal + 11, before=dict(slowConnectionCount=ordinal, major=1),
                after=dict(slowConnectionCount=ordinal + 1, major=1),
                duplicate=dict(slowConnectionCount=ordinal + 1, major=1))


def quiet():
    return dict(observedMs=500, noAdditionalRead=True, sourceRetained=True)


def ledger(case, batches, held=False):
    rows = []
    for batch in range(batches):
        kinds = ["discovery", "directory"]
        if case.endswith("errors") and not (held and batch == batches - 1):
            kinds.append("direct")
        for kind in kinds:
            pending = held and batch == batches - 1 and kind == "directory"
            if kind == "discovery":
                method = "api.getFlannelHttpUrl"
                body = dict(include_external_workspaces="true",
                            _x_reason="client-needs-valid-flannel-url")
                response = dict(raw=dict(ok=True,
                                         url="https://edgeapi.slack.com/cache/TINIT0001",
                                         ttl_seconds=300))
            elif kind == "directory":
                method, body = "usergroups/info", dict(ids=GROUPS[:])
                response = (dict(error="offline") if case.endswith("errors") and batch == 0
                            else dict(raw=dict(ok=True, results="malformed"))
                            if case.endswith("errors") else
                            dict(raw=dict(ok=True, results=[raw(g, True, batch == 1)
                                                           for g in GROUPS])))
            else:
                method = "subteams.info"
                body = dict(usergroups="SMISSING01,SMISSING02", include_users="false",
                            include_count="true", include_disabled="true",
                            _x_reason="fallback:fetchRawUserGroupsById")
                response = (dict(error="offline") if batch == 0 else
                            dict(raw=dict(ok=True, subteams="malformed")))
            row = dict(serial=len(rows) + 1, kind=kind, method=method, body=body,
                       account=ACCOUNT, team=TEAM, epoch=0, released=not pending)
            if not pending:
                row.update(response=response,
                           outcome="offline" if "error" in response else "response",
                           receiverRetired=False, delivered=True)
            rows.append(row)
    return rows


# Independent stage facts: batch count, held, editor, source renamed, editor
# renamed, edited native draft, saved form, replaced account.
FACTS = {
    "known-freshness": [
        ("initial", 0, 0, 0, 0, 0, 0, 0, 0), ("fresh-open", 0, 0, 1, 0, 0, 0, 0, 0),
        ("mounted-held", 1, 1, 1, 0, 0, 0, 0, 0), ("edited-held", 1, 1, 1, 0, 0, 1, 0, 0),
        ("refreshed-mounted", 1, 0, 1, 1, 0, 1, 0, 0), ("saved", 1, 0, 0, 1, 1, 1, 1, 0),
        ("reopened", 1, 0, 1, 1, 1, 1, 1, 0), ("equal-held", 2, 1, 1, 1, 1, 1, 1, 0),
        ("equal-mounted", 2, 0, 1, 1, 1, 1, 1, 0)],
    "known-freshness-errors": [
        ("initial", 0, 0, 0, 0, 0, 0, 0, 0), ("fresh-open", 0, 0, 1, 0, 0, 0, 0, 0),
        ("mounted-held", 1, 1, 1, 0, 0, 0, 0, 0), ("offline-mounted", 1, 0, 1, 0, 0, 0, 0, 0),
        ("fresh-reopened", 1, 0, 1, 0, 0, 0, 0, 0), ("reset-held", 2, 1, 1, 0, 0, 0, 0, 0),
        ("malformed-mounted", 2, 0, 1, 0, 0, 0, 0, 0), ("final-reopened", 2, 0, 1, 0, 0, 0, 0, 0)],
    "known-freshness-retirement": [
        ("initial", 0, 0, 0, 0, 0, 0, 0, 0), ("fresh-open", 0, 0, 1, 0, 0, 0, 0, 0),
        ("mounted-held", 1, 1, 1, 0, 0, 0, 0, 0), ("cancelled", 1, 1, 0, 0, 0, 0, 0, 0),
        ("late-settled", 1, 0, 0, 1, 0, 0, 0, 0), ("reopened", 1, 0, 1, 1, 1, 0, 0, 0),
        ("account-held", 2, 1, 1, 1, 1, 0, 0, 0), ("account-replaced", 2, 1, 0, 0, 0, 0, 0, 1),
        ("late-refused", 2, 0, 0, 0, 0, 0, 0, 1)],
}


def receipt(case="known-freshness"):
    result = dict(case=case, fixture="profile-fields", requests=ledger(case, 2),
                  captures={}, stages=[])
    for fact in FACTS[case]:
        stage, count, held, editor, renamed, editor_renamed, edited, saved, replaced = fact
        held, editor, renamed, editor_renamed, edited, saved, replaced = map(
            bool, (held, editor, renamed, editor_renamed, edited, saved, replaced))
        requests = ledger(case, count, held)
        form = dict(enabled=not replaced, blocks=rich(saved), dirty=not saved,
                    account=ACCOUNT, epoch=0, busy=False, uncertain=False,
                    choiceValue="Morning", countryValue="US",
                    managedValue="Preserved organization value", reads=int(saved),
                    attempts=[[dict(element_id="long", long_text=dict(
                        blocks=json.dumps(rich(True))))]] if saved else [], visit=1, serial=2)
        scheduler = dict(scope=dict(account=ACCOUNT, team=TEAM, user=USER, epoch=0),
                         started=count, finished=count - int(held), worker=held, pending=[],
                         inFlight=GROUPS[:] if held else None,
                         freshIds=[] if held else GROUPS[:],
                         observations=dict(slowConnectionCount=count + 1, major=1),
                         lastSettlement=None if count - int(held) == 0 else
                         dict(ids=GROUPS[:], attempted=GROUPS[:], timedOut=False,
                              adopted=not case.endswith("errors"),
                              errors=2 if case.endswith("errors") else 0))
        known = dict(seed=seed(), networkCalls=0, resets=[reset(i) for i in range(1, count + 1)],
                     source=None if replaced else source(renamed),
                     scheduler=None if replaced else scheduler,
                     opening=None if replaced else dict(
                         visit=1, serial=2, key=3, mounted=editor, reading=False, lease=False))
        model = dict(account=FOREIGN if replaced else ACCOUNT, epoch=int(replaced),
                     requests=requests, pending=requests[-1:] if held else [],
                     driver=dict(started=0, finished=0, active=0), profile=form,
                     knownFreshness=known, groups={g: dict(native=None, source=None, label=None)
                                                   for g in GROUPS} if replaced else groups(renamed))
        controls = [] if replaced else ["Back", "Save section"] + (
            ["Use description", "Cancel description edit"] if editor else ["Edit description"])
        captured = dict(snapshot=dict(blocks=rich(edited), composing=False, revision=2,
                                      generation="original-native-mount"),
                        tokens=tokens(editor_renamed)) if editor else None
        geometry = dict(ok=True, noOverflow=True, viewport=VIEWPORT, dialog=not replaced,
                        dialogRect=rect(), controls=[
                            dict(text=c, visible=True, rect=rect(), disabled=c == "Save section")
                            for c in controls], notices=[
                            dict(text="Profile fields saved.", visible=True, rect=rect())
                        ] if stage == "saved" else [],
                        tokens=[dict(text=label, editable=False, visible=True, rect=rect(),
                                     fragments=[rect()], fontWeight="700" if i == 0 else "400",
                                     fontStyle="italic" if i == 1 else "normal")
                                for i, label in enumerate(names(editor_renamed))] if editor else [],
                        feedbackRequired=False, feedbackVisible=False,
                        richEditable=True if editor else None, captured=captured,
                        globalToast=None, pngDimensions=[1280, 780])
        proof = dict(quiet=quiet())
        if editor:
            proof["custody"] = custody(edited, editor_renamed)
        if stage == "initial":
            proof = dict(originalDraft=True, actualSourceSink=True)
        if stage == "fresh-open":
            proof["noRead"] = True
        held_value = dict(immediateAdmission=True, duplicateIdsCoalesced=True, duplicateOpen=True,
                          reset=reset(count), request=requests[-1]["serial"] if requests else 0,
                          opening=copy.deepcopy(known["opening"]), quiet=quiet())
        if stage in ("mounted-held", "account-held"):
            proof.update(held_value)
        if stage in ("equal-held", "reset-held"):
            proof["held"] = held_value
        if stage in ("equal-held", "reset-held", "offline-mounted"):
            proof["remembered"] = custody(edited, editor_renamed, True)
        if stage in ("offline-mounted", "malformed-mounted"):
            for key, request in zip(("directoryRelease", "fallbackRelease"), requests[-2:]):
                proof[key] = dict(serial=request["serial"], receiverRetired=False, delivered=True)
        if stage == "malformed-mounted":
            proof.update(directoryRaw=dict(ok=True, results="malformed"),
                         directRaw=dict(ok=True, subteams="malformed"))
        if stage == "edited-held":
            before, after = copy.deepcopy(captured), copy.deepcopy(captured)
            before["snapshot"].update(blocks=rich(), revision=1)
            proof.update(nativeEdit=dict(ok=True, before=before, after=after),
                         remembered=custody(True, False, True))
        if stage in ("refreshed-mounted", "equal-mounted", "offline-mounted",
                     "malformed-mounted", "late-settled"):
            proof["settled"] = copy.deepcopy(scheduler)
        if stage in ("refreshed-mounted", "late-settled", "equal-mounted", "late-refused"):
            proof.update(response=dict(ok=True, results=[
                raw(g, True, stage in ("equal-mounted", "late-refused")) for g in GROUPS]),
                release=dict(serial=requests[-1]["serial"],
                             receiverRetired=False, delivered=True))
        if stage == "refreshed-mounted":
            proof.update(mountedOldLabels=True, source=copy.deepcopy(known["source"]))
        if stage in ("equal-mounted", "offline-mounted", "malformed-mounted"):
            proof.update(sourceBefore=copy.deepcopy(known["source"]),
                         sourceAfter=copy.deepcopy(known["source"]), equalVersionSkipped=True)
        if stage == "saved":
            proof = dict(actualProfileSaveReadback=True, rawDuplicatesPreserved=True)
        if stage in ("reopened", "fresh-reopened", "final-reopened"):
            proof.update(noRedundantRead=True, newLabels=True)
        if stage == "cancelled":
            proof.update(accountOwnedReadRetained=True, before=copy.deepcopy(model),
                         after=copy.deepcopy(model))
        if stage == "late-settled":
            proof["cancelledConsumerNotRemounted"] = True
        if stage == "account-replaced":
            proof.update(actualInstallLoaded=True, before=dict(account=ACCOUNT),
                         after=copy.deepcopy(model), originalWorkerAlive=True)
        if stage == "late-refused":
            proof.update(successorUnchanged=True, before=copy.deepcopy(model),
                         after=copy.deepcopy(model), originalWorkerFinished=True)
        result["stages"].append(dict(stage=stage, model=model, geometry=geometry, proof=proof))
        result["captures"][stage] = str(CAPTURES / f'profile-fields-{stage}.png')
    return result


def validate(value):
    return oracle.validate_known(value, CAPTURES, VIEWPORT, RUNNER["require"],
                                 RUNNER["rectangle"], RUNNER["validate_png"], False)


def row(value, stage):
    return next(r for r in value["stages"] if r["stage"] == stage)


class KnownFreshnessOracleTests(unittest.TestCase):
    def reject(self, stage, mutate, case="known-freshness"):
        value = receipt(case)
        validate(value)
        mutate(row(value, stage))
        with self.assertRaises(RuntimeError):
            validate(value)

    def test_all_three_complete_scenarios_admit(self):
        for case in FACTS:
            with self.subTest(case=case):
                validate(receipt(case))

    def test_fixtures_keep_combining_marks_and_astral_text(self):
        self.assertIn("\u4e2d\U0001f98a", rich(True)[0]["elements"][0]["elements"][0]["text"])
        self.assertEqual(names()[0], "@@ Cafe\u0301 crew  ")
        self.assertNotEqual(names()[0], "@@ Caf\u00e9 crew  ")

    def test_raw_unicode_normalization_is_not_accepted(self):
        self.reject("fresh-open", lambda r: r["model"]["groups"][GROUPS[0]]["source"].update(
            handle="@@ Caf\u00e9 crew  "))

    def test_edited_text_loss_is_rejected(self):
        self.reject("edited-held", lambda r: r["geometry"]["captured"]["snapshot"]["blocks"]
                    [0]["elements"][0]["elements"][0].update(text="Native edited ?? & description with "))

    def test_duplicate_occurrence_loss_is_rejected(self):
        self.reject("refreshed-mounted", lambda r: r["geometry"]["captured"]["snapshot"]["blocks"]
                    [0]["elements"][0]["elements"].pop(2))

    def test_opaque_token_change_is_rejected(self):
        self.reject("fresh-open", lambda r: r["geometry"]["captured"]["tokens"]
                    [1]["value"]["opaque"].update(keep=False))

    def test_inline_style_swap_is_rejected(self):
        self.reject("fresh-open", lambda r: r["geometry"]["tokens"][1].update(
            fontWeight="700", fontStyle="normal"))

    def test_mounted_label_cannot_follow_background_rename(self):
        self.reject("refreshed-mounted", lambda r: r["geometry"]["tokens"][0].update(
            text="@Renamed crew"))

    def test_new_opening_must_receive_new_label(self):
        self.reject("reopened", lambda r: r["geometry"]["tokens"][0].update(text=names()[0]))

    def test_original_native_node_and_selection_are_required(self):
        for key in ("sameNode", "sameSelection", "noToast"):
            with self.subTest(key=key):
                self.reject("refreshed-mounted", lambda r: r["proof"]["custody"].update({key: False}))

    def test_saved_wire_must_retain_exact_opaque_duplicates(self):
        def mutate(r):
            wire = rich(True)
            wire[0]["elements"][0]["elements"][1].pop("tab_id")
            r["model"]["profile"]["attempts"][0][0]["long_text"]["blocks"] = json.dumps(wire)
        self.reject("saved", mutate)

    def test_held_request_cannot_claim_delivery(self):
        self.reject("mounted-held", lambda r: r["model"]["requests"][-1].update(released=True))

    def test_known_path_cannot_use_missing_opening(self):
        self.reject("mounted-held", lambda r: r["model"]["driver"].update(started=1, active=1))

    def test_reset_requires_actual_single_generation_transition(self):
        for field, value in (("actualAcceptedConnection", False), ("generation", 99)):
            with self.subTest(field=field):
                self.reject("mounted-held", lambda r: r["model"]["knownFreshness"]["resets"][0].update(
                    {field: value}))

    def test_duplicate_reset_cannot_advance_slow_count(self):
        self.reject("mounted-held", lambda r: r["model"]["knownFreshness"]["resets"][0]
                    ["duplicate"].update(slowConnectionCount=3))

    def test_scheduler_cannot_stamp_held_attempt_fresh(self):
        self.reject("mounted-held", lambda r: r["model"]["knownFreshness"]["scheduler"].update(
            freshIds=GROUPS[:]))

    def test_scheduler_source_scope_is_not_interchangeable(self):
        self.reject("fresh-open", lambda r: r["model"]["knownFreshness"]["scheduler"]["scope"].update(
            account=FOREIGN))

    def test_actual_source_lineage_is_required(self):
        self.reject("initial", lambda r: r["model"]["knownFreshness"]["seed"].update(
            actualSourceSink=False))

    def test_typed_row_cannot_substitute_for_raw_source(self):
        self.reject("fresh-open", lambda r: r["model"]["groups"][GROUPS[0]]["native"].update(
            label_source=None))

    def test_membership_must_not_be_replaced_by_metadata_refresh(self):
        self.reject("refreshed-mounted", lambda r: r["model"]["groups"][GROUPS[0]]["native"].update(
            users=["UUNEXPECTED"], members_loaded=True))

    def test_equal_version_poison_cannot_publish(self):
        self.reject("equal-mounted", lambda r: r["model"]["knownFreshness"]["source"]["rows"][0]
                    ["source"].update(handle="Equal version must not replace"))

    def test_equal_response_and_skip_proof_are_not_optional(self):
        self.reject("equal-mounted", lambda r: r["proof"].pop("response"))
        self.reject("equal-mounted", lambda r: r["proof"].update(equalVersionSkipped=False))

    def test_failed_fetch_must_keep_original_source_and_current_editor(self):
        for stage in ("offline-mounted", "malformed-mounted"):
            with self.subTest(stage=stage):
                self.reject(stage, lambda r: r["proof"]["sourceAfter"].update(current=False),
                            "known-freshness-errors")

    def test_error_cannot_be_reported_as_unattempted_or_pending(self):
        self.reject("offline-mounted", lambda r: r["model"]["knownFreshness"]["scheduler"].update(
            finished=0, worker=True), "known-freshness-errors")

    def test_default_ids_wire_cannot_be_changed_to_updated_ids(self):
        self.reject("mounted-held", lambda r: r["model"]["requests"][-1].update(
            body=dict(updated_ids={GROUPS[0]: 101})))

    def test_actual_quiet_window_is_required(self):
        self.reject("fresh-reopened", lambda r: r["proof"]["quiet"].update(observedMs=0),
                    "known-freshness-errors")

    def test_cancel_does_not_cancel_account_owned_read(self):
        self.reject("cancelled", lambda r: r["proof"].update(accountOwnedReadRetained=False),
                    "known-freshness-retirement")

    def test_late_cancelled_result_cannot_remount_editor(self):
        self.reject("late-settled", lambda r: r["model"]["knownFreshness"]["opening"].update(
            mounted=True), "known-freshness-retirement")

    def test_account_replacement_cannot_keep_old_source(self):
        self.reject("account-replaced", lambda r: r["model"]["knownFreshness"].update(
            source=source(True)), "known-freshness-retirement")

    def test_late_original_response_requires_actual_delivery_not_cancellation(self):
        self.reject("late-refused", lambda r: r["proof"]["release"].update(
            delivered=False, receiverRetired=True), "known-freshness-retirement")

    def test_account_replacement_must_observe_original_worker_still_alive(self):
        self.reject("account-replaced", lambda r: r["proof"].update(
            originalWorkerAlive=False), "known-freshness-retirement")
        self.reject("account-replaced", lambda r: r["proof"].pop(
            "originalWorkerAlive"), "known-freshness-retirement")

    def test_successor_comparison_requires_original_worker_finished(self):
        self.reject("late-refused", lambda r: r["proof"].update(
            originalWorkerFinished=False), "known-freshness-retirement")
        self.reject("late-refused", lambda r: r["proof"].pop(
            "originalWorkerFinished"), "known-freshness-retirement")

    def test_late_ledger_cannot_claim_receiver_retirement(self):
        self.reject("late-refused", lambda r: r["model"]["requests"][-1].update(
            delivered=False, receiverRetired=True), "known-freshness-retirement")

    def test_successor_comparison_must_retain_original_account(self):
        self.reject("late-refused", lambda r: r["proof"]["before"].update(account=ACCOUNT),
                    "known-freshness-retirement")

    def test_hidden_token_and_out_of_view_controls_are_rejected(self):
        self.reject("fresh-open", lambda r: r["geometry"]["tokens"][0].update(visible=False))
        self.reject("fresh-open", lambda r: r["geometry"]["controls"][0]["rect"].update(
            top=900, bottom=920))

    def test_native_generation_and_revision_must_prove_real_edit(self):
        self.reject("edited-held", lambda r: r["proof"]["nativeEdit"]["after"]["snapshot"].update(
            generation="replacement"))
        self.reject("edited-held", lambda r: r["proof"]["nativeEdit"]["after"]["snapshot"].update(
            revision=1))


if __name__ == "__main__":
    unittest.main()
