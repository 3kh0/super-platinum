"""Strict known-group freshness acceptance; no process or network operations."""
import json
from pathlib import Path

CASES = {
    "known-freshness": (
        "initial", "fresh-open", "mounted-held", "edited-held", "refreshed-mounted",
        "saved", "reopened", "equal-held", "equal-mounted"),
    "known-freshness-errors": (
        "initial", "fresh-open", "mounted-held", "offline-mounted",
        "fresh-reopened", "reset-held", "malformed-mounted", "final-reopened"),
    "known-freshness-retirement": (
        "initial", "fresh-open", "mounted-held", "cancelled", "late-settled",
        "reopened", "account-held", "account-replaced", "late-refused"),
}
ACCOUNT = "00000000-0000-4000-8000-000000000081"
FOREIGN = "00000000-0000-4000-8000-000000000082"
TEAM, SELF = "TINIT0001", "UINIT0001"
GROUPS = ("SMISSING01", "SMISSING02")
LABEL = "@@ Cafe\u0301 crew  "
OTHER_LABEL = "@Review crew"
RENAMED = "@Renamed crew"
DRAFT = "Keep my unsaved description with "
EDITED = "Native edited \u4e2d\U0001f98a & description with "


def blocks(edited=False):
    elements = [dict(type="text", text=EDITED if edited else DRAFT)]
    for i, group in enumerate((GROUPS[0], GROUPS[0], GROUPS[1]), 1):
        token = dict(type="usergroup", usergroup_id=group, fixture_occurrence=i,
                     tab_id="opaque-tab", opaque={"keep": True})
        if i < 3:
            token["style"] = {"bold" if i == 1 else "italic": True}
        elements.append(token)
    return [dict(type="rich_text", elements=[
        dict(type="rich_text_section", elements=elements)])]


def labels(renamed=False):
    return [RENAMED if renamed else LABEL] * 2 + [OTHER_LABEL]


def raw(group, renamed=False, poison=False):
    value = dict(id=group, team_id=TEAM,
                 handle=LABEL if group == GROUPS[0] else "Review crew",
                 name="Fixture crew", date_delete=0, is_visible=True,
                 fixture_metadata={"keep": True}, date_update=101)
    if group == GROUPS[0] and renamed:
        value.update(date_update=102,
                     handle="Equal version must not replace" if poison else "Renamed crew")
    return value


def response(renamed=False, poison=False):
    return dict(ok=True, results=[raw(group, renamed, poison) for group in GROUPS])


def effective(group, renamed=False):
    return dict(raw(group, renamed), user_count=0, _name_lc="fixture crew",
                is_usergroup=True, isNonExistent=False, users=[])


def seed():
    user = dict(id=SELF, name="fixture-self", team_id=TEAM, profile={})
    team = dict(id=TEAM, name="Known group freshness")
    return dict(
        boot=dict(ok=True, team=team, self=user, users=[user], prefs={},
                  channels=[dict(id="CINIT0001", name="warm-harbor",
                                 is_channel=True, is_member=True)]),
        init=dict(ok=True, self=user, team=team, workspaces=[dict(id=TEAM)],
                  onboarding=dict(user_data=dict(onboarding_complete=True)),
                  user_date_created=123),
        rows=[raw(group) for group in GROUPS], origin="Fetch",
        nativeMembers="Preserve", actualSourceSink=True)


def source(value, groups, renamed, require):
    require(isinstance(value, dict) and value.get("current") is True
            and "membership" in value and value["membership"] is None,
            "Missing actual current canonical group source")
    require(value.get("rows") == [
        dict(source=raw(group, renamed), effective=effective(group, renamed))
        for group in GROUPS], "Canonical group source/effective rows changed")
    require(isinstance(groups, dict) and set(groups) == set(GROUPS),
            "Wrong known group inventory")
    for group in GROUPS:
        item = groups[group]
        native = item.get("native", {})
        require(item.get("source") == raw(group, renamed)
                and native.get("label_source") == raw(group, renamed),
                "Native group lost original raw source")
        require(native.get("id") == group and native.get("users") == []
                and native.get("members_loaded") is False,
                "Group metadata changed native membership")
        expected_label = (RENAMED if renamed else LABEL) if group == GROUPS[0] else OTHER_LABEL
        require(item.get("label") == expected_label, "Wrong canonical known group label")


