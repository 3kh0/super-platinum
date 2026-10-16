"""Pure independent geometry and ordered-input contract for toast evidence."""
import json
import math

LABELS = ("Save to Later", "Copy link", "Open in Slack", "Rename…", "Delete…")
STAGES = ("toast-natural", "toast-delete-focus", "toast-dismissed")
PREFIXES = {"message-file-actions-stage: ": "stage", "message-file-toast-input: ": "input",
            "message-file-toast-check: ": "final"}


def require(value, message):
    if not value:
        raise RuntimeError(message)


def rectangle(value):
    require(isinstance(value, list) and len(value) == 4 and all(
        type(n) in (int, float) and math.isfinite(n) for n in value), "Invalid rectangle")
    require(value[2] > value[0] and value[3] > value[1], "Empty rectangle")
    return value


def inside(a, b):
    a, b = rectangle(a), rectangle(b)
    return a[0] >= b[0] and a[1] >= b[1] and a[2] <= b[2] and a[3] <= b[3]


def hits(samples, bounds):
    require(isinstance(samples, list) and len(samples) == 5, "Five hit samples required")
    l, t, r, b = rectangle(bounds)
    d = min(6, (r-l)/4, (b-t)/4)
    points = [[(l+r)/2, (t+b)/2], [l+d, t+d], [r-d, t+d], [l+d, b-d], [r-d, b-d]]
    for sample, point in zip(samples, points):
        require(sample.get("hit") is True and sample.get("point") == point,
                "Center/corner hit failed or point differs")


def validate_layout(layout, viewport, visible, tabs):
    require(layout.get("ok") is True and layout.get("viewport") == viewport,
            "Layout/viewport differs")
    require(layout.get("toastExpected") is visible and layout.get("noActivation") is True,
            "Toast mode/activation differs")
    require(type(layout.get("tabCount")) is int and layout["tabCount"] == tabs, "Tab count differs")
    events = layout.get("events")
    require(isinstance(events, list) and len(events) == tabs, "Trusted event count differs")
    for event in events:
        require(all(type(event.get(k)) is bool for k in ("trusted", "repeat", "shift", "ctrl", "alt", "meta"))
                and event == dict(key="Tab", trusted=True, repeat=False, shift=False,
                              ctrl=False, alt=False, meta=False), "Non-Tab/untrusted event")
    screen = [0, 0, *viewport]
    panel = rectangle(layout.get("panel"))
    require(inside(panel, screen) and inside(panel, layout.get("clip")), "Panel clipped")
    controls = layout.get("controls")
    require(isinstance(controls, list) and [c.get("label") for c in controls] == list(LABELS),
            "Menu identity differs")
    focused = []
    for control in controls:
        require(control.get("visible") is True and control.get("enabled") is True
                and inside(control.get("bounds"), panel), "Control invisible/clipped/disabled")
        hits(control.get("hits"), control["bounds"])
        if control.get("focused") is True:
            focused.append(control["label"])
    require(len(focused) <= 1 and layout.get("focusedLabel") == (focused[0] if focused else None),
            "Focus identity differs")
    toast = layout.get("toast")
    if visible:
        require(isinstance(toast, dict) and toast.get("visible") is True
                and toast.get("positioned") is True and toast.get("text") == "File link copied."
                and toast.get("gap") == 8 and toast.get("panelClear") is True, "Visible toast required")
        bounds = rectangle(toast.get("bounds"))
        require(inside(bounds, screen) and inside(toast.get("textBounds"), bounds)
                and inside(toast.get("dismissBounds"), bounds), "Toast text/control clipped")
        require(bounds[2]+8 <= panel[0] or panel[2]+8 <= bounds[0]
                or bounds[3]+8 <= panel[1] or panel[3]+8 <= bounds[1], "Toast lacks 8px gap")
        hits(toast.get("textHits"), toast["textBounds"])
        hits(toast.get("dismissHits"), toast["dismissBounds"])
    else:
        require(toast is None, "Dismissed toast still present")


def read_events(log, *, closed=False):
    events = []
    for line in log.read_text(encoding="utf-8", errors="strict").splitlines(keepends=True):
        if not line.endswith("\n"):
            require(not closed or not any(prefix in line for prefix in PREFIXES),
                    "Incomplete closed protocol record")
            continue
        for prefix, kind in PREFIXES.items():
            if prefix in line:
                events.append((kind, json.loads(line.split(prefix, 1)[1])))
    return events


class Protocol:
    def __init__(self, viewport):
        self.viewport, self.stages, self.tabs, self.start, self.done = viewport, 0, 0, None, False

    def accept(self, kind, value):
        require(not self.done and all(value.get(k) is True for k in ("ok", "noSession", "noTransport")),
                "Failed/late/nonisolated receipt")
        layout = value.get("layout", {})
        if kind == "stage":
            require(self.stages < 3 and type(value.get("ordinal")) is int
                    and value["ordinal"] == self.stages and value.get("stage") == STAGES[self.stages],
                    "Stage order differs")
            validate_layout(layout, self.viewport, self.stages < 2, self.tabs)
            if self.stages == 0:
                require(self.tabs == 0 and layout["focusedLabel"] in LABELS[:2], "Natural focus differs")
                self.start = LABELS.index(layout["focusedLabel"])
            elif self.stages == 1:
                require(self.tabs == 4-self.start and layout["focusedLabel"] == LABELS[4],
                        "Delete not reached by exact Tabs")
            self.stages += 1
        elif kind == "input":
            require(self.stages == 1 and self.tabs < 4-self.start and type(value.get("index")) is int
                    and value["index"] == self.tabs, "Input order/count differs")
            validate_layout(layout, self.viewport, True, self.tabs)
            require(layout["focusedLabel"] == LABELS[self.start+self.tabs], "Pre-Tab focus differs")
            self.tabs += 1
        elif kind == "final":
            require(self.stages == 3 and all(type(value.get(k)) is int for k in ("stages", "tabs", "toastPublications"))
                    and value.get("stages") == 3 and value.get("tabs") == self.tabs
                    and value.get("toastPublications") == 1 and all(value.get(k) is True for k in
                    ("noRequests", "noActivation", "trustedTabs")), "Incomplete final")
            validate_layout(layout, self.viewport, False, self.tabs)
            self.done = True
        else:
            raise RuntimeError("Unknown protocol event")
