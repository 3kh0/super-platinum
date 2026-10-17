"""Strict configured Entity receipt checks. Pure: never launches or reads a session.
The binary/source binding and owned screenshot verification belong to the native runner.
"""
import copy
import json
import re

STAGES = ("entity-format", "entity-defaults", "field-edit", "field-saving", "uncertain",
          "conflict", "retired", "observed", "table", "entity-cell", "entity-canceled")
CASES = tuple(f"slack-list-{domain}-defaults-{mode}-create"
              for domain in ("people", "channel") for mode in ("single", "multiple", "null", "omitted"))
FLAG = "SUPER_PLATINUM_LIST_ENTITY_FIELD_CREATE_CHECK"
ENV_PREFIX = "SUPER_PLATINUM_LIST_ENTITY_FIELD_CREATE"


def require(condition, message):
    if not condition:
        raise ValueError(message)


def case_for(fixture):
    require(isinstance(fixture, str), "fixture type")
    prefix = fixture.removesuffix("-narrow")
    require(prefix in CASES, "unknown fixture")
    domain, mode = prefix.split("-")[2], prefix.split("-")[4]
    return prefix, domain == "channel", mode


def same(a, b):
    return json.dumps(a, sort_keys=True, separators=(",", ":")) == json.dumps(b, sort_keys=True, separators=(",", ":"))


def options(channel, mode):
    out = {"format": "multi_entity" if mode in ("multiple", "omitted") else "single_entity",
           "show_member_name": channel}
    if mode == "null":
        out["default_value"] = None
    elif mode != "omitted":
        ids = ["G00000002", "C00000001"] if channel else ["U00000002", "U00000001"]
        out.update(default_value="|".join(ids), default_value_typed={"channel" if channel else "user": ids})
    return out


def transitions(channel, mode):
    """Independent literal UI plan: retains two through Single, clears, removes, refills."""
    b, a = (["G00000002", "C00000001"] if channel else ["U00000002", "U00000001"])
    result = []
    def add(kind, value, multiple, ids=None, omitted=False):
        wire = {"format": "multi_entity" if multiple else "single_entity", "show_member_name": channel}
        if ids is not None:
            wire.update(default_value="|".join(ids), default_value_typed={"channel" if channel else "user": ids})
        elif not omitted:
            wire["default_value"] = None
        result.append(({"kind": kind, "value": value}, wire))
    if not channel:
        add("show", False, False)
    add("multiple", True, True)
    add("activate", b, True, [b])
    add("activate", a, True, [b, a])
    add("multiple", False, False, [b, a])
    add("activate", b, False, [b])
    add("activate", b, False)
    add("multiple", True, True)
    add("activate", b, True, [b])
    add("remove", b, True, omitted=True)
    add("activate", b, True, [b])
    add("activate", a, True, [b, a])
    if mode in ("single", "null"):
        add("multiple", False, False, [b, a])
    if mode == "null":
        add("activate", b, False, [b])
        add("activate", b, False)
    if mode == "omitted":
        add("remove", b, True, [a])
        add("remove", a, True, omitted=True)
    return result


