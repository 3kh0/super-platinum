#!/usr/bin/env python3
"""Drive and capture isolated signed slack-list fixtures without OS input."""
import json
import os
import pathlib
import subprocess
import time

ROOT = pathlib.Path(__file__).resolve().parents[1]
BINARY = ROOT / "target/debug/Super Platinum.app/Contents/MacOS/super-platinum"
TMP = pathlib.Path(os.environ.get("TMPDIR", "/tmp"))
MARKER = TMP / "super-platinum-agent.sock.path"
CAPTURES = ROOT / "tmp/agent-ui"
CAPTURES.mkdir(parents=True, exist_ok=True)

for fixture in ("slack-list", "slack-list-narrow"):
    capture_name = fixture
    socket = TMP / f"slack-list-{os.getpid()}.sock"
    env = dict(os.environ, SUPER_PLATINUM_FIXTURE=fixture,
               SUPER_PLATINUM_AGENT="1", SUPER_PLATINUM_AGENT_SOCK=str(socket),
               SUPER_PLATINUM_SLACK_LIST_CHECK="1",
               SUPER_PLATINUM_SLACK_LIST_CAPTURE_DIR=str(CAPTURES))
    previous = MARKER.read_text() if MARKER.exists() else None
    log = CAPTURES / f"{capture_name}.log"

    def control(*args, check=True, tool_env=env):
        return subprocess.run([str(ROOT / "scripts/agentctl.sh"), *map(str, args)],
                              cwd=ROOT, env=tool_env, capture_output=True,
                              text=True, timeout=8, check=check)

    with log.open("w") as output:
        process = subprocess.Popen([str(BINARY)], cwd=ROOT, env=env,
                                   stdout=output, stderr=output)
        try:
            deadline = time.monotonic() + 30
            while not socket.exists():
                if process.poll() is not None or time.monotonic() > deadline:
                    raise RuntimeError(f"{capture_name}: failed to start")
                time.sleep(.1)
            control("ping")
            def activate():
                result = subprocess.run(["osascript", "-l", "JavaScript", "-e",
                                         'ObjC.import("AppKit"); '
                                         f'var app=$.NSRunningApplication.runningApplicationWithProcessIdentifier({process.pid}); '
                                         'if(app.finishedLaunching) app.activateWithOptions(2); '
                                         'app.finishedLaunching && app.active'],
                                        capture_output=True, text=True, check=True, timeout=5)
                return result.stdout.strip() == "true"

            # Socket readiness precedes the first painted window. Keep the
            # isolated fixture active so WebKit does not suspend frame work.
            activation_deadline = time.monotonic() + 10
            while not activate():
                if process.poll() is not None or time.monotonic() > activation_deadline:
                    raise RuntimeError(f"{capture_name}: window failed to activate")
                time.sleep(.1)
            deadline = time.monotonic() + 60
            last_activation = time.monotonic()
            while True:
                if time.monotonic() - last_activation > 1:
                    activate()
                    last_activation = time.monotonic()
                receipts = [line.split("slack-list-check: ", 1)[1]
                            for line in log.read_text().splitlines(keepends=True)
                            if line.endswith("\n") and "slack-list-check: " in line]
                if receipts:
                    receipt = json.loads(receipts[-1])
                    if receipt.get("ok") is not True:
                        raise RuntimeError(f"{capture_name}: native assertion failed: {receipt}")
                    for stage, proof in (("table", receipt.get("horizontal", {})),
                                         ("reordered", receipt.get("savedViews", {}).get("reorderedHorizontal", {}))):
                        for flag in ("compactColumns", "primaryPinned", "opaqueFocusState", "syntheticNavigationHitTest", "homeRestoresLeft"):
                            if proof.get(flag) is not True:
                                raise RuntimeError(f"{capture_name}: missing {stage} horizontal proof: {flag}")
                    if receipt.get("savedViews", {}).get("hiddenPrimaryUnpinned") is not True:
                        raise RuntimeError(f"{capture_name}: hidden primary kept stale scroll clearance")
                    for surface, proof in (("table", receipt.get("keyboardHelp", {})),
                                           ("board", receipt.get("boardViews", {}).get("keyboardHelp", {})),
                                           ("record", receipt.get("recordDetails", {}).get("keyboardHelp", {}))):
                        for flag in ("collapsedAccessible", "descriptionPreserved", "nativeDisclosure", "expandedBounded", "closeRestoresContent", "focusRestored"):
                            if proof.get(flag) is not True:
                                raise RuntimeError(f"{capture_name}: missing {surface} keyboard help proof: {flag}")
                    for stage in ("rich", "references", "hierarchy"):
                        path = CAPTURES / f"{capture_name}-{stage}.png"
                        if receipt.get("captures", {}).get(stage) != str(path) or not path.is_file():
                            raise RuntimeError(f"{capture_name}: missing {stage} capture")
                    for kind in ("rich", "select"):
                        path = CAPTURES / f"{capture_name}-editor-{kind}.png"
                        if receipt.get("cellEditors", {}).get("captures", {}).get(kind) != str(path) or not path.is_file():
                            raise RuntimeError(f"{capture_name}: missing {kind} editor capture")
                    for kind, stage in (("columns", "saved-view-columns"), ("groups", "saved-view-groups")):
                        path = CAPTURES / f"{capture_name}-{stage}.png"
                        if receipt.get("savedViews", {}).get("captures", {}).get(kind) != str(path) or not path.is_file():
                            raise RuntimeError(f"{capture_name}: missing saved view {kind} capture")
                    for kind, stage in (("today", "saved-date-today"), ("week", "saved-date-week")):
                        path = CAPTURES / f"{capture_name}-{stage}.png"
                        if receipt.get("dateViews", {}).get("captures", {}).get(kind) != str(path) or not path.is_file():
                            raise RuntimeError(f"{capture_name}: missing date view {kind} capture")
                    for kind, stage in (("multi", "board-multi"), ("paging", "board-paging"), ("completion", "board-completion")):
                        path = CAPTURES / f"{capture_name}-{stage}.png"
                        if receipt.get("boardViews", {}).get("captures", {}).get(kind) != str(path) or not path.is_file():
                            raise RuntimeError(f"{capture_name}: missing board view {kind} capture")
                    if receipt.get("recordDetails", {}).get("refreshPreservesPanelPageFocus") is not True:
                        raise RuntimeError(f"{capture_name}: record detail refresh stability was not verified")
                    for kind in ("table", "board", "unavailable"):
                        path = CAPTURES / f"{capture_name}-detail-{kind}.png"
                        if receipt.get("recordDetails", {}).get("captures", {}).get(kind) != str(path) or not path.is_file():
                            raise RuntimeError(f"{capture_name}: missing record detail {kind} capture")
                    for kind, stage in (("editor", "date-editor"), ("hidden", "date-hidden-editor")):
                        date_editor = CAPTURES / f"{capture_name}-{stage}.png"
                        if receipt.get("dateEdits", {}).get("captures", {}).get(kind) != str(date_editor) or not date_editor.is_file():
                            raise RuntimeError(f"{capture_name}: missing date {kind} capture")
                    for flag in ("cancelNoWrite", "cancelPendingNormalization", "uncontrolledDrafts", "normalizedSave", "rename", "displayModes", "removeLink", "unsafeStorageInert", "duplicateLocked", "keyboardFocus", "boundedLayout", "cleanup"):
                        if receipt.get("linkEdits", {}).get(flag) is not True:
                            raise RuntimeError(f"{capture_name}: missing Link editor proof: {flag}")
                    link_editor = CAPTURES / f"{capture_name}-link-editor.png"
                    if receipt.get("linkEdits", {}).get("captures", {}).get("editor") != str(link_editor) or not link_editor.is_file():
                        raise RuntimeError(f"{capture_name}: missing Link editor capture")
                    user_editor = CAPTURES / f"{capture_name}-user-editor.png"
                    if receipt.get("userEdits", {}).get("captures", {}).get("editor") != str(user_editor) or not user_editor.is_file():
                        raise RuntimeError(f"{capture_name}: missing people editor capture")
                    for flag in ("candidateFilters", "guestAndExternal", "typingAndCancelNoWrite", "inputSurvivesResultRender", "singleReplace", "clear", "uncachedRemoval", "multiSelection", "taskAssigneeReadOnly", "keyboardEntryAndFocus", "duplicateLocked", "canonicalWrites", "cleanup"):
                        if receipt.get("userEdits", {}).get(flag) is not True:
                            raise RuntimeError(f"{capture_name}: missing people editor verification {flag}")
                    channel_editor = CAPTURES / f"{capture_name}-channel-editor.png"
                    if receipt.get("channelEdits", {}).get("captures", {}).get("editor") != str(channel_editor) or not channel_editor.is_file():
                        raise RuntimeError(f"{capture_name}: missing channel editor capture")
                    for flag in ("candidateFilters", "privateAndExternal", "typingAndCancelNoWrite", "inputSurvivesResultRender", "singleReplace", "clear", "uncachedRemoval", "multiSelection", "keyboardEntryAndFocus", "duplicateLocked", "canonicalWrites", "cleanup"):
                        if receipt.get("channelEdits", {}).get(flag) is not True:
                            raise RuntimeError(f"{capture_name}: missing channel editor verification {flag}")
                    for kind, stage in (("created", "list-created"), ("uncertain", "list-create-uncertain")):
                        path = CAPTURES / f"{capture_name}-{stage}.png"
                        if receipt.get("creation", {}).get("captures", {}).get(kind) != str(path) or not path.is_file():
                            raise RuntimeError(f"{capture_name}: missing creation {kind} capture")
                    break
                if process.poll() is not None or time.monotonic() > deadline:
                    raise RuntimeError(f"{capture_name}: native assertions timed out")
                time.sleep(.1)
            control("screenshot", CAPTURES / f"{capture_name}.png")
            print(f"{capture_name}: native list interactions, cell editors, saved tables, boards, record details, date edits and creation passed; overview, content, editors and views captured", flush=True)
        except BaseException:
            try:
                control("screenshot", CAPTURES / f"{capture_name}-failure.png", check=False)
            except (OSError, subprocess.SubprocessError):
                pass  # Preserve the original startup/assertion failure.
            raise
        finally:
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
                    prior = dict(os.environ, SUPER_PLATINUM_AGENT_SOCK=previous.strip())
                    try:
                        if control("ping", check=False, tool_env=prior).returncode == 0:
                            MARKER.write_text(previous)
                    except (OSError, subprocess.SubprocessError):
                        pass  # A stale prior marker must not mask this run's result.
            socket.unlink(missing_ok=True)
