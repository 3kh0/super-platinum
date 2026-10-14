#!/usr/bin/env python3
"""Check synthetic scheduling acknowledgments preserve native input; never sends."""
import argparse
import os
from pathlib import Path
from native_check import CheckSpec, run_fixture
from native_fixture import DEFAULT_BINARY, ROOT

SPEC = CheckSpec(
    fixture="schedule-clear",
    flag="SUPER_PLATINUM_SCHEDULE_CLEAR_CHECK",
    prefix="schedule-clear-check: ",
    required=(
        "newerNativeInputPreserved", "syntheticCompositionPreserved",
        "reopenedThreadPreserved", "reopenedChannelPreserved", "matchingSourceCleared", "bothComposersChecked",
        "navigationRetainedNewerDraft",
        "noTransport", "noSession", "noSlackMutation", "eachVisible", "hitTested",
    ),
    prepared_requests=0,
)

if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path(os.environ.get("SUPER_PLATINUM_BINARY", DEFAULT_BINARY)))
    parser.add_argument("--captures", type=Path, default=ROOT / "tmp/agent-ui")
    fixtures = ("schedule-clear", "schedule-clear-narrow", "schedule-clear-thread")
    parser.add_argument("--fixture", action="append", choices=fixtures)
    args = parser.parse_args()
    for fixture in args.fixture or fixtures:
        run_fixture(SPEC, fixture, args.binary, args.captures.resolve())