def expected_ledger(before, after, channel, mode, serial):
    def req(path, fields):
        return {"method": "POST", "path": "/api/" + path, "fields": [["token", "fixture-not-a-token"], *fields]}
    file = req("files.info", [["file", "F_LIST_FIXTURE"]])
    team = req("team.prefs.get", [["target_team", "T1"], ["_x_reason", "native-list-permission"]])
    user = req("users.prefs.get", [["prefs", "task_lists,assigned_to_list"], ["_x_reason", "native-list-completion-permission"]])
    name = ("Channel" if channel else "Reviewer") + " \u00c9quipe \u6771\u4eac"
    create = req("lists.columns.create", [["list_id", "F_LIST_FIXTURE"], ["view_id", "V_FILTER"],
        ["name", name], ["type", "channel" if channel else "user"],
        ["options", json.dumps(options(channel, mode), sort_keys=True, ensure_ascii=False, separators=(",", ":"))],
        ["_x_reason", "create-column"]])
    reads = [file, team, user]
    team_result = {"ok": True, "prefs": {"allow_lists": "full"}}
    user_result = {"ok": True, "prefs": {"task_lists": "[]", "assigned_to_list": ""}}
    def responses(doc):
        return [{"ok": True, "file": doc}, team_result, user_result]
    return {"requests": reads + [create] + reads + [file] + reads,
        "responses": responses(before) + [{"syntheticFailure": "response_lost"}] + responses(before) + [{"ok": True, "file": after}] + responses(after),
        "finished": [{"serial": serial, "kind": kind, "applied": applied} for kind, applied in
                     (("write", False), ("check", True), ("check", False), ("check", True))],
        "server": after, "writePending": False, "readPending": False}


def configuration(value, channel, mode, empty_ledger):
    initial = value["initial"]
    require(initial["wire"] == {"format": "single_entity", "show_member_name": True, "default_value": None}, "initial default")
    require(initial["enabled"] is True, "configuration disabled")
    plan = transitions(channel, mode)
    require(len(value["steps"]) == len(plan), "configuration plan length")
    previous = initial
    for step, (action, wire) in zip(value["steps"], plan):
        require(step["before"] == previous and step["action"]["ok"] is True and step["action"]["action"] == action, "action chain")
        after = step["after"]
        require(all(after[key] == initial[key] for key in ("serial", "visit", "selection", "choice")), "configuration owner")
        require(type(after["epoch"]) is int and after["epoch"] > 0 and after["epoch"] != previous["epoch"], "configuration epoch")
        require(same(after["wire"], wire) and step["verified"]["model"] == after and step["verified"]["dom"]["ok"] is True, "exact configuration")
        if action["kind"] == "remove":
            require(step["verified"]["dom"]["removeFocus"]["ok"] is True, "remove focus handshake")
        previous = after
    require(value["final"]["model"] == previous and value["final"]["dom"]["ok"] is True and same(previous["wire"], options(channel, mode)), "final configuration")
    require(value["ledgerBefore"] == empty_ledger == value["ledgerAfter"], "configuration dispatched")


def validate(receipt, fixture, owner, viewport):
    try:
        return _validate(receipt, fixture, owner, viewport)
    except (KeyError, IndexError, TypeError, OverflowError) as error:
        raise ValueError("Malformed Entity receipt shape") from error


