"""Sampled owned-process/capture evidence for disconnected Canvas runners.

This adds no app commands except the existing focus/screenshot normalization.
It is not an atomic occlusion proof or a raw Slack-request ledger.
"""
from contextlib import ExitStack, contextmanager
from pathlib import Path
import runpy
import sys

SCRIPTS = next(p / "scripts" for p in Path(__file__).resolve().parents
               if (p / "Cargo.toml").is_file() and (p / "scripts/native_fixture.py").is_file())
sys.path.insert(0, str(SCRIPTS))
_SHARED = runpy.run_path(str(SCRIPTS / "check-slack-list-board-add-native.py"))
binary_sha, png_signature, normalize, validate_exit, validate_pointer_cleanup, closed_receipts = (
    _SHARED[k] for k in ("binary_sha", "png_signature", "normalize", "validate_exit",
                        "validate_pointer_cleanup", "closed_receipts"))


def open_process(pid):
    from windows_process_metrics import WindowsAPI
    api = WindowsAPI()
    return api, api.open(pid)


@contextmanager
def pointer_cleanup(evidence):
    stack = ExitStack()
    primary = None
    try:
        yield stack
    except BaseException as error:
        primary = error
        raise
    finally:
        try:
            stack.close()
        except BaseException as error:
            detail = f"{type(error).__name__}: {error}"
            evidence["pointerCleanupError"] = detail
            if primary is None:
                raise
            if hasattr(primary, "add_note"):
                primary.add_note("Canvas pointer cleanup also failed: " + detail)


class CaptureAudit:
    def __init__(self, native, evidence, api, handle, pointer):
        self.native, self.evidence = native, evidence
        self.api, self.handle, self.pointer = api, handle, pointer

    def checkpoint(self, label):
        self.native.require_running()
        if self.handle is not None:
            current = self.api.times(self.handle)
            if (current["creation_filetime"] != self.evidence["processCreationFiletime"]
                    or current["exit_filetime"] != 0):
                raise RuntimeError("Canvas owned process identity changed")
        if self.pointer is not None:
            self.pointer.check(label, require_clear=True)

    def capture(self, path, label):
        path = Path(path).resolve()
        self.checkpoint(label + ":before")
        self.native.capture(path)
        dimensions = png_signature(path, self.evidence.get("expectedPngDimensions"))
        self.checkpoint(label + ":after")
        self.evidence.setdefault("captures", []).append({
            "label": label, "path": str(path), "dimensions": dimensions})
        return dimensions


@contextmanager
def audited_fixture(native, evidence):
    """Own NativeFixture and restore pointer before terminating its process.

Always retain cleanup outcomes. If a body failed, audit failures are attached
to that original exception instead of replacing the actual native failure.
"""
    api = handle = None
    primary = None
    evidence.update(binary=str(native.binary), binarySha256=binary_sha(native.binary))
    evidence["captureEvidenceScope"] = (
        "Sampled owned-window/pointer checks around captures; no continuous input "
        "monitoring or atomic occlusion guarantee. No raw request ledger added.")
    try:
        with native:
            evidence["pid"] = native.process.pid
            if sys.platform == "win32":
                api, handle = open_process(native.process.pid)
                evidence["processCreationFiletime"] = api.times(handle)["creation_filetime"]
            with pointer_cleanup(evidence) as stack:
                pointer = normalize(native, evidence, stack, api, handle)
                audit = CaptureAudit(native, evidence, api, handle, pointer)
                try:
                    yield audit
                except BaseException:
                    # Failed-state capture precedes pointer restoration/process exit.
                    # It is diagnostic only and cannot replace the original error.
                    path = native.log.with_name(native.log.stem + "-failure.png")
                    if path.exists():
                        path = native.log.with_name(native.log.stem + "-error.png")
                        suffix = 2
                        while path.exists():
                            path = native.log.with_name(native.log.stem + f"-error-{suffix}.png")
                            suffix += 1
                    try:
                        audit.capture(path, "failure")
                        evidence["failureScreenshot"] = str(path)
                    except Exception as failure:
                        evidence["failureCaptureError"] = f"{type(failure).__name__}: {failure}"
                    raise
    except BaseException as error:
        primary = error
        raise
    finally:
        failures = []

        def collect(label, action):
            try:
                action()
            except Exception as error:
                failures.append({"stage": label, "type": type(error).__name__, "message": str(error)})

        # A failed OS poll is missing exit evidence, not a successful exit.
        evidence["processExited"] = False
        evidence["exitCode"] = None
        try:
            def poll():
                code = None if native.process is None else native.process.poll()
                evidence["exitCode"] = code
                evidence["processExited"] = code is not None
            collect("poll", poll)
            evidence["cleanupErrors"] = list(native.cleanup_errors)
            if not evidence["processExited"]:
                failures.append({"stage": "process", "type": "RuntimeError", "message": "Owned process exit not observed"})
            if evidence["cleanupErrors"]:
                failures.append({"stage": "cleanup", "type": "RuntimeError", "message": "Native fixture cleanup reported errors"})
            if evidence.get("pointerCleanupError"):
                failures.append({"stage": "pointer-cleanup", "type": "RuntimeError",
                                 "message": evidence["pointerCleanupError"]})
            if handle is not None:
                def identity():
                    value = api.times(handle)
                    evidence["cleanupProcessIdentity"] = value
                    evidence["exitIdentity"] = value
                    validate_exit(evidence["processCreationFiletime"], value)
                collect("identity", identity)
        finally:
            if handle is not None:
                def close_handle():
                    if not api.kernel.CloseHandle(handle):
                        raise OSError("Could not close retained Canvas process handle")
                collect("handle", close_handle)
        if sys.platform == "win32":
            collect("pointer", lambda: validate_pointer_cleanup(evidence.get("pointer", {})))

        def digest():
            evidence["binarySha256After"] = binary_sha(native.binary)
            if evidence["binarySha256After"] != evidence["binarySha256"]:
                raise RuntimeError("Binary changed during Canvas run")
        collect("binary", digest)
        evidence["auditErrors"] = failures
        if failures:
            evidence["ok"] = False
            detail = "Canvas evidence cleanup failed: " + repr(failures)
            if primary is not None:
                if hasattr(primary, "add_note"):
                    primary.add_note(detail)
            else:
                raise RuntimeError(detail)
