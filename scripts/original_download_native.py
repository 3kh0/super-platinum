"""Bounded local server and independent original-download evidence oracle."""
import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import math
from pathlib import Path
import re
import threading
from native_check import expected_viewport
from toast_geometry import validate_toast

BYTES = 65 * 1024 * 1024 + 7
CHUNK = b"Z" * 65536
STAGES = ("initial", "busy", "canceled", "retry-busy", "saved", "close-busy", "closed")
FIXTURES = ("original-download", "original-download-narrow")


def require(ok, message):
    if not ok:
        raise RuntimeError(message)


def expected_digest():
    digest = hashlib.sha256()
    remaining = BYTES
    while remaining:
        block = CHUNK[:min(len(CHUNK), remaining)]
        digest.update(block)
        remaining -= len(block)
    return digest.hexdigest()


def validate_stage(record, index, width=None, fixture="original-download"):
    require(isinstance(record, dict) and 0 <= index < len(STAGES), "Stage type/index")
    stage = STAGES[index]
    dom = record.get("dom")
    require(isinstance(dom, dict) and dom.get("stage") == stage, "Exact stage order")
    require(fixture in FIXTURES and dom.get("viewport") == expected_viewport(fixture), "Exact normal/narrow viewport")
    require(record.get("noSession") is True and record.get("noTransport") is True, "Disconnected shell")
    issued = (0, 1, 1, 2, 2, 3, 3)[index]
    outcomes = ([], [], [[1,"canceled"]], [[1,"canceled"]], [[1,"canceled"],[2,"saved"]],
                [[1,"canceled"],[2,"saved"]], [[1,"canceled"],[2,"saved"],[3,"canceled"]])[index]
    require(isinstance(record.get("outcomes"), list)
            and all(isinstance(row, list) and len(row) == 2 and type(row[0]) is int and isinstance(row[1], str)
                    for row in record["outcomes"]), "Typed outcomes")
    require(type(record.get("issued")) is int and record["issued"] == issued and record.get("outcomes") == outcomes,
            "Exact issued/results lifecycle")
    busy = stage in ("busy", "retry-busy", "close-busy")
    closed = stage == "closed"
    require(dom.get("closed") is closed, "Closed DOM state")
    control = dom.get("control")
    if closed:
        require(control is None, "Closed viewer control retained")
    else:
        label = "Cancel" if busy else "Download"
        require(isinstance(control, dict) and control.get("label") == label
                and control.get("aria") == ("Cancel download original-fixture.bin" if busy else "Download original-fixture.bin")
                and control.get("disabled") is False and control.get("inside") is True and control.get("hit") is True,
                "Exact enabled visible action")
        for key in ("x", "y", "width", "height"):
            require(type(control.get(key)) in (int, float) and math.isfinite(control[key]), "Finite geometry")
        viewport = dom.get("viewport")
        require(isinstance(viewport, list) and len(viewport) == 2
                and all(type(n) in (int,float) and math.isfinite(n) and n > 0 for n in viewport), "Viewport")
        require(control["width"] >= 96 and control["height"] > 0 and control["x"] >= 0 and control["y"] >= 0
                and control["x"] + control["width"] <= viewport[0]
                and control["y"] + control["height"] <= viewport[1], "Control bounds")
        if width is not None:
            require(abs(control["width"] - width) <= 1, "Action width shifted")
        width = control["width"] if width is None else width
    files = record.get("files")
    require(isinstance(files, list), "Destination inventory")
    final = [["attempt-2.bin", BYTES]] if index >= 4 else []
    if busy:
        parts = [row for row in files if isinstance(row, list) and len(row) == 2
                 and isinstance(row[0], str) and re.fullmatch(r"\.super-platinum-download-[0-9a-f-]{36}\.part", row[0])
                 and type(row[1]) is int and 65536 <= row[1] <= BYTES]
        require(len(parts) == 1 and sorted(files) == sorted(final + parts), "Actual partial write inventory")
    else:
        require(files == final, "Canceled/complete destination inventory")
    if stage == "saved":
        require(record.get("toast") == "Saved original-fixture.bin", "Actual success publication")
        validate_toast(dom.get("toast"), dom["viewport"], record["toast"])
    if stage in ("canceled", "closed"):
        require(not str(record.get("toast") or "").startswith("Saved "), "Stale success after cancellation")
    return width


def validate_terminal(value, stages, owner):
    require(isinstance(value, dict) and value.get("ok") is True, "Native terminal failure")
    for key in ("fixture", "nonce", "pid"):
        require(value.get(key) == owner[key], "Terminal owner mismatch")
    require(type(value.get("pid")) is int, "PID type")
    require(value.get("stages") == stages and len(stages) == len(STAGES), "Full exact stage ledger")
    width = None
    for index, stage in enumerate(stages):
        width = validate_stage(stage, index, width, owner["fixture"])
    require(all(value.get(key) is True for key in ("actualHandlers", "syntheticAdmissionTransport", "noSession", "noTransport"))
            and value.get("osSaveDialog") is False and value.get("bytes") == BYTES
            and value.get("outcomes") == [[1,"canceled"],[2,"saved"],[3,"canceled"]], "Terminal scope/lifecycle")


