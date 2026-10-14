#!/usr/bin/env python3
"""Verify native media decode, pending placeholders and generation refresh."""
import argparse
from pathlib import Path
import time

from native_fixture import DEFAULT_BINARY, ROOT, NativeFixture


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=DEFAULT_BINARY)
    parser.add_argument("--captures", type=Path, default=ROOT / "tmp/agent-ui/media")
    args = parser.parse_args()
    captures = args.captures.resolve()
    with NativeFixture("channel", captures / "media-protocol.log", args.binary,
                       {"SUPER_PLATINUM_MEDIA_CHECK": "1"}) as native:
        deadline = time.monotonic() + 30
        while True:
            native.require_running()
            receipts = native.receipts("media-protocol-check: ")
            if receipts:
                receipt = receipts[-1]
                if (receipt.get("ok") is not True
                        or receipt.get("generationRefresh") is not True
                        or len(receipt.get("probes", [])) != 8
                        or len(receipt.get("refreshProbes", [])) != 4
                        or receipt.get("brokenDomImages") != 0):
                    native.capture(captures / "media-protocol-failure.png")
                    raise RuntimeError(f"Native media assertions failed: {receipt}")
                native.focus()
                native.capture(captures / "media-protocol.png")
                print(f"Native media decode/placeholder/refresh checks passed: {receipt}")
                return
            if time.monotonic() >= deadline:
                raise RuntimeError(f"Native media assertions timed out; see {native.log}")
            time.sleep(.1)


if __name__ == "__main__":
    main()
