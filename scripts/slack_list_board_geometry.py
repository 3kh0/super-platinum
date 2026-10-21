"""Validate native measured List board/header receipts; never runs the app."""
import math

EXPECTED = (
    ("OPT_ENGINEERING", "Engineering", "2"),
    ("OPT_DESIGN", "Design", "2"),
    ("OPT_OPERATIONS", "Operations", "0"),
    ("", "Ungrouped", "67"),
)
TOLERANCE = 1


def require(condition, message):
    if not condition:
        raise RuntimeError("List header geometry: " + message)


def number(value):
    return type(value) in (int, float) and math.isfinite(value)


def rectangle(value):
    require(isinstance(value, dict), "missing rectangle")
    require(all(number(value.get(k)) for k in
                ("left", "right", "top", "bottom", "width", "height")),
            "nonfinite rectangle")
    require(value["width"] > 0 and value["height"] > 0, "empty rectangle")
    require(abs(value["right"] - value["left"] - value["width"]) <= .01
            and abs(value["bottom"] - value["top"] - value["height"]) <= .01,
            "inconsistent rectangle dimensions")
    for key, edge in (("x", "left"), ("y", "top")):
        if key in value:
            require(number(value[key]) and abs(value[key] - value[edge]) <= .01,
                    "inconsistent rectangle origin")
    return value


def inside(inner, outer):
    return (inner["left"] >= outer["left"] - TOLERANCE
            and inner["right"] <= outer["right"] + TOLERANCE
            and inner["top"] >= outer["top"] - TOLERANCE
            and inner["bottom"] <= outer["bottom"] + TOLERANCE)


def overlap(a, b):
    return (min(a["right"], b["right"]) - max(a["left"], b["left"]) > TOLERANCE
            and min(a["bottom"], b["bottom"]) - max(a["top"], b["top"]) > TOLERANCE)


def flags(value, *names):
    require(isinstance(value, dict), "missing measured proof")
    for key in names:
        require(value.get(key) is True, "missing " + key)


def paint(value, viewport, hit=True):
    flags(value, "inView")
    if hit:
        flags(value, "hit")
    r = rectangle(value.get("rect"))
    require(inside(r, viewport), "control outside viewport")
    return r


def label(value, expected, bounds):
    require(isinstance(value, dict) and value.get("text") == expected, "changed label")
    ranges = value.get("ranges")
    require(isinstance(ranges, list) and bool(ranges), "missing text Range")
    lines = []
    for raw in ranges:
        r = rectangle(raw)
        require(inside(r, bounds), "text clips its control")
        if not any(abs(y - r["top"]) <= TOLERANCE for y in lines):
            lines.append(r["top"])
    require(type(value.get("lines")) is int and value["lines"] == len(lines) == 1,
            "short label is not one measured line")
    return ranges


def _action_identity(action, kind, key, name):
    # Preserve original action data, rather than accepting interchangeable buttons.
    require(type(action.get("disabled")) is bool, "missing disabled state")
    require(action.get("add") is (kind == "add"), "changed Add identity")
    expected = {"choice": None, "value": None, "view": None, "delete": None,
                "move": None, "laneKey": None}
    if kind in ("name", "color"):
        expected.update(choice=kind, value=key, view="V_BOARD_MULTI")
    elif kind == "delete":
        expected.update(delete=key, view="V_BOARD_MULTI")
    else:
        expected.update(laneKey=key)
        if kind != "add":
            expected["move"] = kind
    require(all(k in action and action[k] == v for k, v in expected.items()),
            "changed action source identity")
    if kind == "add":
        require(action.get("ariaLabel") == "Add item to " + name, "lost Add label")
    elif kind in ("left", "right"):
        require(action.get("ariaLabel") == "Move lane " + kind, "lost move label")


