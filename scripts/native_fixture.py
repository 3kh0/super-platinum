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

    def __init__(self, fixture, log, binary=None, checks=None):
        self.fixture = fixture
        self.log = Path(log).resolve()
        self.binary = Path(binary or os.environ.get("SUPER_PLATINUM_BINARY", DEFAULT_BINARY)).resolve()
        self.checks = checks or {}
        self.process = None
        self.endpoint = None
        self._temporary = None
        self._output = None

    def __enter__(self):
        if not self.binary.is_file():
            raise FileNotFoundError(f"native binary missing: {self.binary}; build it first")
        self.log.parent.mkdir(parents=True, exist_ok=True)
        self._temporary = tempfile.TemporaryDirectory(prefix="sp-fixture-")
        temporary = Path(self._temporary.name)
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
            return self
        except BaseException:
            self.close()
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

    def close(self):
        if self.process is not None and self.process.poll() is None:
            self.process.terminate()
            try:
                self.process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait(timeout=5)
        if self._output is not None:
            self._output.close()
            self._output = None
        if self._temporary is not None:
            # WebView2 descendants can release scratch files shortly after the
            # owned GUI process exits. Retry only cleanup of our own directory;
            # never kill browser processes by name or mask assertion evidence.
            deadline = time.monotonic() + 3
            while True:
                try:
                    self._temporary.cleanup()
                    break
                except OSError as error:
                    if sys.platform != "win32" or error.winerror not in (5, 32, 33):
                        raise
                    if time.monotonic() >= deadline:
                        warnings.warn(f"fixture exited but temporary files remain locked at "
                                      f"{self._temporary.name}: {error}", RuntimeWarning)
                        break
                    time.sleep(.1)
            self._temporary = None

    def __exit__(self, exception_type, exception, traceback):
        self.close()
