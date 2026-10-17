"""Opt-in reader barrier, released only inside the normalized fixture context."""
import json
import os
from pathlib import Path
import secrets
import time

from canvas_evidence import closed_receipts

NONCE = "SUPER_PLATINUM_CANVAS_READER_NONCE"
PREFIX = "canvas-comments-waiting: "


def prepare(spec, fixture, directory, checks):
    if not spec.readiness:
        return None
    if (spec.fixture != "canvas-comments" or fixture not in ("canvas-comments", "canvas-comments-narrow")
            or spec.flag != "SUPER_PLATINUM_CANVAS_COMMENTS_CHECK" or NONCE in checks):
        raise RuntimeError("Unsupported readiness owner")
    owner = dict(version=1, nonce=secrets.token_hex(16), fixture=fixture)
    for name in ("reader-ready.json", "reader-ready.pending", "reader-owner.json"):
        if (directory / name).exists():
            raise RuntimeError("Fresh readiness files required")
    with (directory / "reader-owner.json").open("x", encoding="utf-8") as output:
        output.write(json.dumps(owner) + "\n")
    checks[NONCE] = owner["nonce"]
    return owner


def validate_waiting(receipts, expected):
    if (type(expected.get("pid")) is not int or expected["pid"] <= 0
            or len(receipts) != 1 or receipts[0] != expected
            or type(receipts[0].get("pid")) is not int
            or type(receipts[0].get("version")) is not int):
        raise RuntimeError("Reader readiness tuple differs or duplicated")


def publish(directory, expected):
    pending = directory / "reader-ready.pending"
    with pending.open("x", encoding="utf-8", newline="\n") as output:
        output.write(json.dumps(expected) + "\n")
    os.link(pending, directory / "reader-ready.json")
    pending.unlink()


def release(native, spec, directory, owner):
    """Caller must already hold audited_fixture's normalized context."""
    expected = dict(owner, pid=native.process.pid)
    deadline = time.monotonic() + 25
    while True:
        native.require_running()
        if native.receipts(spec.fixture + "-stage: ") or native.receipts(spec.fixture + "-check: "):
            raise RuntimeError("Reader acted or failed before normalization readiness")
        waiting = native.receipts(PREFIX)
        if waiting:
            validate_waiting(waiting, expected)
            publish(directory, expected)
            return expected
        if time.monotonic() >= deadline:
            raise TimeoutError("Reader readiness request timed out")
        time.sleep(.05)


def validate_closed(log, spec, expected):
    validate_waiting(closed_receipts(log, PREFIX), expected)
    prefixes = (PREFIX, spec.fixture + "-stage: ", spec.fixture + "-check: ")
    records = [line for line in Path(log).read_text(encoding="utf-8", errors="replace").splitlines()
               if any(line.startswith(p) for p in prefixes)]
    if not records or not records[0].startswith(PREFIX):
        raise RuntimeError("Reader actions preceded readiness")
