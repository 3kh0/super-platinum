#!/usr/bin/env python3
"""Resolve/re-open Canvas threads once, retain unknown delivery, and explicitly review its local lock."""
import sys
sys.dont_write_bytecode = True
import math
from urllib.parse import urlsplit
import canvas_native
from canvas_native import CanvasCheck, main

SPEC = CanvasCheck(
    fixture="canvas-comment-resolve", flag="SUPER_PLATINUM_CANVAS_COMMENT_RESOLVE_CHECK",
    required=("noSession", "noTransport", "syntheticInput", "exactRequests", "resolved", "reopened",
              "knownNotSent", "stalePreflightRefused", "uncertainNoReplay", "durableUncertain",
              "reopenedLocked", "focusContinuity", "explicitReview", "reviewNoReplay"),
    counts={"requestCount":26, "preparedCount":4, "retainedReviewCount":1},
    stages=("initial", "resolved", "reopened", "not-sent", "stale", "uncertain", "reopened-uncertain", "review", "reviewed"),
    private_files=(("SUPER_PLATINUM_CANVAS_COMMENT_DB", "comments.sqlite"),),
)
_base_validate = canvas_native.validate
TS="1719800000.000100"
EXPECTED=("history", "replies", "info", "replies", "archive", "info", "replies", "replies",
          "info", "replies", "unarchive", "info", "replies", "replies", "info", "replies",
          "replies", "info", "replies", "replies", "info", "replies", "unarchive",
          "history", "replies", "replies")

def expected_form(method):
    if method == "info":
        return {"file":"Ffixture"}
    if method in ("archive", "unarchive"):
        return {"channel_id":"Cfixture", "thread_ts":TS}
    wanted={"channel":"Cfixture", "inclusive":"true", "limit":"20" if method=="history" else "28", "cached_latest_updates":"{}"}
    if method=="history":
        wanted.update(document_stream="latest_reply", ignore_replies="true", include_pin_count="true",
                      no_user_profile="true", include_stories="true", include_free_team_extra_messages="true", include_date_joined="false")
    else:
        wanted["ts"]=TS
    return wanted

def check_request(request, method):
    url=urlsplit(request.get("url", ""))
    prefix="files." if method=="info" else "quip.thread." if method in ("archive", "unarchive") else "conversations."
    if (url.scheme!="https" or url.netloc!="fixture.slack.com" or url.path!="/api/"+prefix+method
            or url.fragment or request.get("method")!="POST"):
        raise RuntimeError("Unexpected Canvas comment endpoint")
    pairs=request.get("fields")
    if not isinstance(pairs,list) or any(not isinstance(p,list) or len(p)!=2 or not all(isinstance(x,str) for x in p) for p in pairs):
        raise RuntimeError("Malformed comment request fields")
    if len(dict(pairs))!=len(pairs) or dict(pairs)!=expected_form(method):
        raise RuntimeError("Unexpected Canvas comment request payload")

def validate(spec, fixture, receipt):
    _base_validate(spec, fixture, receipt)
    requests=receipt.get("requests")
    prepared=receipt.get("prepared")
    if not isinstance(requests,list) or len(requests)!=len(EXPECTED) or not isinstance(prepared,list) or len(prepared)!=4:
        raise RuntimeError("Missing actual request ledger")
    for request, method in zip(requests,EXPECTED):
        check_request(request,method)
    for request, method in zip(prepared,("archive","unarchive","archive","unarchive")):
        check_request(request,method)
    import uuid
    try:
        if str(uuid.UUID(receipt["retainedOperation"]))!=receipt["retainedOperation"]:
            raise ValueError()
    except (KeyError,ValueError,TypeError,AttributeError):
        raise RuntimeError("Missing exact retained operation identity")
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
    for ordinal, layout in enumerate(layouts):
        controls = layout.get("controls")
        if (layout.get("viewport") != viewport or layout.get("ok") is not True
                or layout.get("focused") is not True or layout.get("noOverflow") is not True or not bounded(layout.get("dialog"))
                or not bounded(layout.get("panel")) or not isinstance(controls, list) or len(controls) != (5 if ordinal == 7 else 3)
                or any(c.get("hit") is not True or not bounded(c.get("rect")) for c in controls)
                or controls[0].get("label") != "Back to canvas"
                or controls[2].get("label") != "Refresh comments"
                or ordinal == 7 and [c.get("label") for c in controls[3:]] != ["Cancel review", "Remove local lock"]):
            raise RuntimeError("Incomplete or clipped comment controls")


if __name__ == "__main__":
    canvas_native.validate = validate
    main(SPEC, __doc__)
