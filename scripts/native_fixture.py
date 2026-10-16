"""Isolated native fixture processes and bounded NDJSON control (Windows/Unix).

No build, login, shell CLI, or global agent discovery marker is required. Callers
must inspect captured PNGs separately before claiming visual verification.
"""
import itertools
import json
import os
from pathlib import Path
import socket
import struct
import subprocess
import sys
import tempfile
import time
from urllib.parse import urlsplit
import warnings

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_BINARY = ROOT / "target/debug" / (
    "super-platinum.exe" if sys.platform == "win32" else "super-platinum"
)
_IDS = itertools.count(1)
MAX_RESPONSE = 8 * 1024 * 1024


def request(endpoint, command, timeout=20, **fields):
    """Send once; a timeout never replays a possibly executed command."""
    request_id = next(_IDS)
    endpoint = str(endpoint)
    if endpoint.startswith("tcp://"):
        address = urlsplit(endpoint)
        if address.hostname not in ("127.0.0.1", "::1", "localhost"):
            raise ValueError("native fixture agent must use loopback TCP")
        stream = socket.create_connection((address.hostname, address.port), timeout)
    else:
        stream = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        stream.settimeout(timeout)
        try:
            stream.connect(endpoint)
        except BaseException:
            stream.close()
            raise
    with stream:
        stream.settimeout(timeout)
        stream.sendall((json.dumps({"id": request_id, "cmd": command, **fields}) + "\n").encode())
        with stream.makefile("rb") as source:
            line = source.readline(MAX_RESPONSE + 1)
    if len(line) > MAX_RESPONSE or not line.endswith(b"\n"):
        raise RuntimeError(f"{command}: missing or oversized agent response")
    response = json.loads(line)
    if response.get("id") != request_id:
        raise RuntimeError(f"{command}: mismatched agent response ID")
    if response.get("ok") is not True:
        raise RuntimeError(f"{command}: {response.get('error', 'agent request failed')}")
    return response


