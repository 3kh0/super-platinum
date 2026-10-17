"""Constructed emitter-shaped fixtures, NOT native observations.

Intentionally imports no validator or production constructor. Values below are
literal protocol expectations read from entity.rs/configuration.rs/creation.js.
"""
import copy
import json
from pathlib import Path

KINDS = [["text", "Text"], ["number", "Number"], ["checkbox", "Checkbox"], ["date", "Date"], ["select", "Select"], ["user", "People"], ["link", "Link"], ["channel", "Channel"], ["rating", "Rating"], ["vote", "Vote"]]
NAMES = ("entity-format", "entity-defaults", "field-edit", "field-saving", "uncertain", "conflict", "retired", "observed", "table", "entity-cell", "entity-canceled")


def sample(fixture):
    prefix = fixture.removesuffix("-narrow")
    channel = prefix.split("-")[2] == "channel"
    mode = prefix.split("-")[4]
    multi = mode in ("multiple", "omitted")
    vp = [668, 380] if fixture.endswith("-narrow") else [1280, 780]
    seed = json.loads(Path(__file__).with_name("list_entity_create_seed.json").read_text(encoding="utf-8"))
    before, population = seed["before"], seed["population"]
    kind = "channel" if channel else "user"
    name = ("Channel" if channel else "Reviewer") + " \u00c9quipe \u6771\u4eac"
    col, key = ("C_CHANNEL_OBSERVED", "channel_observed") if channel else ("C_PEOPLE_OBSERVED", "people_observed")
    b, a = ("G00000002", "C00000001") if channel else ("U00000002", "U00000001")
    choices = ([["C00000001", "#alpha-public"], ["G00000002", "#beta-private"], ["C00000003", "#gamma-external"]] + [[f"C{i:08}", f"#lane-{i:02}"] for i in range(4, 11)] + [["G00000011", "#record-member"], ["C00000012", "#zulu-external"]]) if channel else ([["U00000001", "Alice Example"], ["U00000002", "Bob Guest"]] + [[f"U{i:08}", f"Person {i:02}"] for i in range(3, 12)] + [["W00000012", "Whitney External"]])

    def wire(multiple, ids=None, omitted=False, show=channel):
        v = {"format": "multi_entity" if multiple else "single_entity", "show_member_name": show}
        if ids is not None:
            v.update(default_value="|".join(ids), default_value_typed={kind: ids})
        elif not omitted:
            v["default_value"] = None
        return v
    wanted = wire(multi, None if mode in ("null", "omitted") else [b, a], mode == "omitted")
    after = copy.deepcopy(before)
    after["list_metadata"]["schema"].insert(1, {"id": col, "key": key, "name": name, "type": kind, "options": wanted, "opaque": {"fixture": "retained"}})
    after["list_metadata"]["views"][0]["columns"].insert(1, {"id": col, "key": key, "visible": True})
    obs = {"id": col, "key": key, "name": name}
    def request(method, fields):
        return {"method": "POST", "path": "/api/" + method, "fields": [["token", "fixture-not-a-token"], *fields]}
    file = request("files.info", [["file", "F_LIST_FIXTURE"]])
    team = request("team.prefs.get", [["target_team", "T1"], ["_x_reason", "native-list-permission"]])
    user = request("users.prefs.get", [["prefs", "task_lists,assigned_to_list"], ["_x_reason", "native-list-completion-permission"]])
    write = request("lists.columns.create", [["list_id", "F_LIST_FIXTURE"], ["view_id", "V_FILTER"], ["name", name], ["type", kind], ["options", json.dumps(wanted, ensure_ascii=False, sort_keys=True, separators=(",", ":"))], ["_x_reason", "create-column"]])
    requests = [file, team, user, write, file, team, user, file, file, team, user]
    tr = {"ok": True, "prefs": {"allow_lists": "full"}}
    ur = {"ok": True, "prefs": {"task_lists": "[]", "assigned_to_list": ""}}
    responses = [{"ok": True, "file": before}, tr, ur, {"syntheticFailure": "response_lost"}, {"ok": True, "file": before}, tr, ur, {"ok": True, "file": after}, {"ok": True, "file": after}, tr, ur]
    finished = [{"serial": 99, "kind": k, "applied": a} for k, a in (("write", False), ("check", True), ("check", False), ("check", True))]
    def ledger(n=0, f=0, server=False, write=False, read=False):
        return copy.deepcopy({"requests": requests[:n], "responses": responses[:n], "finished": finished[:f], "server": after if server else before, "writePending": write, "readPending": read})
    def model(serial, option, epoch, enabled=True, visit=2, selection=3):
        labels = dict(choices)
        return {"serial": serial, "visit": visit, "selection": selection, "choice": 4, "epoch": epoch, "enabled": enabled, "wire": copy.deepcopy(option), "selected": [[id_, labels[id_]] for id_ in option.get("default_value_typed", {}).get(kind, [])], "directory": {"query": "", "loading": False, "error": None, "candidates": copy.deepcopy(choices)}}
    def snapshot(m, transition=False, remove=None):
        ids = [v[0] for v in m["selected"]]
        multi = m["wire"]["format"] == "multi_entity"
        display = ids if multi else ids[:1]
        v = {"ok": True, "owner": {k: str(m[k]) for k in ("serial", "visit", "selection", "choice", "epoch")}, "multiple": multi, "showNames": None if channel else m["wire"]["show_member_name"], "query": "", "inventory": copy.deepcopy(choices), "display": display, "checked": [id_ for id_, _ in choices if id_ in display], "removes": ids if multi else [], "hint": "Current default: " + m["selected"][0][1] if not multi and ids else None, "locked": not m["enabled"]}
        if transition:
            v["removeFocus"] = remove
        return v
    def verified(m, remove=None):
        return {"model": copy.deepcopy(m), "dom": snapshot(m, True, remove)}
    def config(serial):
        initial = model(serial, wire(False, show=True), 5)
        plan = []
        if not channel:
            plan.append(({"kind": "show", "value": False}, wire(False)))
        plan += [({"kind": "multiple", "value": True}, wire(True)), ({"kind": "activate", "value": b}, wire(True, [b])), ({"kind": "activate", "value": a}, wire(True, [b, a])), ({"kind": "multiple", "value": False}, wire(False, [b, a])), ({"kind": "activate", "value": b}, wire(False, [b])), ({"kind": "activate", "value": b}, wire(False)), ({"kind": "multiple", "value": True}, wire(True)), ({"kind": "activate", "value": b}, wire(True, [b])), ({"kind": "remove", "value": b}, wire(True, omitted=True)), ({"kind": "activate", "value": b}, wire(True, [b])), ({"kind": "activate", "value": a}, wire(True, [b, a]))]
        if not multi:
            plan.append(({"kind": "multiple", "value": False}, wire(False, [b, a])))
        if mode == "null":
            plan += [({"kind": "activate", "value": b}, wire(False, [b])), ({"kind": "activate", "value": b}, wire(False))]
        if mode == "omitted":
            plan += [({"kind": "remove", "value": b}, wire(True, [a])), ({"kind": "remove", "value": a}, wire(True, omitted=True))]
        previous, steps = initial, []
        for action, option in plan:
            after_model = model(serial, option, previous["epoch"] + 1)
            remove = None
            if action["kind"] == "remove":
                ids = [v[0] for v in previous["selected"]]
                n = ids.index(action["value"])
                target = ids[n + 1] if n + 1 < len(ids) else ids[n - 1] if n else None
                remove = {"ok": True, "target": "slack-list-entity-default-query" if target is None else None, "defaultId": target}
            steps.append({"before": previous, "action": {"ok": True, "scrolling": [], "rect": [40, 100, 20, 20], "focusOutlineFits": True, "action": action, "before": snapshot(previous)}, "after": after_model, "verified": verified(after_model, remove)})
            previous = after_model
        return {"initial": initial, "steps": steps, "final": verified(previous), "ledgerBefore": ledger(), "ledgerAfter": ledger()}
    config_old, configured = config(8), config(99)
    final = configured["final"]["model"]
    def outer(serial=99, visit=2, selection=3, observed=False, title=name, field=kind, choice=4):
        return {"serial": serial, "visit": visit, "selection": selection, "name": title, "view": "V_FILTER", "observed": obs if observed else None, "kind": field, "choice": choice}
    def dom(m):
        return {"ok": True, "serial": str(m["serial"]), "name": m["name"], "nameDisabled": False, "kind": m["kind"], "kindDisabled": False, "choice": str(m["choice"]), "nameChoice": str(m["choice"]), "copy": True, "title": "Add field", "options": copy.deepcopy(KINDS), "hint": "Shared with everyone who has access to this List.", "focused": "slack-list-rename-name", "observed": None}
    def activation(selector):
        return {"ok": True, "selector": "#slack-list-" + selector, "hitPoints": 9, "scrolling": []}
    initial = outer(8, title="", field="text", choice=3)
    canceled = {"model": outer(8), "dom": dom(outer(8)), "input": {"ok": True, "value": name, "scrolling": []}, "activation": activation("new-text-field"), "initial": initial, "initialDom": dom(initial), "selected": {"event": {"ok": True, "value": kind, "scrolling": []}, "model": outer(8, title="")}}
    returned_add = {"ok": True, "sameNode": True, "focused": True, "disabled": False}
    find = {"input": "Launch item 0", "applied": "Launch item 0", "revision": 9}
    binding = {"ok": True, "value": "Launch item 0", "range": [13, 13, "none"]}
    def inspect(part, m, focused):
        v = {"ok": True, "part": part, "scrolling": [], "focusRetained": True, "focused": focused, "rect": [30, 70, 300, 28], "text": "" if part != "summary" else "Observed " + ("Channel" if channel else "People") + " field: " + name, "visible": True, "snapshot": snapshot(m), "viewport": vp}
        if part != "summary":
            v["value"] = "on" if part == "format" else ""
        return v
    config_focus = "slack-list-entity-multiple" if mode == "single" else "slack-list-entity-default-query" if mode == "omitted" else None
    stages = [{"stage": "entity-" + ("defaults" if part == "default" else part), "model": final, "geometry": inspect(part, final, config_focus), "ledger": ledger(), "noSession": True, "noTransport": True} for part in ("format", "default")]
    def control(id_):
        return {"id": "slack-list-" + id_, "hit": True, "disabled": False, "rect": [40, 310, 100, 32]}
    for i, stage_name in enumerate(NAMES[2:9]):
        visit, selection = (2, 3) if i < 4 else (12, 13)
        m = model(99, wanted, final["epoch"], i == 0, visit, selection)
        draft = outer(visit=visit, selection=selection, observed=i == 5)
        if i < 6:
            active = ("name", "leave", "leave", "check", "check", "close")[i]
            last = ("name", "confirm", "confirm", "check", "leave", "check")[i]
            buttons = ("leave", "close", "confirm") if i == 0 else ("leave",) if i == 1 else ("leave", "close", "check") if i == 3 else ("leave", "close") if i == 5 else ("leave", "check")
            g = {"ok": True, "phase": ("Editing", "Saving", "Uncertain", "Conflict", "Uncertain", "Observed")[i], "panel": [20, 20, 600, 340], "controls": [control("rename-" + b) for b in buttons], "observedVisible": True, "focus": {"ok": True, "retainedAction": i in (0, 3), "actionId": "slack-list-rename-" + last, "activeId": "slack-list-rename-" + active, "id": "slack-list-rename-" + active, "ringFits": True}, "viewport": vp, "settings": snapshot(m)}
            reveal = inspect("summary", m, "slack-list-rename-close") if i == 5 else None
            configuration = verified(m)
        else:
            draft, configuration = None, None
            selectors = ("new-text-field", "rename-close", "new-text-field", "rename-confirm", "rename-check", "rename-check", "rename-leave", "rename-check", "rename-close")
            g = {"ok": True, "stage": "table", "phase": None, "actualLayout": True, "bounded": True, "ordered": True, "rowIds": ["R_LIST_003", "R_LIST_001", "R_LIST_007", "R_LIST_005"], "owner": {"visit": "12", "selection": "13", "serial": None, "view": None}, "selectors": True, "controls": [control("new-text-field")], "exactButtons": True, "numberActions": [], "numberReady": True, "focused": "slack-list-new-text-field", "focusedCorrectly": True, "draft": None, "find": {"ok": True, "sameNode": True, "value": "Launch item 0", "range": [13, 13, "none"]}, "findVisible": True, "partial": True, "partialText": "Only part of this List is loaded.", "rowVisible": True, "observedVisible": True, "first": {"id": "R_LIST_003", "primary": [30, 200, 180, 32], "created": [210, 200, 180, 32], "heading": [210, 168, 180, 32], "primaryText": "Launch item 04", "createdText": ""}, "selectedView": "V_FILTER", "selectedLabel": "Shared checklist", "returnedAdd": returned_add, "panel": None, "viewport": vp, "activations": [activation(s) for s in selectors]}
            reveal = {"ok": True, "restored": returned_add, "close": {"reachable": True, "rect": [560, 30, 32, 32], "scrolling": []}, "add": {"reachable": True, "scrolling": []}, "rowScroll": [], "focusRetained": returned_add}
        custody = None if i in (0, 6) else {"serial": 99, "document": before, "population": population, "intentDocument": before, "intentPopulation": population, "name": name, "view": "V_FILTER", "user": "U0", "kind": kind, "choice": 4, "options": wanted}
        stages.append({"stage": stage_name, "ordinal": i, "geometry": g, "reveal": reveal, "configuration": configuration, "draft": draft, "custody": custody, "ledger": ledger((0, 4, 4, 7, 8, 11, 11)[i], (0, 0, 1, 2, 3, 4, 4)[i], i >= 2, write=i == 1), "document": after if i >= 5 else before, "population": population, "find": find, "noSession": True, "noTransport": True})
    def cell_empty(g):
        return {"ok": True, "selected": [], "candidateTypes": ["checkbox" if multi else "radio"] * 12, "generation": str(g), "focusOutlineFits": True, "viewport": vp}
    typed = {"singleAdmitted": True, "multipleAdmitted": multi, "createDefaultIds": wanted.get("default_value_typed"), "rawRowsUnchanged": True, "notExecuted": True}
    local = {"ok": True, "generation": "80", "selected": [b], "save": [400, 310, 90, 32], "cancel": [500, 310, 90, 32], "viewport": vp}
    returned = {"ok": True, "sameCell": True, "instance": '["","R_LIST_003"]', "viewport": vp}
    cells = {"typed": typed, "initial": cell_empty(80), "local": local, "returned": returned, "reopen": cell_empty(90), "final": returned, "cellAttempts": 0, "noBackfill": True, "ledgerUnchanged": True}
    stages += [{"stage": "entity-cell", "geometry": local, "initial": cell_empty(80), "typed": typed, "ledger": ledger(11, 4, True), "noSession": True, "noTransport": True}, {"stage": "entity-canceled", "geometry": returned, "reopen": cell_empty(90), "ledger": ledger(11, 4, True), "noSession": True, "noTransport": True}]
    locked = snapshot(stages[3]["configuration"]["model"])
    return copy.deepcopy({"ok": True, "fixture": fixture, "entityCase": prefix, "mode": kind, "expectedOptions": wanted, "owner": {"version": 1, "nonce": "a" * 32, "pid": 123, "fixture": fixture}, "account": "number-precision-fixture-8", "viewport": vp, "noSession": True, "noTransport": True, "httpExecuted": 0, "actualCreateForms": 1, "creationReads": 10, "before": before, "after": after, "population": population, "find": find, "firstFind": binding, "cancel": {"draft": canceled, "configuration": config_old, "focus": returned_add, "ledger": ledger()}, "configuration": configured, "admitted": outer(), "disabledNoReplay": {"ok": True, "before": locked, "after": locked}, "retired": {"oldVisit": 2, "newVisit": 12, "oldSelection": 3, "newSelection": 13, "oldEpoch": 1, "newEpoch": 2, "held": ledger(8, 2, True, read=True), "completed": ledger(8, 3, True)}, "observed": outer(visit=12, selection=13, observed=True), "doneFocus": returned_add, "ledger": ledger(11, 4, True), "cellCompatibility": cells, "stages": stages, "captures": {s: "/owned/" + s + ".png" for s in NAMES}})
