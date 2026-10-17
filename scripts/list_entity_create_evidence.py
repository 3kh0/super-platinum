"""Independent emitted-evidence checks for the closed Entity fixture protocol.

No DOM evaluation or fixture production helpers are imported. Geometry uses the
same fixed 2**-16 CSS-pixel margin as the emitter, never a measured tolerance.
"""
import copy
import json
import math
import hashlib
from pathlib import Path

from list_entity_create_receipts import require, same, options, transitions, expected_ledger

SEED_SHA256 = "ebdd126acd28dea0ae5b0a598eed5eddec1073980d4e6bc3b0095f54827689e3"

KINDS = [["text", "Text"], ["number", "Number"], ["checkbox", "Checkbox"],
         ["date", "Date"], ["select", "Select"], ["user", "People"],
         ["link", "Link"], ["channel", "Channel"], ["rating", "Rating"], ["vote", "Vote"]]
COUNTS, FINISHED = (0, 4, 4, 7, 8, 11, 11), (0, 0, 1, 2, 3, 4, 4)
PHASES = ("Editing", "Saving", "Uncertain", "Conflict", "Uncertain", "Observed")
ORDER = ["R_LIST_003", "R_LIST_001", "R_LIST_007", "R_LIST_005"]


def fields(v, keys):
    require(type(v) is dict and set(v) == set(keys.split()), "exact fields: " + keys)


def integer(v, minimum=1):
    require(type(v) is int and v >= minimum, "integer")


def rectangle(v, viewport=None):
    require(type(v) is list and len(v) == 4 and all(type(x) in (int, float) and math.isfinite(x) for x in v), "finite rectangle")
    x, y, w, h = v
    require(w > 0 and h > 0, "positive rectangle")
    if viewport:
        require(min(x, y, viewport[0] - x - w, viewport[1] - y - h) >= -(2**-16), "viewport rectangle")


def scroll(v):
    require(type(v) is list, "scroll evidence")
    for item in v:
        fields(item, "id className before after")
        require(item["id"] is None or type(item["id"]) is str, "scroll id")
        require(type(item["className"]) is str, "scroll class")
        for k in ("before", "after"):
            require(type(item[k]) is list and len(item[k]) == 2 and all(type(x) in (int, float) and math.isfinite(x) for x in item[k]), "scroll coordinates")


def focus(v):
    require(same(v, {"ok": True, "sameNode": True, "focused": True, "disabled": False}), "exact returned Add focus")


def candidates(channel):
    if channel:
        return [["C00000001", "#alpha-public"], ["G00000002", "#beta-private"], ["C00000003", "#gamma-external"]] + [[f"C{i:08}", f"#lane-{i:02}"] for i in range(4, 11)] + [["G00000011", "#record-member"], ["C00000012", "#zulu-external"]]
    return [["U00000001", "Alice Example"], ["U00000002", "Bob Guest"]] + [[f"U{i:08}", f"Person {i:02}"] for i in range(3, 12)] + [["W00000012", "Whitney External"]]


def selected(wire, channel):
    names = dict(candidates(channel))
    ids = wire.get("default_value_typed", {}).get("channel" if channel else "user", [])
    return [[id_, names[id_]] for id_ in ids]


def model(v, channel, wire, enabled, owner=None):
    fields(v, "serial visit selection choice epoch enabled wire selected directory")
    for k in ("serial", "visit", "selection", "choice", "epoch"):
        integer(v[k])
    if owner is not None:
        require(all(v[k] == owner[k] for k in ("serial", "visit", "selection", "choice")), "model owner")
    require(v["enabled"] is enabled and same(v["wire"], wire) and same(v["selected"], selected(wire, channel)), "model configuration")
    require(same(v["directory"], {"query": "", "loading": False, "error": None, "candidates": candidates(channel)}), "exact seeded directory")


