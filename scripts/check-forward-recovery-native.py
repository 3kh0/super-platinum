#!/usr/bin/env python3
"""Signed, disconnected seed/restart/retire checks using one isolated journal."""
import json
import os
import pathlib
import sqlite3
import subprocess
import tempfile
import time
import uuid

ROOT = pathlib.Path(__file__).resolve().parents[1]
BINARY = pathlib.Path(os.environ.get(
    "SUPER_PLATINUM_BINARY",
    str(ROOT / "target/debug/super-platinum"),
))
CAPTURES = ROOT / "tmp/agent-ui"
TEMP = pathlib.Path(os.environ.get("TMPDIR", tempfile.gettempdir()))
MARKER = TEMP / "super-platinum-agent.sock.path"
STORAGE = ROOT / "tmp" / f"forward-recovery-{uuid.uuid4()}.sqlite"
ACCOUNT = "00000000-0000-4000-8000-000000000071"


def activate(pid):
    subprocess.run(
        ["osascript", "-l", "JavaScript", "-e",
         'ObjC.import("AppKit"); '
         f"$.NSRunningApplication.runningApplicationWithProcessIdentifier({pid})"
         ".activateWithOptions(2)"],
        stdout=subprocess.DEVNULL, check=True,
    )


def receipts():
    with sqlite3.connect(STORAGE) as connection:
        owner = connection.execute("SELECT account FROM identity WHERE singleton=1").fetchone()[0]
        assert owner == ACCOUNT, "fixture account identity changed"
        return [json.loads(row[0]) for row in connection.execute("SELECT body FROM receipts")]


def run_case(fixture, label, mode):
    socket = TEMP / f"forward-recovery-{os.getpid()}.sock"
    env = dict(os.environ)
    # A caller may have used the ordinary forward fixture runner earlier.
    env.pop("SUPER_PLATINUM_FORWARD_CHECK", None)
    env.update(
        SUPER_PLATINUM_FIXTURE=fixture,
        SUPER_PLATINUM_FORWARD_RECOVERY_CHECK=mode,
        SUPER_PLATINUM_FORWARD_RECOVERY_DB=str(STORAGE),
        SUPER_PLATINUM_AGENT="1",
        SUPER_PLATINUM_AGENT_SOCK=str(socket),
    )
    log = CAPTURES / f"{label}.log"
    previous = MARKER.read_text() if MARKER.exists() else None
    with log.open("w") as output:
        process = subprocess.Popen([str(BINARY)], env=env, stdout=output, stderr=output)
        try:
            ready = time.monotonic() + 8
            while not socket.exists() and process.poll() is None and time.monotonic() < ready:
                time.sleep(.05)
            deadline = time.monotonic() + 45
            last_activation = 0
            while True:
                if time.monotonic() - last_activation > 1:
                    activate(process.pid)
                    last_activation = time.monotonic()
                result = log.read_text()
                if "forward recovery check failed" in result:
                    raise RuntimeError(result)
                if "forward recovery check passed" in result:
                    break
                if process.poll() is not None or time.monotonic() > deadline:
                    raise RuntimeError(f"{fixture}: exited or timed out: {result}")
                time.sleep(.1)
            time.sleep(.25)
            subprocess.run(
                ["scripts/agentctl.sh", "screenshot", str(CAPTURES / f"{label}.png")],
                env=env, check=True, stdout=subprocess.DEVNULL,
            )
            print(f"{label}: checked and captured", flush=True)
        except Exception:
            # Preserve fixture-only evidence before cleanup, without replacing
            # the original failure or waiting indefinitely on a dead process.
            if process.poll() is None and socket.exists():
                try:
                    with (CAPTURES / f"{label}-failed-state.json").open("w") as state:
                        subprocess.run(
                            ["scripts/agentctl.sh", "state"], env=env,
                            stdout=state, stderr=subprocess.DEVNULL, timeout=3,
                        )
                except Exception:
                    pass
                try:
                    subprocess.run(
                        ["scripts/agentctl.sh", "screenshot",
                         str(CAPTURES / f"{label}-failed.png")],
                        env=env, stdout=subprocess.DEVNULL,
                        stderr=subprocess.DEVNULL, timeout=5,
                    )
                except Exception:
                    pass
            raise
        finally:
            # Stop only the disconnected fixture process created above.
            if process.poll() is None:
                process.terminate()
                try:
                    process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait()
            if MARKER.exists() and pathlib.Path(MARKER.read_text().strip()) == socket:
                MARKER.unlink()
                if previous:
                    restore_env = dict(os.environ, SUPER_PLATINUM_AGENT_SOCK=previous.strip())
                    live = subprocess.run(
                        ["scripts/agentctl.sh", "ping"], env=restore_env,
                        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=3,
                    )
                    if live.returncode == 0:
                        MARKER.write_text(previous)
            socket.unlink(missing_ok=True)


def main():
    os.chdir(ROOT)
    CAPTURES.mkdir(parents=True, exist_ok=True)
    initial = None
    cases = [
        ("forward-recovery", "forward-recovery-seed", "seed"),
        ("forward-recovery-narrow", "forward-recovery-restart", "restart"),
        ("forward-recovery", "forward-recovery-retire", "retire"),
    ]
    for fixture, label, mode in cases:
        run_case(fixture, label, mode)
        rows = receipts()
        assert len(rows) == 1, "recovery must not create a second request"
        row = rows[0]
        assert "Uncertain" in row["delivery"]["status"], "retirement cannot claim delivery"
        assert row["retired"] == (mode == "retire"), "unexpected retirement"
        assert row["scope"]["destination"] == "C_FORWARD_RECOVERY_MISSING"
        assert "Recovered note · 日本語 🚀" in json.dumps(row["delivery"]["request"]["blocks"], ensure_ascii=False)
        if initial is None:
            initial = row
        else:
            assert row["attempt_id"] == initial["attempt_id"], "offline actions must not start a resend"
            assert row["delivery"] == initial["delivery"], "restart must preserve the exact request and note"
            assert row["scope"] == initial["scope"], "restart must preserve source and destination"
    print("restart database verified: original request retained, tracking explicitly retired", flush=True)
    print(f"isolated database: {STORAGE}", flush=True)


if __name__ == "__main__":
    main()
