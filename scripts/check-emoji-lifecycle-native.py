#!/usr/bin/env python3
"""Cold first-valid shortcode/picker, refresh and controlled reader; never builds."""
import argparse
import ctypes
from contextlib import ExitStack
import json
import os
from pathlib import Path
import runpy
import secrets
import sys
import time
CONTRACT=runpy.run_path(str(Path(__file__).with_name("emoji_lifecycle_receipts.py")))
globals().update({k:v for k,v in CONTRACT.items() if not k.startswith("__")})


def finalize_cleanup(evidence, native, process_api, handle):
    """Retain cleanup failures without replacing a primary native/body error."""
    errors = list(native.cleanup_errors)
    if evidence.get("pointerCleanupError"):
        errors.append({"stage": "pointer", "type": "RuntimeError", "message": evidence["pointerCleanupError"]})
    if handle:
        try:
            evidence["cleanupProcessIdentity"] = process_api.times(handle)
        except Exception as error:
            evidence["cleanupIdentityError"] = str(error)
            errors.append({"stage": "process-identity", "type": type(error).__name__, "message": str(error)})
        try:
            closed = bool(process_api.kernel.CloseHandle(handle))
            evidence["processHandleClosed"] = closed
            if not closed:
                code = ctypes.get_last_error()
                raise OSError(code, "CloseHandle returned false")
        except Exception as error:
            evidence["processHandleClosed"] = False
            evidence["handleCloseError"] = f"{type(error).__name__}: {error}"
            errors.append({"stage": "process-handle-close", "type": type(error).__name__, "message": str(error)})
    evidence["cleanupErrors"] = errors
    if errors:
        evidence["ok"] = False
        evidence.setdefault("error", "Final cleanup audit failed")


def run(fixture, binary, captures, build_receipt):
    contract(fixture)  # Refuse unknown or incomplete contracts before launch/files.
    provenance=json.loads(build_receipt.read_text(encoding="utf-8"))
    if provenance["binary_sha256"]!=binary_sha(binary):
        raise RuntimeError("Binary differs from sealed build")
    if provenance["arm"] not in ("full","thin") or provenance["profile"]!="dev" or provenance["overrides"]:
        raise RuntimeError("Wrong build arm/profile/overrides")
    if provenance["manifest_sha256"]!=binary_sha(Path(provenance["manifest_path"])):
        raise RuntimeError("Build used another frozen harness manifest")
    destination = captures / fixture
    destination.mkdir(parents=True, exist_ok=False)
    nonce = secrets.token_hex(16)
    directory = destination / f"emoji-lifecycle-{nonce}"
    directory.mkdir()
    digest = binary_sha(binary)
    evidence = {"ok": False, "fixture": fixture, "nonce": nonce, "platform": sys.platform,
                "binary": str(binary), "binarySha256": digest, "buildReceipt":str(build_receipt), "buildReceiptSha256":binary_sha(build_receipt), "build":provenance, "captureDirectory": str(directory),
                "limitations": ["Synthetic disconnected interaction, not live Slack.",
                                "Pointer/hit-test/geometry observations are sampled, not atomic capture proof.",
                                "Owned evidence directory is retained; no real session or Transport is installed.",
                                "Cold synthetic input/accepted DOM/two RAF; not trusted input, physical paint, or memory.",
                                "Full/thin require matched source manifests and ordinary runtime compilation profile; fresh process per attempt."]}
    env = {FLAG: "1", "SUPER_PLATINUM_EMOJI_LIFECYCLE_NONCE": nonce,
           "SUPER_PLATINUM_EMOJI_LIFECYCLE_DIR": str(directory)}
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
                with pointer_cleanup(evidence) as stack:
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
        evidence["processExited"] = native.process is not None and native.process.poll() is not None
        try:
            evidence["retainedProgress"] = closed_receipts(native.log, PROGRESS)
        except (OSError, ValueError, RuntimeError) as error:
            evidence["progressReadError"] = f"{type(error).__name__}: {error}"
        finalize_cleanup(evidence, native, process_api, handle)
        path = destination / f"{fixture}.json"
        path.write_text(json.dumps(evidence, indent=2) + "\n", encoding="utf-8")
        print(json.dumps({"receipt": str(path), "ok": evidence["ok"]}), flush=True)
    if not evidence["ok"]:
        raise RuntimeError("Final retained process audit failed")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=Path(os.environ.get("SUPER_PLATINUM_BINARY", DEFAULT_BINARY)))
    parser.add_argument("--captures", type=Path, default=ROOT / "tmp/agent-ui/windows-emoji-lifecycle")
    parser.add_argument("--build-receipt", type=Path, required=True)
    parser.add_argument("--fixture", action="append", choices=FIXTURES)
    args = parser.parse_args()
    for fixture in args.fixture or FIXTURES:
        run(fixture, args.binary.resolve(), args.captures.resolve(), args.build_receipt.resolve())