def validate_files(directory):
    files = sorted(Path(directory).iterdir())
    require(len(files) == 1 and files[0].name == "attempt-2.bin" and not files[0].is_symlink(), "Final exact file inventory")
    path = files[0]
    require(path.stat().st_size == BYTES, "Full original length")
    with path.open("rb") as stream:
        digest = hashlib.file_digest(stream, "sha256").hexdigest()
    require(digest == expected_digest(), "Original byte digest")
    return {"name": path.name, "bytes": BYTES, "sha256": digest}


def validate_closed_files(directory, before_exit):
    after_exit = validate_files(directory)
    require(after_exit == before_exit, "Destination changed across process exit")
    return after_exit


class LocalOriginal:
    def __init__(self, nonce):
        require(re.fullmatch("[0-9a-f]{32}", nonce), "Nonce")
        self.nonce, self.rows, self.lock = nonce, [], threading.Lock()
        self.gates = [threading.Event() for _ in range(3)]
        owner = self

        class Handler(BaseHTTPRequestHandler):
            def setup(self):
                super().setup()
                self.connection.settimeout(5)  # Bound headers before do_GET.

            def log_message(self, *_):
                pass

            def parse_request(self):
                parsed = super().parse_request()
                if parsed and self.command != "GET":
                    # Cover every parsed method, including unknown methods for
                    # which BaseHTTPRequestHandler otherwise emits an unlogged 501.
                    with owner.lock:
                        if len(owner.rows) < 4:
                            owner.rows.append({"error": "unexpected method", "method": self.command[:64]})
                    self.send_error(405)
                    return False
                return parsed

            def do_GET(self):
                self.connection.settimeout(30)
                with owner.lock:
                    index = len(owner.rows)
                    row = {"index": index + 1, "method": "GET", "path": self.path,
                           "cookieAbsent": self.headers.get("Cookie") is None,
                           "authorizationAbsent": self.headers.get("Authorization") is None,
                           "encoding": self.headers.get("Accept-Encoding"), "bytesSent": 0, "closed": False}
                    if index >= 3:
                        if len(owner.rows) == 3: owner.rows.append({"error": "extra request"})
                        self.send_error(400)
                        return
                    owner.rows.append(row)
                try:
                    require(self.path == "/original/" + owner.nonce and row["cookieAbsent"]
                            and row["authorizationAbsent"] and row["encoding"] == "identity", "Unexpected original request")
                    self.send_response(200)
                    self.send_header("Content-Length", str(BYTES))
                    self.send_header("Content-Type", "application/octet-stream")
                    self.end_headers()
                    self.wfile.write(CHUNK)
                    self.wfile.flush()
                    row["bytesSent"] = len(CHUNK)
                    require(owner.gates[index].wait(45), "Held local body expired")
                    if index == 1:
                        remaining = BYTES - len(CHUNK)
                        while remaining:
                            block = CHUNK[:min(len(CHUNK), remaining)]
                            self.wfile.write(block)
                            row["bytesSent"] += len(block)
                            remaining -= len(block)
                        self.wfile.flush()
                except Exception as error:
                    row["error"] = type(error).__name__ + ": " + str(error)
                finally:
                    row["closed"] = True
                    self.close_connection = True

        self.server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        self.server.daemon_threads = False
        try:
            self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
            self.thread.start()
        except BaseException as primary:
            try:
                self.server.server_close()
            except BaseException as cleanup:
                primary.add_note("Server construction cleanup also failed: " + type(cleanup).__name__ + ": " + str(cleanup))
            raise

    @property
    def port(self):
        return self.server.server_address[1]

    def release(self, index):
        self.gates[index].set()

    def close(self):
        for gate in self.gates:
            gate.set()
        first = None
        def attempt(action):
            nonlocal first
            try:
                action()
            except BaseException as error:
                if first is None:
                    first = error
                else:
                    first.add_note("Additional local teardown failure: " + type(error).__name__ + ": " + str(error))
        attempt(self.server.shutdown)
        attempt(self.server.server_close)
        attempt(lambda: self.thread.join(timeout=5))
        attempt(lambda: require(not self.thread.is_alive(), "Local server teardown"))
        if first is not None:
            raise first

    def validate(self):
        require(len(self.rows) == 3, "Exactly three original GETs")
        for index, row in enumerate(self.rows):
            require(row == {"index": index + 1, "method": "GET", "path": "/original/" + self.nonce,
                            "cookieAbsent": True, "authorizationAbsent": True, "encoding": "identity",
                            "bytesSent": BYTES if index == 1 else 65536, "closed": True}, "Exact local HTTP ledger")
        return list(self.rows)