def board(value, viewport):
    flags(value, "ok", "restored")
    add = value.get("addGroup")
    flags(add, "ok")
    require(add.get("disabled") is False and add.get("view") == "V_BOARD_MULTI"
            and add.get("value") == "", "changed Add group source")
    height = add.get("controlHeight")
    require(number(height) and abs(height - 32) <= .01, "changed control-height token")
    bounds = paint(add, viewport)
    ranges = label(add.get("label"), "Add group", bounds)
    require(len(ranges) == 1, "fragmented Add group label")
    icon = rectangle(add.get("icon"))
    text = ranges[0]
    require(inside(icon, bounds) and icon["right"] <= text["left"] + TOLERANCE
            and min(icon["bottom"], text["bottom"]) > max(icon["top"], text["top"]),
            "Add group icon and text do not share a line")
    sizing = add.get("box", {})
    keys = ("gap", "paddingLeft", "paddingRight", "borderLeft", "borderRight")
    require(all(number(sizing.get(k)) and sizing[k] >= 0 for k in keys),
            "missing Add group sizing operands")
    intrinsic = icon["width"] + text["width"] + sum(sizing[k] for k in keys)
    require(number(add.get("intrinsic")) and abs(add["intrinsic"] - intrinsic) <= .01,
            "forged Add group intrinsic width")
    require(bounds["width"] <= intrinsic + 2
            and abs(bounds["height"] - height) <= TOLERANCE,
            "oversized Add group")
    lanes = value.get("lanes")
    require(isinstance(lanes, list) and len(lanes) == len(EXPECTED), "lost original lanes")
    for row, (key, name, count) in zip(lanes, EXPECTED):
        flags(row, "ok", "identities", "nonoverlap", "actionBounds")
        require(row.get("key") == key, "lost original lane identity/order")
        lane = rectangle(row.get("lane"))
        header = paint(row.get("header"), viewport, hit=False)
        heading = paint(row.get("heading"), viewport)
        counter = paint(row.get("counter"), viewport, hit=False)
        require(inside(header, lane) and inside(heading, header) and inside(counter, header)
                and not overlap(heading, counter), "heading/count outside original lane")
        label(row.get("label"), name, heading)
        label(row.get("count"), count, counter)
        actions = row.get("actions")
        kinds = (["name", "color", "delete"] if key else []) + ["left", "right", "add"]
        names = (["Name", "Color", "Delete"] if key else []) + ["Left", "Right", ""]
        require(isinstance(actions, list) and len(actions) == len(kinds),
                "missing/extra lane actions")
        boxes = []
        for action, kind, text in zip(actions, kinds, names):
            _action_identity(action, kind, key, name)
            flags(action, "focusStable")
            r = paint(action, viewport, hit=not action["disabled"])
            require(action.get("text") == text, "changed action label/order")
            if text:
                label(action.get("label"), text, r)
            else:
                require(action.get("label") == {"text": "", "ranges": [], "lines": 0},
                        "changed icon-only action")
            require(inside(r, header) and inside(r, lane)
                    and abs(r["height"] - height) <= TOLERANCE
                    and not overlap(r, heading) and not overlap(r, counter),
                    "action outside lane/header or overlaps identity")
            require(not any(overlap(r, other) for other in boxes), "overlapping actions")
            if kind == "add":
                require(abs(r["width"] - height) <= TOLERANCE, "oversized lane Add")
            boxes.append(r)
        cards = rectangle(row.get("cards"))
        require(cards["top"] >= header["bottom"] - TOLERANCE,
                "header overlaps card pane")


def header(value, viewport):
    flags(value, "restored")
    for kind, identity, text in (("refresh", "slack-list-refresh", "Refresh"),
                                 ("close", "slack-list-close", "")):
        proof = value.get(kind + "Reachable")
        flags(proof, "ok", "contained", "bounded", "noOverlap", "hit", "focused")
        require(proof.get("id") == identity and proof.get("text") == text,
                "wrong original header control")
        if kind == "close":
            require(proof.get("ariaLabel") == "Close list", "lost Close accessible label")
        r = rectangle(proof.get("rect"))
        parent = rectangle(proof.get("header"))
        other = rectangle(proof.get("otherRect"))
        require(inside(r, parent) and inside(r, viewport) and r["height"] >= 24
                and not overlap(r, other), "unreachable/overlapping header control")
        if kind == "refresh":
            require(r["width"] >= 50, "crushed Refresh")
        scroll = proof.get("scroll")
        require(isinstance(scroll, list) and len(scroll) == 2
                and all(number(v) and v >= 0 for v in scroll), "invalid header scroll")


def validate_geometry(receipt, expected_viewport):
    """Every geometric assertion uses original native operands, not just ok flags."""
    require(receipt.get("layout", {}).get("viewport") == expected_viewport,
            "wrong native viewport")
    width, height = expected_viewport
    viewport = {"left": 0, "top": 0, "right": width, "bottom": height}
    layout = receipt["layout"]
    flags(layout, "ok", "bounded", "headerNoOverlap")
    header(layout.get("header"), viewport)
    board(receipt.get("boardViews", {}).get("headerGeometry"), viewport)