def layout(value, viewport, *, editor, renamed, edited, replaced, saved,
           can_save, require, rectangle):
    require(isinstance(value, dict) and value.get("ok") is True
            and value.get("noOverflow") is True and value.get("viewport") == viewport,
            "Missing measured known-group layout")
    require(value.get("dialog") is (not replaced), "Wrong known-group dialog owner")
    if not replaced:
        rectangle(value.get("dialogRect"), viewport)
    controls, notices, tokens = [value.get(k) for k in ("controls", "notices", "tokens")]
    require(all(isinstance(v, list) for v in (controls, notices, tokens)),
            "Missing known-group DOM inventory")
    for item in controls + notices + tokens:
        require(item.get("visible") is True, "Known-group control or token hidden")
        rectangle(item.get("rect"), viewport)
    wanted = [] if replaced else ["Back", "Save section"] + (
        ["Use description", "Cancel description edit"] if editor else ["Edit description"])
    require(sorted(c.get("text") for c in controls) == sorted(wanted),
            "Known-group editor replaced with missing-reference recovery")
    require(all(c.get("disabled") is (c["text"] == "Save section" and not can_save)
                for c in controls), "Wrong known-group control availability")
    require(value.get("feedbackRequired") is False and value.get("feedbackVisible") is False,
            "Healthy known editor acquired missing-reference feedback")
    require(value.get("richEditable") is (True if editor else None),
            "Known editor was withheld or remounted after retirement")
    if editor:
        wanted_labels = labels(renamed)
        require([t.get("text") for t in tokens] == wanted_labels
                and all(t.get("editable") is False for t in tokens),
                "Mounted group labels or duplicate occurrence order changed")
        require([(t.get("fontWeight"), t.get("fontStyle")) for t in tokens] == [
            ("700", "normal"), ("400", "italic"), ("400", "normal")],
            "Known-group token styles changed")
        for token in tokens:
            require(isinstance(token.get("fragments"), list) and token["fragments"],
                    "Missing known-group inline fragments")
            for fragment in token["fragments"]:
                rectangle(fragment, viewport)
        captured = value.get("captured", {})
        snapshot = captured.get("snapshot", {})
        require(snapshot.get("blocks") == blocks(edited)
                and snapshot.get("composing") is False
                and type(snapshot.get("revision")) is int
                and isinstance(snapshot.get("generation"), str) and snapshot["generation"],
                "Original native rich draft changed")
        require(captured.get("tokens") == [
            dict(value=token, label=label)
            for token, label in zip(blocks()[0]["elements"][0]["elements"][1:], wanted_labels)],
            "Original opaque group token witnesses changed")
    else:
        require(tokens == [] and value.get("captured") is None,
                "Retired editor still has native token capture")
    if saved:
        require(any(n.get("text") == "Profile fields saved." for n in notices),
                "Missing actual Profile Save/readback notice")
    require("globalToast" in value, "Missing independent toast observation")
    toast = value["globalToast"]
    if toast is not None:
        require(isinstance(toast, dict) and type(toast.get("visible")) is bool
                and type(toast.get("positioned")) is bool, "Invalid known-group toast")
        if toast["visible"]:
            require(toast["positioned"] and toast.get("visibility") == "visible",
                    "Known-group toast is not safely positioned")
            rectangle(toast.get("rect"), viewport)
            require(toast.get("dismiss", {}).get("visible") is True,
                    "Known-group toast cannot be dismissed")
            rectangle(toast["dismiss"].get("rect"), viewport)


