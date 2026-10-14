#!/usr/bin/env python3
"""Run the existing thread-window, exact-target and page-gap native assertions."""
from native_window_check import arguments, run

FIXTURES = ("thread-window-200", "thread-window-2000", "thread-window-10000",
            "thread-window-target", "thread-window-gap")
# boot.rs uses fixture_bag(count), which includes the root at index zero.
# The gap fixture's ThreadHistory::visible admits root + indices 250..449
# (200 historical rows) + indices 9900..10000 (101 live/pending rows).
TOTALS = dict(zip(FIXTURES, (201, 2001, 10001, 10001, 302)))


def validate(fixture, receipt):
    total = TOTALS[fixture]
    if receipt.get("ok") is not True or type(receipt.get("total")) is not int or receipt["total"] != total or not 0 < receipt.get("initial", 0) <= 102:
        raise RuntimeError("Thread bounded initial rendering proof is incomplete")
    jumps = receipt.get("jumps", [])
    if [jump.get("fraction") for jump in jumps] != [0, .25, .5, .75, 1] or any(not 0 < jump.get("rows", 0) <= 102 for jump in jumps):
        raise RuntimeError("Thread five-jump bounded rendering proof is incomplete")
    root = "1719800400.000000"
    count = 200 if fixture == "thread-window-200" else 2000 if fixture == "thread-window-2000" else 10000
    pending = f"{1719800400 + count}.000000"
    if jumps[0].get("bounds", {}).get("first") != root:
        raise RuntimeError("Thread top jump did not reach the root")
    latest = jumps[-1].get("bounds", {})
    if latest.get("atBottom") is not True or latest.get("last") != pending:
        raise RuntimeError("Thread latest pending row was not retained at the bottom")
    # Exact initial target and gap-before-pending are asserted inside the
    # unchanged native hook; retain its complete receipt instead of inferring
    # those initial conditions from the final screenshot (which shows latest).
    initial = receipt.get("initialBounds", {})
    if fixture not in ("thread-window-target", "thread-window-gap") and initial.get("atBottom") is not True:
        raise RuntimeError("Thread initial latest position missing")
    state = receipt.get("native_state", {})
    start, end = state.get("start"), state.get("end")
    if type(start) is not int or type(end) is not int or not 0 <= start < end <= total or end - start > 100:
        raise RuntimeError("Thread native window bounds are invalid")
    # ThreadWindow::rebuild_selection mounts [start,end), plus the root and
    # pending last row if outside that range, deduplicating both forced rows.
    mounted = end - start + int(start > 0) + int(end < total)
    if jumps[-1]["rows"] != mounted:
        raise RuntimeError("Thread final DOM count disagrees with its native range and retained rows")


if __name__ == "__main__":
    args = arguments(__doc__, FIXTURES)
    for fixture in args.fixtures:
        run(fixture, args, "SUPER_PLATINUM_THREAD_WINDOW_CHECK", "thread window check", validate)