def snapshot(v, m, channel, transition=False, remove=None):
    fields(v, "ok owner multiple showNames query inventory display checked removes hint locked" + (" removeFocus" if transition else ""))
    ids = [c[0] for c in m["selected"]]
    multi = m["wire"]["format"] == "multi_entity"
    display = ids if multi else ids[:1]
    require(v["ok"] is True and same(v["owner"], {k: str(m[k]) for k in ("serial", "visit", "selection", "choice", "epoch")}), "DOM model owner")
    require(v["multiple"] is multi and v["showNames"] is (None if channel else m["wire"]["show_member_name"]) and v["query"] == "", "DOM controls")
    require(same(v["inventory"], candidates(channel)) and same(v["display"], display), "DOM directory/display")
    # Checked controls follow directory order, whereas display/removes preserve default order.
    require(v["checked"] == [id_ for id_, _ in candidates(channel) if id_ in display], "checked control identities")
    require(v["removes"] == (ids if multi else []) and v["hint"] == ("Current default: " + m["selected"][0][1] if not multi and ids else None), "default display semantics")
    require(v["locked"] is (not m["enabled"]), "settings lock")
    if transition:
        require(same(v["removeFocus"], remove), "exact Remove target witness")


def verified(v, m, channel, remove=None):
    fields(v, "model dom")
    require(same(v["model"], m), "verified model")
    snapshot(v["dom"], m, channel, True, remove)


def configuration(v, channel, mode, empty, viewport):
    fields(v, "initial steps final ledgerBefore ledgerAfter")
    initial = v["initial"]
    model(initial, channel, {"format": "single_entity", "show_member_name": True, "default_value": None}, True)
    require(same(v["ledgerBefore"], empty) and same(v["ledgerAfter"], empty), "configuration no dispatch")
    previous = initial
    plan = transitions(channel, mode)
    require(type(v["steps"]) is list and len(v["steps"]) == len(plan), "literal action inventory")
    for step, (action, wire) in zip(v["steps"], plan):
        fields(step, "before action after verified")
        require(same(step["before"], previous), "configuration predecessor")
        a = step["action"]
        fields(a, "ok scrolling rect focusOutlineFits action before")
        require(a["ok"] is True and a["focusOutlineFits"] is True and same(a["action"], action), "actual configuration event")
        scroll(a["scrolling"])
        rectangle(a["rect"], viewport)
        snapshot(a["before"], previous, channel)
        after = step["after"]
        model(after, channel, wire, True, initial)
        require(after["epoch"] != previous["epoch"], "epoch rotation")
        remove = None
        if action["kind"] == "remove":
            ids = [c[0] for c in previous["selected"]]
            i = ids.index(action["value"])
            target = ids[i + 1] if i + 1 < len(ids) else ids[i - 1] if i > 0 else None
            remove = {"ok": True, "target": "slack-list-entity-default-query" if target is None else None, "defaultId": target}
        verified(step["verified"], after, channel, remove)
        previous = after
    verified(v["final"], previous, channel)
    require(same(previous["wire"], options(channel, mode)), "final exact options")


def outer(v, serial, visit, selection, choice, channel, observed=False, name=None, kind=None):
    fields(v, "serial visit selection name view observed kind choice")
    kind = kind or ("channel" if channel else "user")
    name = name if name is not None else ("Channel" if channel else "Reviewer") + " \u00c9quipe \u6771\u4eac"
    obs = {"id": "C_CHANNEL_OBSERVED" if channel else "C_PEOPLE_OBSERVED", "key": "channel_observed" if channel else "people_observed", "name": name}
    require(same(v, {"serial": serial, "visit": visit, "selection": selection, "choice": choice, "view": "V_FILTER", "name": name, "kind": kind, "observed": obs if observed else None}), "outer immutable draft")


def activation(v, selector):
    fields(v, "ok selector hitPoints scrolling")
    require(v["ok"] is True and v["selector"] == selector and type(v["hitPoints"]) is int and v["hitPoints"] == 9, "native activation")
    scroll(v["scrolling"])


