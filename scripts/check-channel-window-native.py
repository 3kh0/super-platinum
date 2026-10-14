#!/usr/bin/env python3
"""Check bounded channel rendering, targets and selection in an isolated native process."""
from native_check import expected_viewport
from native_window_check import arguments, run

FIXTURES = ("channel-window-10000",)


def validate(fixture, receipt):
    if receipt.get("ok") is not True or receipt.get("selection") is not True or receipt.get("unread") is not True:
        raise RuntimeError("Channel window/selection/unread proof is incomplete")
    native = receipt.get("native", {})
    if (native.get("metadata") != 10001 or native.get("selectionPinned") is not False
            or not 0 < native.get("projected", 0) <= 100
            or native.get("end", 0) - native.get("start", 0) != native.get("projected")):
        raise RuntimeError("Channel native projection was not bounded or selection remained pinned")
    agreement = receipt.get("finalAgreement", {})
    if any(agreement.get(key) is not True for key in ("ok", "matches", "targetPresent", "targetVisible")) or agreement.get("pending") is not False:
        raise RuntimeError("Channel DOM/native target agreement is incomplete")
    jumps = receipt.get("jumps", [])
    if [jump.get("fraction") for jump in jumps] != [0, .25, .5, .75, 1] or any(not 0 < jump.get("mounted", 0) <= 100 for jump in jumps):
        raise RuntimeError("Channel five-jump bounded rendering proof is incomplete")
    if [sample.get("phase") for sample in receipt.get("settled", [])] != ["date", "unread"]:
        raise RuntimeError("Channel date/unread settling receipts missing")
    viewport = receipt.get("viewport", {})
    if [viewport.get("width"), viewport.get("height")] != expected_viewport(fixture):
        raise RuntimeError(f"Unexpected channel viewport: {viewport}")


if __name__ == "__main__":
    args = arguments(__doc__, FIXTURES)
    for fixture in args.fixtures:
        run(fixture, args, "SUPER_PLATINUM_CHANNEL_WINDOW_CHECK", "channel window check", validate)