def expected_requests(case, batches, held=False):
    rows = []
    for batch in range(batches):
        kinds = ("discovery", "directory", "direct") if case.endswith("errors") else (
            "discovery", "directory")
        # A held batch has only reached its directory request.
        if held and batch == batches - 1:
            kinds = ("discovery", "directory")
        for kind in kinds:
            pending = held and batch == batches - 1 and kind == "directory"
            if kind == "discovery":
                body = dict(include_external_workspaces="true",
                            _x_reason="client-needs-valid-flannel-url")
                reply = dict(raw=dict(ok=True, url=f"https://edgeapi.slack.com/cache/{TEAM}",
                                      ttl_seconds=300))
            elif kind == "directory":
                body = dict(ids=list(GROUPS))
                reply = (dict(error="offline") if case.endswith("errors") and batch == 0 else
                         dict(raw=dict(ok=True, results="malformed")) if case.endswith("errors") else
                         dict(raw=response(True, batch == 1)))
            else:
                body = dict(usergroups=",".join(GROUPS), include_users="false",
                            include_count="true", include_disabled="true",
                            _x_reason="fallback:fetchRawUserGroupsById")
                reply = dict(error="offline") if batch == 0 else dict(
                    raw=dict(ok=True, subteams="malformed"))
            row = dict(serial=len(rows) + 1, kind=kind,
                       method={"discovery": "api.getFlannelHttpUrl",
                               "directory": "usergroups/info", "direct": "subteams.info"}[kind],
                       body=body, account=ACCOUNT, team=TEAM, epoch=0, released=not pending)
            if not pending:
                row.update(response=reply, outcome="offline" if "error" in reply else "response",
                           receiverRetired=False, delivered=True)
            rows.append(row)
    return rows


def state_for(case, stage):
    """Explicit stage contract; no runtime receipt fields choose expected behavior."""
    i = CASES[case].index(stage)
    batches = 0 if i < 2 else 1
    if stage in ("equal-held", "equal-mounted", "reset-held", "malformed-mounted",
                 "final-reopened", "account-held", "account-replaced", "late-refused"):
        batches = 2
    held = stage in ("mounted-held", "edited-held", "equal-held", "reset-held",
                     "cancelled", "account-held", "account-replaced")
    replaced = stage in ("account-replaced", "late-refused")
    saved = case == "known-freshness" and i >= CASES[case].index("saved")
    source_renamed = ((case == "known-freshness" and i >= 4)
                      or (case.endswith("retirement") and i >= 4)) and not replaced
    editor_renamed = source_renamed and stage not in ("refreshed-mounted", "late-settled")
    editor = stage not in ("initial", "saved", "cancelled", "late-settled",
                           "account-replaced", "late-refused")
    edited = case == "known-freshness" and i >= 3
    return dict(batches=batches, held=held, replaced=replaced, saved=saved,
                source_renamed=source_renamed, editor_renamed=editor_renamed,
                editor=editor, edited=edited)


def quiet(value, require):
    require(value == dict(observedMs=500, noAdditionalRead=True, sourceRetained=True),
            "Missing observed quiet interval")


def native_custody(value, edited, renamed, require, *, remembered=False):
    expected = dict(ok=True, blocks=blocks(edited), tokens=[
        dict(value=token, label=label)
        for token, label in zip(blocks()[0]["elements"][0]["elements"][1:], labels(renamed))],
        start=2, end=8)
    if not remembered:
        expected.update(sameNode=True, sameSelection=True, noToast=True)
    require(value == expected, "Original known editor/node/selection/token custody changed")


def reset(value, ordinal, require):
    require(isinstance(value, dict) and value.get("team") == TEAM
            and value.get("actualAcceptedConnection") is True,
            "Missing actual accepted reset source")
    before, after = value.get("before", {}), value.get("after", {})
    require(before == dict(slowConnectionCount=ordinal, major=1)
            and after == value.get("duplicate") == dict(slowConnectionCount=ordinal + 1, major=1),
            "Reset manufactured slow count or repeated a generation")
    require(type(value.get("oldGeneration")) is int
            and type(value.get("generation")) is int
            and value["generation"] == value["oldGeneration"] + 1,
            "Reset lacks original socket generation transition")


