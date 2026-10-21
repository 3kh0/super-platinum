#!/usr/bin/env python3
"""Read Canvas threads through actual controls using disconnected exact read responses."""
import sys
sys.dont_write_bytecode = True
import math
from urllib.parse import urlsplit
import canvas_native
import canvas_comment_root_bodies
from canvas_native import CanvasCheck, main

SPEC = CanvasCheck(
    fixture="canvas-comments", flag="SUPER_PLATINUM_CANVAS_COMMENTS_CHECK",
    readiness=True,
    required=("disconnected", "readOnly", "exactRequests", "filterIsolation", "canonicalThread",
              "pagination", "errorNotEmpty", "explicitRetry", "returnedToCanvas", "focusContinuity", "syntheticInput"),
    counts={"requestCount":11},
    stages=("initial", "thread", "thread-more", "previous-page", "resolved", "unread", "error", "recovered", "stream-more"),
)
_base_validate = canvas_native.validate

def validate(spec, fixture, receipt):
    _base_validate(spec, fixture, receipt)
    requests = receipt.get("requests")
    expected = [("history", "latest_reply", None), ("history", "latest_reply", None), ("replies", None, None),
                ("replies", None, "reply-next"), ("replies", None, None), ("history", "latest_reply", None),
                ("history", "archived", None), ("history", "unread", None),
                ("history", "latest_reply", None), ("history", "latest_reply", None),
                ("history", "latest_reply", "stream-next")]
    if not isinstance(requests, list) or len(requests) != len(expected):
        raise RuntimeError("Missing exact comment request ledger")
    for request, (method, filter_value, cursor) in zip(requests, expected):
        url = urlsplit(request.get("url", ""))
        if (url.scheme != "https" or url.netloc != "fixture.slack.com"
                or url.path != "/api/conversations." + method or url.fragment
                or request.get("method") != "POST"):
            raise RuntimeError("Wrong comment request endpoint")
        pairs = request.get("fields")
        if (not isinstance(pairs, list) or any(not isinstance(p, list) or len(p) != 2
                or not all(isinstance(x, str) for x in p) for p in pairs)):
            raise RuntimeError("Malformed comment form")
        fields = dict(pairs)
        if len(fields) != len(pairs):
            raise RuntimeError("Duplicate comment form keys")
        wanted = {"channel":"Cfixture", "inclusive":"true", "limit":"20" if method == "history" else "28"}
        if cursor is not None:
            wanted["cursor"] = cursor
        if method == "history":
            wanted.update(document_stream=filter_value, ignore_replies="true", include_pin_count="true",
                          no_user_profile="true", include_stories="true", include_free_team_extra_messages="true",
                          include_date_joined="false", cached_latest_updates="{}")
        else:
            wanted["ts"] = "1719800000.000100"
            wanted["cached_latest_updates"] = "{}"
        if fields != wanted:
            raise RuntimeError("Comment form differs from exact read contract")
    layouts = receipt.get("layouts")
    viewport = receipt["layout"]["viewport"]
    if not isinstance(layouts, list) or len(layouts) != len(SPEC.stages):
        raise RuntimeError("Missing stage geometry")
    def bounded(rect):
        if not isinstance(rect, dict) or any(type(rect.get(k)) not in (int, float)
                or not math.isfinite(rect[k]) for k in ("x", "y", "width", "height", "right", "bottom")):
            return False
        return (rect["width"] > 0 and rect["height"] > 0 and rect["x"] >= -1 and rect["y"] >= -1
                and abs(rect["right"] - rect["x"] - rect["width"]) < .01
                and abs(rect["bottom"] - rect["y"] - rect["height"]) < .01
                and rect["right"] <= viewport[0]+1 and rect["bottom"] <= viewport[1]+1)
    for layout in layouts:
        controls = layout.get("controls")
        if (layout.get("viewport") != viewport or layout.get("ok") is not True
                or layout.get("focused") is not True or layout.get("noOverflow") is not True or not bounded(layout.get("dialog"))
                or not bounded(layout.get("panel")) or not isinstance(controls, list) or len(controls) != 3
                or any(c.get("hit") is not True or not bounded(c.get("rect")) for c in controls)
                or controls[0].get("label") != "Back to canvas"
                or controls[-1].get("label") != "Refresh comments"):
            raise RuntimeError("Incomplete or clipped comment controls")
    canvas_comment_root_bodies.validate(receipt)


_base_validate_closed = canvas_native.validate_closed


def validate_closed(spec, fixture, log, stages, terminal):
    _base_validate_closed(spec, fixture, log, stages, terminal)
    if [stage.get("layout") for stage in stages] != terminal.get("layouts"):
        raise RuntimeError("Captured comment body/geometry differs from terminal proof")

if __name__ == "__main__":
    canvas_native.validate = validate
    canvas_native.validate_closed = validate_closed
    main(SPEC, __doc__)