class NativeFixture:
    """Own exactly one disconnected fixture and its private temporary endpoint."""

    def __init__(self, fixture, log, binary=None, checks=None, *, after_exit=None):
        self.fixture = fixture
        self.log = Path(log).resolve()
        self.binary = Path(binary or os.environ.get("SUPER_PLATINUM_BINARY", DEFAULT_BINARY)).resolve()
        self.checks = checks or {}
        self.process = None
        self.endpoint = None
        self._temporary = None
        self._output = None
        self._temporary_owner = None
        self._temporary_path = None
        self._temporary_name = None
        self._temporary_identity = None
        self.cleanup_errors = []
        self._after_exit = after_exit
        self._after_exit_attempted = False
        self._entered = False

    def __enter__(self):
        if not self.binary.is_file():
            raise FileNotFoundError(f"native binary missing: {self.binary}; build it first")
        self.log.parent.mkdir(parents=True, exist_ok=True)
        self._temporary = tempfile.TemporaryDirectory(prefix="sp-fixture-")
        temporary = Path(self._temporary.name).resolve(strict=True)
        self._temporary_owner = self._temporary
        self._temporary_path = temporary
        self._temporary_name = self._temporary.name
        info = temporary.stat()
        self._temporary_identity = (info.st_dev, info.st_ino)
        # Cleanup is explicit and ownership-checked below. Disable the stdlib
        # finalizer so a retained/redirected path is never deleted later without
        # that check. TemporaryDirectory.cleanup still works after detach.
        self._temporary._finalizer.detach()
        self._marker = temporary / "super-platinum-agent.sock.path"
        # Rust's temp_dir uses TMPDIR on Unix and the OS temporary path on
        # Windows. Overriding all three isolates the server discovery file as
        # well as WebView scratch data, leaving the active client's marker alone.
        env = {key: value for key, value in os.environ.items()
               if not key.startswith("SUPER_PLATINUM_")}
        env.update(TMP=str(temporary), TEMP=str(temporary), TMPDIR=str(temporary),
                   SUPER_PLATINUM_FIXTURE=self.fixture, SUPER_PLATINUM_AGENT="1")
        if sys.platform == "win32":
            env["SUPER_PLATINUM_AGENT_ADDR"] = "127.0.0.1:0"
        else:
            env["SUPER_PLATINUM_AGENT_SOCK"] = str(temporary / "agent.sock")
        env.update(self.checks)
        try:
            if self._after_exit is not None and any(env[name] != str(temporary)
                                                   for name in ("TMP", "TEMP", "TMPDIR")):
                raise ValueError("Closed audit requires the owned child temporary environment")
            self._output = self.log.open("w", encoding="utf-8")
            self.process = subprocess.Popen([str(self.binary)], cwd=ROOT, env=env,
                                            stdout=self._output, stderr=self._output,
                                            # Suppress a dev console, not the
                                            # native GUI window being checked.
                                            creationflags=(subprocess.CREATE_NO_WINDOW
                                                           if sys.platform == "win32" else 0))
            deadline = time.monotonic() + 30
            while True:
                self.require_running()
                if self._marker.is_file():
                    endpoint = self._marker.read_text(encoding="utf-8").strip()
                    if endpoint:
                        self.endpoint = endpoint
                        break
                if time.monotonic() >= deadline:
                    raise RuntimeError(f"{self.fixture}: agent endpoint timed out; see {self.log}")
                time.sleep(.05)
            self.control("ping")
            self.focus()
            self._entered = True
            return self
        except BaseException as error:
            self._close_preserving(error)
            raise

    def require_running(self):
        if self.process is None or self.process.poll() is not None:
            raise RuntimeError(f"{self.fixture}: native fixture exited; see {self.log}")

    def control(self, command, timeout=20, **fields):
        self.require_running()
        if not self.endpoint:
            raise RuntimeError(f"{self.fixture}: agent endpoint unavailable")
        return request(self.endpoint, command, timeout, **fields)

    def focus(self):
        self.control("focus-window")

    def receipts(self, prefix):
        return [json.loads(line.split(prefix, 1)[1])
                for line in self.log.read_text(encoding="utf-8", errors="replace").splitlines(keepends=True)
                if line.endswith("\n") and prefix in line]

    def capture(self, path):
        path = Path(path).resolve()
        # Prevent a stale PNG from turning a false-positive agent response into
        # capture evidence. The caller owns this explicitly named output file.
        path.unlink(missing_ok=True)
        response = self.control("screenshot", timeout=25, path=str(path))
        with path.open("rb") as source:
            header = source.read(24)
        if (len(header) != 24 or header[:8] != b"\x89PNG\r\n\x1a\n"
                or header[12:16] != b"IHDR" or min(struct.unpack(">II", header[16:24])) == 0):
            raise RuntimeError(f"{self.fixture}: screenshot is not a valid PNG header: {path}")
        return response

    def _cleanup_error(self, stage, error, attempts=1):
        self.cleanup_errors.append({
            "stage": stage, "type": type(error).__name__, "message": str(error),
            "path": str(self._temporary_path) if stage == "temporary" else None,
            "attempts": attempts,
        })

    def _close_preserving(self, primary):
        try:
            self.close()
        except BaseException as cleanup:
            if primary is None:
                raise
            # A cleanup failure must not replace the exception that caused exit.
            # The structured record is available even on Python without notes.
            if hasattr(primary, "add_note"):
                primary.add_note(f"Native fixture cleanup also failed: {type(cleanup).__name__}: {cleanup}")

    def _verify_temporary(self):
        path = self._temporary_path
        if (self._temporary is not self._temporary_owner or path is None
                or not path.is_absolute() or not path.name.startswith("sp-fixture-")
                or self._temporary.name != self._temporary_name
                or Path(self._temporary.name).resolve() != path):
            raise RuntimeError("Refusing cleanup of an unverified fixture temporary directory")
        try:
            info = path.stat()
        except FileNotFoundError:
            return path  # Already gone: stdlib cleanup is an idempotent no-op.
        if (info.st_dev, info.st_ino) != self._temporary_identity:
            raise RuntimeError("Refusing cleanup of a replaced fixture temporary directory")
        return path

    def close(self):
        try:
            if self.process is not None and self.process.poll() is None:
                self.process.terminate()
                try:
                    self.process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    self.process.kill()
                    self.process.wait(timeout=5)
            if self._after_exit is not None and self._entered and not self._after_exit_attempted:
                if self.process is None or self.process.poll() is None:
                    raise RuntimeError("Fixture process closure is not confirmed")
        except BaseException as error:
            self._cleanup_error("process", error)
            raise
        try:
            if self._output is not None:
                self._output.close()
                self._output = None
        except BaseException as error:
            self._cleanup_error("output", error)
            raise
        audit_error = None
        try:
            if self._after_exit is not None and self._entered and not self._after_exit_attempted:
                self._after_exit_attempted = True  # Never replay a failed audit on close retry.
                # Captured child TMP/TEMP/TMPDIR, verified against its original
                # owner and file identity. Logs are closed and storage still exists.
                self._after_exit(self._verify_temporary())
        except BaseException as error:
            audit_error = error
            self._cleanup_error("after_exit", error)
            raise
        finally:
            try:
                self._cleanup_temporary()
            except BaseException as error:
                if audit_error is None:
                    raise
                if hasattr(audit_error, "add_note"):
                    audit_error.add_note(f"Temporary cleanup also failed: {type(error).__name__}: {error}")

    def _cleanup_temporary(self):
        if self._temporary is not None:
            # WebView2 descendants can release/recreate scratch files shortly
            # after the owned GUI exits. Retry only this verified directory,
            # including Windows directory-not-empty (145), for at most 3 seconds.
            deadline = time.monotonic() + 3
            attempts = 0
            while True:
                attempts += 1
                try:
                    self._verify_temporary()
                    self._temporary.cleanup()
                    break
                except BaseException as error:
                    retryable = (isinstance(error, OSError) and sys.platform == "win32"
                                 and getattr(error, "winerror", None) in (5, 32, 33, 145))
                    if not retryable:
                        self._cleanup_error("temporary", error, attempts)
                        raise
                    if time.monotonic() >= deadline:
                        self._cleanup_error("temporary", error, attempts)
                        warnings.warn(f"fixture exited but temporary files remain locked at "
                                      f"{self._temporary_path}: {error}", RuntimeWarning)
                        break
                    time.sleep(.1)
            self._temporary = None
            self._temporary_owner = None

    def __exit__(self, exception_type, exception, traceback):
        self._close_preserving(exception)