def held_proof(value, row, ordinal, require):
    require(isinstance(value, dict) and all(value.get(k) is True for k in
            ("immediateAdmission", "duplicateIdsCoalesced", "duplicateOpen")),
            "Known group admission waited for background acquisition")
    reset(value.get("reset"), ordinal, require)
    require(value.get("request") == row["model"]["requests"][-1]["serial"]
            and value.get("opening") == row["model"]["knownFreshness"]["opening"],
            "Held read borrowed another occurrence or opening")
    quiet(value.get("quiet"), require)


def profile(value, saved, replaced, require):
    if replaced:
        require(value.get("enabled") is False, "Retired Profile remained enabled")
        return
    require(value.get("enabled") is True and value.get("blocks") == blocks(saved)
            and value.get("dirty") is (not saved), "Known Profile form draft changed")
    require(value.get("account") == ACCOUNT and value.get("epoch") == 0
            and value.get("busy") is False and value.get("uncertain") is False,
            "Known Profile ownership or pending state changed")
    require(value.get("choiceValue") == "Morning" and value.get("countryValue") == "US"
            and value.get("managedValue") == "Preserved organization value",
            "Known Profile sibling fields changed")
    require(value.get("reads") == int(saved) and isinstance(value.get("attempts"), list)
            and len(value["attempts"]) == int(saved),
            "Unexpected Profile write/readback count")
    if saved:
        attempt = value["attempts"][0]
        require(isinstance(attempt, list), "Missing actual Profile Save wire")
        long = [field for field in attempt if field.get("element_id") == "long"]
        require(len(long) == 1 and isinstance(long[0].get("long_text", {}).get("blocks"), str),
                "Missing original long-text Save payload")
        try:
            wire = json.loads(long[0]["long_text"]["blocks"])
        except (ValueError, TypeError) as error:
            raise RuntimeError("Malformed Profile Save blocks") from error
        require(wire == blocks(True), "Save changed edited text or original opaque tokens")


def scheduler(value, expected, require, *, errors=False):
    batches, held = expected["batches"], expected["held"]
    require(isinstance(value, dict) and value.get("scope") == dict(
        account=ACCOUNT, team=TEAM, user=SELF, epoch=0), "Wrong known freshness scope")
    require(value.get("started") == batches and value.get("finished") == batches - int(held)
            and value.get("worker") is held and value.get("pending") == [],
            "Known manager duplicate work or incorrect settlement")
    require(value.get("inFlight") == (list(GROUPS) if held else None)
            and value.get("freshIds") == ([] if held else list(GROUPS)),
            "Attempt freshness conflated with pending or canonical metadata")
    require(value.get("observations") == dict(slowConnectionCount=batches + 1, major=1),
            "Manager did not observe the actual reset")
    completed = batches - int(held)
    last = value.get("lastSettlement")
    if completed == 0:
        require(last is None, "Unattempted group acquired a settlement")
    else:
        require(isinstance(last, dict) and last.get("ids") == last.get("attempted") == list(GROUPS)
                and last.get("timedOut") is False, "Wrong original completed batch")
        require(last.get("adopted") is (not errors)
                and type(last.get("errors")) is int and last["errors"] == (2 if errors else 0),
                "Wrong actual adoption/error outcome")