def dom_draft(v, m, channel):
    fields(v, "ok serial name nameDisabled kind kindDisabled choice nameChoice copy title options hint focused observed")
    require(v["ok"] is True and v["serial"] == str(m["serial"]) and v["name"] == m["name"] and v["kind"] == m["kind"] and v["choice"] == v["nameChoice"] == str(m["choice"]), "opened draft identity")
    require(v["nameDisabled"] is False and v["kindDisabled"] is False and v["copy"] is True and v["title"] == "Add field" and v["options"] == KINDS and v["hint"] == "Shared with everyone who has access to this List." and v["focused"] == "slack-list-rename-name" and v["observed"] is None, "opened draft controls")


def opened(v, channel, config):
    fields(v, "model dom input activation initial initialDom selected")
    m = config["initial"]
    serial, visit, selection = (m[k] for k in ("serial", "visit", "selection"))
    initial = v["initial"]
    integer(initial["choice"])
    outer(initial, serial, visit, selection, initial["choice"], channel, name="", kind="text")
    dom_draft(v["initialDom"], initial, channel)
    fields(v["selected"], "event model")
    event = v["selected"]["event"]
    fields(event, "ok value scrolling")
    require(event["ok"] is True and event["value"] == ("channel" if channel else "user"), "selected type")
    scroll(event["scrolling"])
    outer(v["selected"]["model"], serial, visit, selection, m["choice"], channel, name="")
    require(m["choice"] != initial["choice"], "type epoch")
    outer(v["model"], serial, visit, selection, m["choice"], channel)
    dom_draft(v["dom"], v["model"], channel)
    fields(v["input"], "ok value scrolling")
    require(v["input"]["ok"] is True and v["input"]["value"] == v["model"]["name"], "name input")
    scroll(v["input"]["scrolling"])
    activation(v["activation"], "#slack-list-new-text-field")


def inspect(v, part, m, channel, viewport):
    fields(v, "ok part scrolling focusRetained focused rect text visible snapshot viewport" + (" value" if part != "summary" else ""))
    require(v["ok"] is True and v["part"] == part and v["focusRetained"] is True and v["visible"] is True and v["viewport"] == viewport, "inspect capture evidence")
    require(v["focused"] is None or type(v["focused"]) is str, "capture active element")
    scroll(v["scrolling"])
    rectangle(v["rect"], viewport)
    snapshot(v["snapshot"], m, channel)
    if part == "format":
        require(v["text"] == "" and v["value"] == "on", "format input capture")
    elif part == "default":
        require(v["text"] == "" and v["value"] == "", "default input capture")
    else:
        label = "Channel" if channel else "People"
        name = ("Channel" if channel else "Reviewer") + " \u00c9quipe \u6771\u4eac"
        require(v["text"] == f"Observed {label} field: {name}", "observed summary copy")


def geometry(v, i, m, channel, viewport):
    fields(v, "ok phase panel controls observedVisible focus viewport settings")
    require(v["ok"] is True and v["phase"] == PHASES[i] and v["viewport"] == viewport and v["observedVisible"] is True, "stage geometry")
    rectangle(v["panel"], viewport)
    names = ("leave", "close", "confirm") if i == 0 else ("leave",) if i == 1 else ("leave", "close", "check") if i == 3 else ("leave", "close") if i == 5 else ("leave", "check")
    require(type(v["controls"]) is list and len(v["controls"]) == len(names), "stage control count")
    for c, name in zip(v["controls"], names):
        fields(c, "id hit disabled rect")
        require(c["id"] == "slack-list-rename-" + name and c["hit"] is True and c["disabled"] is False, "exact enabled stage controls")
        rectangle(c["rect"], viewport)
    f = v["focus"]
    fields(f, "ok retainedAction actionId activeId id ringFits")
    allowed = (("name",), ("leave",), ("leave", "check"), ("leave", "check"), ("check",), ("close", "leave"))[i]
    require(f["ok"] is True and f["ringFits"] is True and f["id"] == f["activeId"] and f["id"] in ["slack-list-rename-" + x for x in allowed], "exact stage focus")
    require(type(f["retainedAction"]) is bool, "retained action flag")
    expected_action = ("name", "confirm", "confirm", "check", "leave", "check")[i]
    require(f["actionId"] in ["slack-list-rename-" + expected_action, f["id"]], "last action node")

    if f["retainedAction"]:
        require(f["id"] == f["actionId"], "retained exact action focus")
    snapshot(v["settings"], m, channel)