def _validate(receipt, fixture, owner, viewport):
    """Returns receipt unchanged after closed identity, raw-wire and lifecycle checks."""
    prefix, channel, mode = case_for(fixture)
    require(receipt["ok"] is True and receipt["fixture"] == fixture and receipt["entityCase"] == prefix, "receipt identity")
    require(receipt["owner"] == owner and owner["fixture"] == fixture and owner["version"] == 1
            and type(owner["pid"]) is int and owner["pid"] > 0 and re.fullmatch(r"[0-9a-f]{32}", owner["nonce"]), "process owner")
    require(receipt["viewport"] == list(viewport) and len(viewport) == 2 and all(type(x) is int and x > 0 for x in viewport), "viewport")
    require(re.fullmatch(r"number-precision-fixture-[1-9][0-9]*", receipt["account"]), "synthetic account")
    require(receipt["noSession"] is True and receipt["noTransport"] is True and type(receipt["httpExecuted"]) is int and receipt["httpExecuted"] == 0, "offline only")
    require(type(receipt["actualCreateForms"]) is int and receipt["actualCreateForms"] == 1 and type(receipt["creationReads"]) is int and receipt["creationReads"] == 10, "one create ten reads")
    require(receipt["mode"] == ("channel" if channel else "user") and same(receipt["expectedOptions"], options(channel, mode)), "configured options")
    before, after, serial = receipt["before"], receipt["after"], receipt["admitted"]["serial"]
    require(type(serial) is int and serial > 0, "intent serial")
    expected = copy.deepcopy(before)
    column_id = "C_CHANNEL_OBSERVED" if channel else "C_PEOPLE_OBSERVED"
    key = "channel_observed" if channel else "people_observed"
    name = ("Channel" if channel else "Reviewer") + " \u00c9quipe \u6771\u4eac"
    expected["list_metadata"]["schema"].insert(1, {"id": column_id, "key": key, "name": name, "type": "channel" if channel else "user", "options": options(channel, mode), "opaque": {"fixture": "retained"}})
    expected["list_metadata"]["views"][0]["columns"].insert(1, {"id": column_id, "key": key, "visible": True})
    require(same(after, expected), "raw readback addition")
    ledger = expected_ledger(before, after, channel, mode, serial)
    require(same(receipt["ledger"], ledger), "exact create/read/retirement ledger")
    empty = {"requests": [], "responses": [], "finished": [], "server": before, "writePending": False, "readPending": False}
    require(receipt["cancel"]["ledger"] == empty and receipt["cancel"]["focus"]["ok"] is True, "Cancel authority")
    configuration(receipt["cancel"]["configuration"], channel, mode, empty)
    configuration(receipt["configuration"], channel, mode, empty)
    require(receipt["cancel"]["draft"]["model"]["serial"] != serial, "Cancel serial reused")
    require(receipt["configuration"]["final"]["model"]["serial"] == serial, "prepared configuration owner")
    population = receipt["population"]
    require(len(population["records"]) == 73 and population["response_metadata"]["next_cursor"] == "precision-opaque+/=", "partial raw population")
    require(all(all(f.get("key") != key and f.get("column_id") != column_id for f in row["fields"]) for row in population["records"]), "raw backfill")
    require([s["stage"] for s in receipt["stages"]] == list(STAGES) and set(receipt["captures"]) == set(STAGES), "stage inventory")
    require(len(set(receipt["captures"].values())) == len(STAGES), "duplicate capture")
    for stage in receipt["stages"]:
        require(stage["noSession"] is True and stage["noTransport"] is True and stage["geometry"]["ok"] is True and stage["geometry"]["viewport"] == list(viewport), "stage isolation/geometry")
        if "population" in stage:
            require(stage["population"] == population and stage["find"] == receipt["find"], "raw stage population/Find")
    require(receipt["stages"][7]["geometry"]["observedVisible"] is True, "observed summary hidden")
    retired = receipt["retired"]
    require(retired["oldVisit"] != retired["newVisit"] and retired["oldSelection"] != retired["newSelection"] and retired["newEpoch"] == retired["oldEpoch"] + 1, "retired owner")
    require(retired["completed"]["finished"] == ledger["finished"][:3] and retired["completed"]["requests"] == ledger["requests"][:8], "retired read applied/replayed")
    require(receipt["disabledNoReplay"]["ok"] is True and receipt["doneFocus"]["ok"] is True, "locked/Done controls")
    cells = receipt["cellCompatibility"]
    require(cells["cellAttempts"] == 0 and cells["noBackfill"] is True and cells["ledgerUnchanged"] is True, "cell mutation")
    require(all(cells[k]["ok"] is True for k in ("initial", "local", "returned", "reopen", "final")), "cell native controls")
    require(cells["initial"]["selected"] == [] == cells["reopen"]["selected"] and cells["initial"]["generation"] != cells["reopen"]["generation"], "cell cancellation seed")
    typed = cells["typed"]
    require(typed["singleAdmitted"] is True and typed["multipleAdmitted"] is (mode in ("multiple", "omitted")) and typed["notExecuted"] is True and typed["rawRowsUnchanged"] is True, "cell cardinality")
    from list_entity_create_evidence import validate as validate_evidence
    validate_evidence(receipt, channel, mode, list(viewport))
    return receipt