def validate_known(receipt, captures, viewport, require, rectangle, validate_png, png):
    case = receipt["case"]
    rows = receipt["stages"]
    final_expected = expected_requests(case, 2)
    require(receipt.get("requests") == final_expected, "Wrong actual known-group request/response ledger")
    for row in rows:
        stage = row["stage"]
        expected = state_for(case, stage)
        model, geometry, proof = row.get("model", {}), row.get("geometry"), row.get("proof", {})
        replaced = expected["replaced"]
        require(model.get("account") == (FOREIGN if replaced else ACCOUNT)
                and model.get("epoch") == int(replaced), "Wrong known-group stage account")
        wanted_requests = expected_requests(case, expected["batches"], expected["held"])
        require(model.get("requests") == wanted_requests, "Known stage changed original endpoint ledger")
        require(model.get("pending") == ([wanted_requests[-1]] if expected["held"] else []),
                "Wrong original held receiver inventory")
        require(model.get("driver") == dict(started=0, finished=0, active=0),
                "Known selector used MissingOpening recovery")
        profile(model.get("profile", {}), expected["saved"], replaced, require)
        known = model.get("knownFreshness", {})
        require(known.get("seed") == seed() and known.get("networkCalls") == 0,
                "Known freshness lacks original source seed")
        resets = known.get("resets")
        require(isinstance(resets, list) and len(resets) == expected["batches"],
                "Unexpected lifecycle reset count")
        for ordinal, value in enumerate(resets, 1):
            reset(value, ordinal, require)
        if replaced:
            require(known.get("source") is None and known.get("scheduler") is None
                    and known.get("opening") is None, "Old known work leaked into successor owner")
            require(model.get("groups") == {
                group: dict(native=None, source=None, label=None) for group in GROUPS},
                "Old known group published into successor")
        else:
            source(known.get("source"), model.get("groups"), expected["source_renamed"], require)
            scheduler(known.get("scheduler"), expected, require, errors=case.endswith("errors"))
            opening = known.get("opening", {})
            require(opening.get("visit") == model["profile"].get("visit")
                    and opening.get("serial") == model["profile"].get("serial")
                    and type(opening.get("key")) is int and opening["key"] >= 0
                    and opening.get("mounted") is expected["editor"]
                    and opening.get("reading") is False and opening.get("lease") is False,
                    "Known editor borrowed a recovery lease or another section")
        layout(geometry, viewport, editor=expected["editor"],
               renamed=expected["editor_renamed"], edited=expected["edited"],
               replaced=replaced, saved=stage == "saved", can_save=False,
               require=require, rectangle=rectangle)
        validate_proof(row, case, expected, require)
        path = Path(receipt["captures"][stage])
        require(path.resolve() == (captures / f'{receipt["fixture"]}-{stage}.png').resolve(),
                "Known capture escaped original path")
        dimensions = geometry.get("pngDimensions")
        require(isinstance(dimensions, list) and len(dimensions) == 2
                and all(type(v) is int and v > 0 for v in dimensions),
                "Missing original known-group PNG dimensions")
        if png:
            validate_png(path, dimensions)
    return receipt