def table(v, visit, selection, viewport, binding):
    fields(v, "ok stage phase actualLayout bounded ordered rowIds owner selectors controls exactButtons numberActions numberReady focused focusedCorrectly draft find findVisible partial partialText rowVisible observedVisible first selectedView selectedLabel returnedAdd panel viewport activations")
    for k in ("ok", "actualLayout", "bounded", "ordered", "selectors", "exactButtons", "numberReady", "focusedCorrectly", "findVisible", "partial", "rowVisible", "observedVisible"):
        require(v[k] is True, "table " + k)
    require(v["stage"] == "table" and v["phase"] is None and v["viewport"] == viewport and v["rowIds"] == ORDER and v["selectedView"] == "V_FILTER" and v["selectedLabel"] == "Shared checklist", "table identity/order")
    require(same(v["owner"], {"visit": str(visit), "selection": str(selection), "serial": None, "view": None}), "table owner")
    require(v["numberActions"] == [] and v["panel"] is None and v["draft"] is None and v["focused"] == "slack-list-new-text-field", "table closed review")
    require(type(v["partialText"]) is str and "Only part" in v["partialText"], "partial disclosure")
    fields(v["find"], "ok sameNode value range")
    require(v["find"]["ok"] is True and v["find"]["sameNode"] is True and v["find"]["value"] == "Launch item 0", "replacement Find binding")
    start, end, direction = v["find"]["range"]
    integer(start, 0); integer(end, 0)
    require(start <= end <= 13 and direction in ("none", "forward", "backward"), "replacement Find range")
    focus(v["returnedAdd"])
    require(type(v["controls"]) is list and len(v["controls"]) == 1, "Add control")
    c = v["controls"][0]
    fields(c, "id disabled hit rect")
    require(c["id"] == "slack-list-new-text-field" and c["disabled"] is False and c["hit"] is True, "Add usable")
    rectangle(c["rect"], viewport)
    first = v["first"]
    fields(first, "id primary created heading primaryText createdText")
    require(first["id"] == "R_LIST_003" and first["primaryText"] == "Launch item 04" and first["createdText"] == "", "created cell is still empty")
    for k in ("primary", "created", "heading"):
        rectangle(first[k], viewport)
    selectors = ("new-text-field", "rename-close", "new-text-field", "rename-confirm", "rename-check", "rename-check", "rename-leave", "rename-check", "rename-close")
    require(type(v["activations"]) is list and len(v["activations"]) == 9, "exact lifecycle activation count")
    for a, selector in zip(v["activations"], selectors):
        activation(a, "#slack-list-" + selector)


