#!/usr/bin/env python3
"""Disconnected actual emoji insertion/ACK; staged runner refuses native launch."""
import argparse
from contextlib import ExitStack
import json
import os
from pathlib import Path
import runpy
import secrets
import sys
import time
CONTRACT=runpy.run_path(str(Path(__file__).with_name("emoji_native_receipts.py")))
globals().update({k:v for k,v in CONTRACT.items() if not k.startswith("__")})

def run(fixture, binary, captures):
    contract(fixture)  # Refuse unknown or incomplete contracts before launch/files.
    destination = captures / fixture
    destination.mkdir(parents=True, exist_ok=False)
    nonce = secrets.token_hex(16)
    directory = destination / f"emoji-native-{nonce}"
    directory.mkdir()
    digest = binary_sha(binary)
    evidence = {"ok": False, "fixture": fixture, "nonce": nonce, "platform": sys.platform,
                "binary": str(binary), "binarySha256": digest, "captureDirectory": str(directory),
                "limitations": ["Synthetic disconnected interaction, not live Slack.",
                                "Pointer/hit-test/geometry observations are sampled, not atomic capture proof.",
                                "Owned evidence directory is retained; no real session or Transport is installed.",
                                "Actual native admission/ACK; no prepared preference requests or writer HTTP coverage.",
                                "Synthetic key events and acknowledgements do not establish trusted keyboard or HTTP behavior."]}
    env = {FLAG: "1", "SUPER_PLATINUM_EMOJI_NATIVE_NONCE": nonce,
           "SUPER_PLATINUM_EMOJI_NATIVE_DIR": str(directory)}
    native = NativeFixture(fixture, destination / f"{fixture}.log", binary, env)
    process_api = handle = None
    try:
        with native:
            evidence["pid"] = native.process.pid
            if sys.platform == "win32":
                from windows_process_metrics import WindowsAPI
                process_api = WindowsAPI()
                handle = process_api.open(native.process.pid)
                evidence["processCreationFiletime"] = process_api.times(handle)["creation_filetime"]
            try:
                with ExitStack() as stack:
                    expected = ready_tuple(fixture, nonce, native.process.pid)
                    deadline = time.monotonic() + 25
                    while not (waiting := native.receipts(WAITING)):
                        native.require_running()
                        if native.receipts(PREFIX) or time.monotonic() >= deadline:
                            raise RuntimeError("Fixture did not reach bounded readiness")
                        time.sleep(.05)
                    if len(waiting) != 1:
                        raise RuntimeError("Duplicate readiness request")
                    validate_ready(waiting[0], expected)
                    pointer = normalize(native, evidence, stack, process_api, handle)
                    publish_ready(directory, expected)
                    evidence["readiness"] = expected
                    deadline, sampled, stage_count = time.monotonic() + 170, 0., 0
                    while True:
                        native.require_running()
                        edges = native.receipts(STAGE)
                        if pointer:
                            for edge in edges[stage_count:]:
                                pointer.check(f"{edge.get('stage')}:{edge.get('edge')}", require_clear=True)
                            if time.monotonic() - sampled >= .2:
                                pointer.check("running", require_clear=True)
                                sampled = time.monotonic()
                        stage_count = len(edges)
                        receipts = native.receipts(PREFIX)
                        if receipts:
                            if len(receipts) != 1:
                                raise RuntimeError("Duplicate terminal receipt")
                            evidence["native"] = receipts[0]
                            evidence["pngDimensions"] = validate_native(fixture, nonce, native.process.pid, directory, receipts[0], edges,
                                                                         expected_dimensions=evidence.get("expectedPngDimensions"))
                            if pointer:
                                pointer.check("terminal", require_clear=True)
                            break
                        if time.monotonic() >= deadline:
                            raise TimeoutError("Native emoji assertions timed out")
                        time.sleep(.05)
            except BaseException:
                try:
                    path = destination / f"{fixture}-failure.png"
                    native.capture(path)
                    evidence["failureScreenshot"] = str(path)
                except (OSError, RuntimeError, ValueError):
                    pass
                raise
        evidence["processExited"] = native.process.poll() is not None
        if not evidence["processExited"]:
            raise RuntimeError("Owned process did not exit")
        if sys.platform == "win32":
            validate_pointer_cleanup(evidence.get("pointer", {}))
        evidence["pngDimensions"] = validate_closed_log(native.log, expected, directory, evidence["native"],
                                                        evidence.get("expectedPngDimensions"))
        evidence["closedLogRevalidated"] = True
        evidence["pngValidation"] = "Complete chunk/CRC/IEND and bounded noninterlaced IDAT scanline validation; no pixel-color decoding"
        if handle:
            exit_identity = process_api.times(handle)
            evidence["exitIdentity"] = exit_identity
            validate_exit(evidence["processCreationFiletime"], exit_identity)
        evidence["binarySha256After"] = binary_sha(binary)
        if evidence["binarySha256After"] != digest:
            raise RuntimeError("Binary changed during native run")
        evidence["ok"] = True
    except BaseException as error:
        evidence.update(ok=False, error=f"{type(error).__name__}: {error}")
        raise
    finally:
        # This executes after pointer restoration and NativeFixture cleanup, so
        # failure receipts also retain their actual cleanup outcomes.
        evidence["cleanupErrors"] = list(native.cleanup_errors)
        try:
            try:
                evidence["processExited"] = native.process is not None and native.process.poll() is not None
            except BaseException as cleanup_error:
                detail = f"process status cleanup: {type(cleanup_error).__name__}: {cleanup_error}"
                evidence["processExited"] = False
                evidence["cleanupErrors"].append(detail)
                evidence["ok"] = False
                evidence.setdefault("error", detail)
            if evidence["cleanupErrors"]:
                evidence["ok"] = False
                evidence.setdefault("error", "Native cleanup reported an error")
            if not evidence["processExited"]:
                evidence["ok"] = False
                evidence.setdefault("error", "Owned process did not exit during cleanup")
            if handle:
                try:
                    evidence["cleanupProcessIdentity"] = process_api.times(handle)
                    validate_exit(evidence["processCreationFiletime"], evidence["cleanupProcessIdentity"])
                    if evidence.get("exitIdentity") is not None:
                        same(evidence["cleanupProcessIdentity"], evidence["exitIdentity"],
                             "Retained process identity changed after exit")
                except BaseException as cleanup_error:
                    detail = f"process identity cleanup: {type(cleanup_error).__name__}: {cleanup_error}"
                    evidence["cleanupIdentityError"] = str(cleanup_error)
                    evidence["cleanupErrors"].append(detail)
                    evidence["ok"] = False
                    evidence.setdefault("error", detail)
        finally:
            if handle:
                try:
                    if not process_api.kernel.CloseHandle(handle):
                        raise OSError("CloseHandle returned false")
                except BaseException as cleanup_error:
                    detail = f"process handle cleanup: {type(cleanup_error).__name__}: {cleanup_error}"
                    evidence["cleanupErrors"].append(detail)
                    evidence["ok"] = False
                    evidence.setdefault("error", detail)
        path = destination / f"{fixture}.json"
        path.write_text(json.dumps(evidence, indent=2) + "\n", encoding="utf-8")
        print(json.dumps({"receipt": str(path), "ok": evidence["ok"]}), flush=True)
    if not evidence["ok"]:
        raise RuntimeError(evidence.get("error", "Final retained process audit failed"))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path(os.environ.get("SUPER_PLATINUM_BINARY", DEFAULT_BINARY)))
    parser.add_argument("--captures", type=Path, default=ROOT / "tmp/agent-ui/windows-emoji-native")
    parser.add_argument("--fixture", action="append", choices=FIXTURES)
    args = parser.parse_args()
    for fixture in args.fixture or FIXTURES:
        run(fixture, args.binary.resolve(), args.captures.resolve())