def validate_proof(row, case, expected, require):
    stage, proof = row["stage"], row.get("proof", {})
    model = row["model"]
    known = model["knownFreshness"]
    if stage == "initial":
        require(proof == dict(originalDraft=True, actualSourceSink=True), "Missing known seed execution")
    if stage == "fresh-open":
        require(proof.get("noRead") is True and "quiet" in proof,
                "Fresh known row performed a read or lacks quiet observation")
    if stage in ("edited-held", "equal-held", "reset-held", "offline-mounted"):
        require("remembered" in proof, "Missing original native node witness")
    if stage in ("offline-mounted", "malformed-mounted"):
        require("quiet" in proof, "Missing quiet error settlement")
        for key, request in zip(("directoryRelease", "fallbackRelease"), model["requests"][-2:]):
            require(proof.get(key) == dict(serial=request["serial"], receiverRetired=False,
                                           delivered=True), "Wrong original error receiver")
    if stage == "malformed-mounted":
        require(proof.get("directoryRaw") == dict(ok=True, results="malformed")
                and proof.get("directRaw") == dict(ok=True, subteams="malformed"),
                "Wrong original malformed responses")
    if "quiet" in proof:
        quiet(proof["quiet"], require)
    if stage in ("mounted-held", "account-held"):
        held_proof(proof, row, expected["batches"], require)
    if stage in ("equal-held", "reset-held"):
        held_proof(proof.get("held"), row, expected["batches"], require)
    if "remembered" in proof:
        native_custody(proof["remembered"], expected["edited"], expected["editor_renamed"],
                       require, remembered=True)
    if "custody" in proof:
        native_custody(proof["custody"], expected["edited"], expected["editor_renamed"], require)
    if stage == "edited-held":
        edit = proof.get("nativeEdit", {})
        before, after = edit.get("before", {}), edit.get("after", {})
        require(edit.get("ok") is True and before.get("snapshot", {}).get("blocks") == blocks()
                and after.get("snapshot", {}).get("blocks") == blocks(True)
                and before.get("tokens") == after.get("tokens") == row["geometry"]["captured"]["tokens"],
                "Native edit changed original group tokens")
        a, b = before["snapshot"], after["snapshot"]
        require(a.get("generation") == b.get("generation") == row["geometry"]["captured"]["snapshot"]["generation"]
                and type(a.get("revision")) is int and type(b.get("revision")) is int
                and b["revision"] > a["revision"] and not a.get("composing") and not b.get("composing"),
                "Native edit lost original editor generation")
    if stage in ("refreshed-mounted", "equal-mounted", "offline-mounted",
                 "malformed-mounted", "late-settled"):
        require(proof.get("settled") == known["scheduler"], "Settlement does not match current manager")
    if stage in ("refreshed-mounted", "late-settled", "equal-mounted", "late-refused"):
        require(proof.get("response") == response(True, stage in ("equal-mounted", "late-refused")),
                "Missing exact original rename/equal-version response")
        last = model["requests"][-1]
        require(proof.get("release") == dict(serial=last["serial"],
                receiverRetired=False, delivered=True),
                "Original known receiver disposition changed")
    if stage == "refreshed-mounted":
        require(proof.get("mountedOldLabels") is True and proof.get("source") == known["source"]
                and "custody" in proof, "Refreshed source rewrote current editor")
    if stage in ("equal-mounted", "offline-mounted", "malformed-mounted"):
        require(proof.get("sourceBefore") == proof.get("sourceAfter") == known["source"]
                and "custody" in proof and (stage != "equal-mounted" or proof.get("equalVersionSkipped") is True),
                "Failed/equal response changed canonical source")
    if stage == "saved":
        require(proof == dict(actualProfileSaveReadback=True, rawDuplicatesPreserved=True),
                "Missing original Profile Save/readback")
    if stage in ("reopened", "fresh-reopened", "final-reopened"):
        require(proof.get("noRedundantRead") is True and "quiet" in proof,
                "Fresh reopen repeated work")
        if stage == "reopened":
            require(proof.get("newLabels") is True, "Next opening failed to adopt renamed label")
    if stage == "cancelled":
        require(proof.get("accountOwnedReadRetained") is True and proof.get("after") == model
                and proof.get("before", {}).get("profile", {}).get("blocks") == model["profile"]["blocks"],
                "Cancel changed original draft or consumed account-owned read")
    if stage == "late-settled":
        require(proof.get("cancelledConsumerNotRemounted") is True,
                "Late group result remounted cancelled editor")
    if stage == "account-replaced":
        require(proof.get("actualInstallLoaded") is True and proof.get("after") == model
                and proof.get("before", {}).get("account") == ACCOUNT
                and proof.get("originalWorkerAlive") is True,
                "Missing actual account replacement")
    if stage == "late-refused":
        require(proof.get("successorUnchanged") is True and proof.get("after") == model
                and proof.get("originalWorkerFinished") is True,
                "Missing original worker completion or successor comparison")
        for key in ("account", "epoch", "groups", "profile", "knownFreshness"):
            require(proof.get("before", {}).get(key) == model.get(key),
                    "Late original response changed successor")