def validate(r, channel, mode, viewport):
    fields(r, "ok fixture mode entityCase expectedOptions owner account viewport noSession noTransport httpExecuted actualCreateForms creationReads before after population find firstFind cancel configuration admitted disabledNoReplay retired observed doneFocus ledger cellCompatibility stages captures")
    require(all(type(path) is str and path.endswith(".png") and not any(ord(c) < 32 for c in path) for path in r["captures"].values()), "capture path shape")
    fields(r["owner"], "version nonce pid fixture")
    require(type(r["owner"]["version"]) is int and r["owner"]["version"] == 1, "owner protocol version")
    seed_bytes = Path(__file__).with_name("list_entity_create_seed.json").read_bytes()
    require(hashlib.sha256(seed_bytes).hexdigest() == SEED_SHA256, "immutable literal seed")
    seed = json.loads(seed_bytes)
    require(same(r["before"], seed["before"]) and same(r["population"], seed["population"]), "literal seeded document and raw73 population")
    fields(r["find"], "input applied revision")
    integer(r["find"]["revision"])
    require(r["find"]["input"] == r["find"]["applied"] == "Launch item 0", "settled Find")
    fields(r["firstFind"], "ok value range")
    require(r["firstFind"]["ok"] is True and r["firstFind"]["value"] == "Launch item 0", "initial Find binding")
    a, b, direction = r["firstFind"]["range"]
    integer(a, 0); integer(b, 0)
    require(a <= b <= len("Launch item 0") and direction in ("none", "forward", "backward"), "Find range")
    wire = options(channel, mode)
    serial = r["admitted"]["serial"]
    ledger = expected_ledger(r["before"], r["after"], channel, mode, serial)
    def checkpoint(count, finished, server, write=False, read=False):
        return {"requests": ledger["requests"][:count], "responses": ledger["responses"][:count], "finished": ledger["finished"][:finished], "server": r["after"] if server else r["before"], "writePending": write, "readPending": read}
    empty = checkpoint(0, 0, False)
    fields(r["cancel"], "draft configuration focus ledger")
    configuration(r["cancel"]["configuration"], channel, mode, empty, viewport)
    opened(r["cancel"]["draft"], channel, r["cancel"]["configuration"])
    focus(r["cancel"]["focus"]); focus(r["doneFocus"])
    configuration(r["configuration"], channel, mode, empty, viewport)
    final = r["configuration"]["final"]["model"]
    ret = r["retired"]
    fields(ret, "oldVisit newVisit oldSelection newSelection oldEpoch newEpoch held completed")
    for key in ("oldVisit", "newVisit", "oldSelection", "newSelection"):
        integer(ret[key])
    integer(ret["oldEpoch"], 0); integer(ret["newEpoch"], 0)
    require(final["visit"] == ret["oldVisit"] and final["selection"] == ret["oldSelection"], "initial configuration owner")
    require(same(ret["held"], checkpoint(8, 2, True, read=True)) and same(ret["completed"], checkpoint(8, 3, True)), "held and retired response ledger")
    outer(r["admitted"], serial, ret["oldVisit"], ret["oldSelection"], final["choice"], channel)
    outer(r["observed"], serial, ret["newVisit"], ret["newSelection"], final["choice"], channel, True)
    for index, part in enumerate(("format", "default")):
        stage = r["stages"][index]
        fields(stage, "stage model geometry ledger noSession noTransport")
        require(same(stage["model"], final) and same(stage["ledger"], empty), "configuration capture owner/ledger")
        inspect(stage["geometry"], part, final, channel, viewport)
        target = "slack-list-entity-multiple" if mode == "single" else "slack-list-entity-default-query" if mode == "omitted" else None
        require(stage["geometry"]["focused"] == target, "configuration capture retains final action focus")
    for i, stage in enumerate(r["stages"][2:9]):
        fields(stage, "stage ordinal geometry reveal configuration draft custody ledger document population find noSession noTransport")
        integer(stage["ordinal"], 0)
        require(stage["ordinal"] == i and same(stage["document"], r["after"] if i >= 5 else r["before"]) and same(stage["population"], r["population"]) and same(stage["find"], r["find"]), "stage raw state")
        require(same(stage["ledger"], checkpoint(COUNTS[i], FINISHED[i], i >= 2, write=i == 1)), "stage exact ledger checkpoint")
        visit = ret["oldVisit" if i < 4 else "newVisit"]
        selection = ret["oldSelection" if i < 4 else "newSelection"]
        if i == 6:
            require(stage["draft"] is None and stage["custody"] is None and stage["configuration"] is None, "Done releases active draft")
            table(stage["geometry"], visit, selection, viewport, r["firstFind"])
            reveal = stage["reveal"]
            fields(reveal, "ok restored close add rowScroll focusRetained")
            require(reveal["ok"] is True, "table reveal")
            focus(reveal["restored"]); focus(reveal["focusRetained"])
            fields(reveal["close"], "reachable rect scrolling")
            fields(reveal["add"], "reachable scrolling")
            require(reveal["close"]["reachable"] is True and reveal["add"]["reachable"] is True, "table reveal control reachability")
            rectangle(reveal["close"]["rect"], viewport)
            scroll(reveal["close"]["scrolling"]); scroll(reveal["add"]["scrolling"]); scroll(reveal["rowScroll"])
            continue
        outer(stage["draft"], serial, visit, selection, final["choice"], channel, i == 5)
        m = stage["configuration"]["model"]
        model(m, channel, wire, i == 0, stage["draft"])
        require(m["epoch"] == final["epoch"], "frozen configuration epoch")
        verified(stage["configuration"], m, channel)
        geometry(stage["geometry"], i, m, channel, viewport)
        if i == 5:
            inspect(stage["reveal"], "summary", m, channel, viewport)
            require(stage["reveal"]["focused"] == stage["geometry"]["focus"]["id"], "summary reveal preserves focused node")
        else:
            require(stage["reveal"] is None, "unexpected stage reveal")
        wanted = {"serial": serial, "document": r["before"], "population": r["population"], "intentDocument": r["before"], "intentPopulation": r["population"], "name": stage["draft"]["name"], "view": "V_FILTER", "user": "U0", "kind": r["mode"], "choice": final["choice"], "options": wire}
        require(same(stage["custody"], wanted if i > 0 else None), "immutable intent custody")
    locked = r["stages"][3]["configuration"]["model"]
    fields(r["disabledNoReplay"], "ok before after")
    require(r["disabledNoReplay"]["ok"] is True and same(r["disabledNoReplay"]["before"], r["disabledNoReplay"]["after"]), "disabled controls unchanged")
    snapshot(r["disabledNoReplay"]["before"], locked, channel)
    cell_evidence(r, channel, mode, viewport)


