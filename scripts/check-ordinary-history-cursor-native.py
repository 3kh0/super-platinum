#!/usr/bin/env python3
"""Offline mounted ordinary history cursor, using unchanged audited native capture guards.

No build or live Slack. The fixture owns synthetic metadata/history responses;
this runner verifies exact held state and closed ledgers before accepting them.
"""
import argparse
from copy import deepcopy
import json
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tmp/ordinary-history-cursor-20261024/scripts"))
from history_oracle import Oracle, PREFIX, FLAG, FIXTURES, STAGES, REQUIRED, require
from canvas_native import CanvasCheck, run
from canvas_evidence import closed_receipts
from native_fixture import DEFAULT_BINARY


def spec(fixture):
    require(fixture in FIXTURES, "Unknown exact ordinary history cursor fixture")
    return CanvasCheck(fixture=PREFIX, flag=FLAG, prefix=PREFIX,
                       required=REQUIRED, counts={}, stages=STAGES)


class Control:
    def __init__(self, fixture):
        spec(fixture)
        self.fixture = fixture
        self.oracle = Oracle(fixture)
        self.native = None
        self.stages = []
        self.terminal = None
        self.evidence = {"scope": "offline mounted ordinary history cursor", "observations": []}

    def prepare(self, directory, fixture, checks):
        require(fixture == self.fixture, "Controller fixture changed")
        require(checks.get(FLAG) == "1", "Missing exact fixture authorization")
        return self

    def __enter__(self): return self
    def __exit__(self, *_): return False

    def bind(self, native):
        require(self.native is None, "Controller already bound")
        self.native = native

    def service(self, native):
        require(native is self.native, "Foreign owned process")

    def before_capture(self, native, stage):
        self.service(native)
        self.oracle.observe(stage)
        self.stages.append(deepcopy(stage))
        self.evidence["observations"].append({"stage": stage["stage"], "ordinal": stage["ordinal"]})

    def finish(self, native):
        self.service(native)
        receipts = native.receipts(PREFIX + "-check: ")
        require(len(receipts) == 1, "Missing or duplicate terminal receipt")
        self.oracle.finish(receipts[0])
        self.terminal = deepcopy(receipts[0])
        self.evidence["terminalValidated"] = True

    def validate_closed(self, native):
        self.service(native)
        stages = closed_receipts(native.log, PREFIX + "-stage: ")
        terminal = closed_receipts(native.log, PREFIX + "-check: ")
        require(stages == self.stages and terminal == [self.terminal],
                "Closed navigation ledger changed after capture")
        replay = Oracle(self.fixture)
        for stage in stages: replay.observe(stage)
        replay.finish(terminal[0])
        self.evidence["closedReplayValidated"] = True


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=DEFAULT_BINARY)
    parser.add_argument("--captures", type=Path, required=True)
    parser.add_argument("--fixture", action="append", choices=FIXTURES)
    args = parser.parse_args(argv)
    fixtures = args.fixture or list(FIXTURES)
    require(len(fixtures) == len(set(fixtures)), "Duplicate fixture execution requested")
    for fixture in fixtures:
        run(spec(fixture), fixture, args.binary.resolve(), args.captures.resolve() / fixture,
            storage_factory=Control(fixture))


if __name__ == "__main__": main()
