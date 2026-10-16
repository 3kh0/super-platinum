#!/usr/bin/env python3
"""Owned disconnected download captures; never builds or opens an OS Save dialog."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import secrets
import sys
import time

from canvas_evidence import audited_fixture
from native_fixture import NativeFixture, DEFAULT_BINARY
from original_download_native import (FIXTURES, STAGES, LocalOriginal, require,
                                      validate_stage, validate_terminal, validate_files, validate_closed_files)

WAITING = "original-download-waiting: "
STAGE = "original-download-stage: "
PREFIX = "original-download-check: "


def records(path, prefix, final=False):
    with path.open("rb") as stream:
        data = stream.read(4 * 1024 * 1024 + 1)
    require(len(data) <= 4 * 1024 * 1024, "Bounded native log")
    rows = []
    for line in data.decode("utf-8", errors="strict").splitlines(keepends=True):
        if prefix not in line:
            continue
        if not line.endswith("\n"):
            require(not final, "Incomplete closed native record")
            continue
        rows.append(json.loads(line.split(prefix, 1)[1]))
    return rows


def acknowledge(directory, owner, stage):
    data = dict(owner, stage=stage)
    staged = directory / f"ack-{stage}.staged"
    target = directory / f"ack-{stage}.json"
    with staged.open("x", encoding="utf-8") as stream:
        json.dump(data, stream)
        stream.flush()
        os.fsync(stream.fileno())
    try:
        os.link(staged, target)
    finally:
        staged.unlink()


def run(fixture, binary, captures):
    require(fixture in FIXTURES, "Exact fixture alias")
    directory = (captures / fixture).resolve()
    directory.mkdir(parents=True, exist_ok=False)
    (directory / "downloads").mkdir()
    nonce = secrets.token_hex(16)
    evidence = {"ok": False, "fixture": fixture, "nonce": nonce,
                "limitations": ["Synthetic loopback inputs/admission and real handlers; no production authentication proof.",
                                "OS Save dialog substituted; no overwrite-consent or trusted-input proof.",
                                "Sampled geometry/pointer/ownership checks, not continuous occlusion proof."]}
    server = native = None
    primary = None
    owner = terminal = None
    stages = []
    try:
        server = LocalOriginal(nonce)
        env = {"SUPER_PLATINUM_ORIGINAL_DOWNLOAD_CHECK": "1", "SUPER_PLATINUM_ORIGINAL_DOWNLOAD_DIR": str(directory),
               "SUPER_PLATINUM_ORIGINAL_DOWNLOAD_NONCE": nonce, "SUPER_PLATINUM_ORIGINAL_DOWNLOAD_PORT": str(server.port)}
        native = NativeFixture(fixture, directory / "native.log", binary, env)
        with audited_fixture(native, evidence) as audit:
            owner = {"version": 1, "nonce": nonce, "pid": native.process.pid, "fixture": fixture}
            deadline = time.monotonic() + 30
            while not (waiting := records(native.log, WAITING)):
                native.require_running()
                require(not records(native.log, PREFIX) and time.monotonic() < deadline, "Native readiness failed")
                time.sleep(.04)
            require(waiting == [owner], "Exact native readiness")
            acknowledge(directory, owner, "ready")
            width = None
            for index, stage in enumerate(STAGES):
                deadline = time.monotonic() + 35
                while len(rows := records(native.log, STAGE)) <= index:
                    audit.checkpoint(stage + ":waiting")
                    require(not records(native.log, PREFIX) and time.monotonic() < deadline, "Native stage failed/expired")
                    time.sleep(.04)
                require(len(rows) == index + 1 and rows[:index] == stages, "Stage ledger changed/unpaused")
                width = validate_stage(rows[index], index, width, fixture)
                stages.append(rows[index])
                audit.capture(directory / (stage + ".png"), stage)
                # Only the second body is allowed to finish. First/third are
                # released after cancellation evidence, never to manufacture it.
                if stage == "canceled": server.release(0)
                if stage == "retry-busy": server.release(1)
                if stage == "closed": server.release(2)
                acknowledge(directory, owner, stage)
            deadline = time.monotonic() + 10
            while not (end := records(native.log, PREFIX)):
                audit.checkpoint("terminal")
                require(time.monotonic() < deadline, "Native terminal expired")
                time.sleep(.04)
            require(len(end) == 1, "Exact terminal count")
            terminal = end[0]
            validate_terminal(terminal, stages, owner)
            evidence["terminal"] = terminal
            evidence["destination"] = validate_files(directory / "downloads")
        # The owned native process has exited before this independent second
        # inventory/length/digest check. No still-running writer is certified.
        evidence["closedDestination"] = validate_closed_files(directory / "downloads", evidence["destination"])
        require([row["label"] for row in evidence.get("captures", [])] == list(STAGES), "Exact seven captures")
        require(records(native.log, WAITING, final=True) == [owner], "Closed readiness changed")
        require(records(native.log, STAGE, final=True) == stages, "Closed stage ledger changed")
        require(records(native.log, PREFIX, final=True) == [terminal], "Closed terminal changed")
        evidence["closedLogValidated"] = True
    except BaseException as error:
        primary = error
        evidence["error"] = type(error).__name__ + ": " + str(error)
        raise
    finally:
        failures = []
        try:
            if server is not None:
                server.close()
                if primary is None:
                    server.validate()
        except BaseException as error:
            failures.append(type(error).__name__ + ": " + str(error))
            if primary is not None:
                primary.add_note("Local server audit also failed: " + failures[-1])
        evidence["httpLedger"] = [] if server is None else list(server.rows)
        # Retain closed partial/failure records too; never infer a passing ledger.
        if native is not None and native.log.exists():
            try:
                require(native.log.stat().st_size <= 4 * 1024 * 1024, "Bounded closed log")
                with native.log.open("rb") as stream:
                    evidence["closedLogSha256"] = hashlib.file_digest(stream, "sha256").hexdigest()
                evidence["closedRecords"] = {"waiting": records(native.log, WAITING, final=True),
                    "stages": records(native.log, STAGE, final=True), "terminal": records(native.log, PREFIX, final=True)}
            except Exception as error:
                failures.append("closed records: " + str(error))
        evidence["localAuditErrors"] = failures
        evidence["ok"] = primary is None and not failures and evidence.get("closedLogValidated") is True
        try:
            with (directory / "receipt.json").open("x", encoding="utf-8") as stream:
                json.dump(evidence, stream, indent=2)
                stream.write("\n")
        except BaseException as error:
            if primary is None:
                raise
            primary.add_note("Could not write download receipt: " + str(error))
        if failures and primary is None:
            raise RuntimeError("Local download audit failed: " + repr(failures))
    return evidence


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=DEFAULT_BINARY)
    parser.add_argument("--captures", type=Path, required=True)
    parser.add_argument("--fixture", choices=FIXTURES)
    args = parser.parse_args()
    for fixture in ([args.fixture] if args.fixture else FIXTURES):
        result = run(fixture, args.binary.resolve(), args.captures.resolve())
        print(json.dumps({"fixture": fixture, "ok": result["ok"]}))


if __name__ == "__main__":
    main()