def cell_evidence(r, channel, mode, viewport):
    c = r["cellCompatibility"]
    fields(c, "typed initial local returned reopen final cellAttempts noBackfill ledgerUnchanged")
    multi = mode in ("multiple", "omitted")
    typ = c["typed"]
    require(same(typ, {"singleAdmitted": True, "multipleAdmitted": multi, "createDefaultIds": options(channel, mode).get("default_value_typed"), "rawRowsUnchanged": True, "notExecuted": True}), "typed cell admission")
    for k in ("initial", "reopen"):
        v = c[k]
        fields(v, "ok selected candidateTypes generation focusOutlineFits viewport")
        require(v["ok"] is True and v["selected"] == [] and v["candidateTypes"] == ["checkbox" if multi else "radio"] * 12 and v["focusOutlineFits"] is True and v["viewport"] == viewport, "empty cell picker proof")
        require(type(v["generation"]) is str and v["generation"].isdigit() and int(v["generation"]) > 0, "cell generation")
    v = c["local"]
    fields(v, "ok generation selected save cancel viewport")
    require(v["ok"] is True and v["viewport"] == viewport and v["generation"] == c["initial"]["generation"] and v["selected"] == (["G00000002"] if channel else ["U00000002"]), "local cell choice")
    rectangle(v["save"], viewport); rectangle(v["cancel"], viewport)
    for k in ("returned", "final"):
        fields(c[k], "ok sameCell instance viewport")
        require(c[k]["ok"] is True and c[k]["sameCell"] is True and c[k]["instance"] == '["","R_LIST_003"]' and c[k]["viewport"] == viewport, "same exact source cell return")
    fields(r["stages"][9], "stage geometry initial typed ledger noSession noTransport")
    fields(r["stages"][10], "stage geometry reopen ledger noSession noTransport")
    require(same(r["stages"][9]["geometry"], c["local"]) and same(r["stages"][9]["initial"], c["initial"]) and same(r["stages"][9]["typed"], typ) and same(r["stages"][10]["geometry"], c["final"]) and same(r["stages"][10]["reopen"], c["reopen"]), "cell stage correspondence")
    require(all(same(s["ledger"], r["ledger"]) for s in r["stages"][9:]), "cell stages no dispatch")
