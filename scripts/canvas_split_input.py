"""Two non-replayed Enter actions confined to an owned offline WebView profile."""
import json
from pathlib import Path
import re
import secrets
import socket
import subprocess
import sys

from native_fixture import ROOT
from canvas_evidence import closed_receipts


CORPORA = {"ascii": ("Keep thisSecond paragraph", 9),
           "unicode": ("Café & 日本😀Second paragraph", 11)}


def validate_request(value, fixture, nonce, ordinal, corpus="ascii", mode="middle", typing="none"):
    if (corpus not in CORPORA or mode not in ("middle", "end", "reuse", "start")
            or typing not in ("none", "correction", "erase", "restore") or (typing == "restore" and mode not in ("middle", "start"))):
        raise RuntimeError("Unknown Canvas Enter corpus; no input sent")
    seed, offset = CORPORA[corpus]
    if mode == "middle" and typing != "none" and corpus == "unicode":
        seed = seed.removesuffix("Second paragraph") + "Second \u65e5\u672c\U0001f600 & paragraph"
    if mode == "start":
        offset = 0
    if mode in ("end", "reuse"):
        seed = seed.removesuffix("Second paragraph")
    if (value.get("ok") is not True or value.get("fixture") != fixture
            or value.get("corpus", "ascii") != corpus
            or value.get("mode", "middle") != mode
            or value.get("typing", "none") != typing
            or value.get("nonce") != nonce or type(value.get("ordinal")) is not int
            or value["ordinal"] != ordinal or ordinal not in (0, 1)
            or value.get("fieldId") != "canvas-text-draft"
            or value.get("seed") != seed
            or type(value.get("offset")) is not int or value["offset"] != offset
            or type(value.get("revision")) is not int or value["revision"] != 0
            or any(not isinstance(value.get(k), str) or not value[k].isascii()
                   or not value[k].isdigit() for k in ("generation", "visit"))):
        raise RuntimeError("Invalid owned Canvas Enter request; no input sent")


class CanvasSplitInput:
    @classmethod
    def prepare(cls, directory, fixture, checks):
        if (sys.platform != "win32" or fixture not in ("canvas-structure", "canvas-structure-narrow")
                or checks.get("SUPER_PLATINUM_CANVAS_SPLIT_CHECK") != "1"):
            raise RuntimeError("Canvas Enter harness requires the owned Windows structure fixture")
        result = cls()
        result.corpus = checks.get("SUPER_PLATINUM_CANVAS_SPLIT_CORPUS", "ascii")
        result.mode = checks.get("SUPER_PLATINUM_CANVAS_SPLIT_MODE", "middle")
        result.typing = checks.get("SUPER_PLATINUM_CANVAS_SPLIT_TYPING", "none")
        if (result.corpus not in CORPORA or result.mode not in ("middle", "end", "reuse", "start")
                or result.typing not in ("none", "correction", "erase", "restore")
                or (result.typing == "restore" and result.mode not in ("middle", "start"))):
            raise RuntimeError("Unknown Canvas Enter corpus")
        result.fixture, result.nonce = fixture, secrets.token_hex(16)
        result.requests, result.receipts = [], []
        result.port = None
        checks.update(SUPER_PLATINUM_CANVAS_SPLIT_NONCE=result.nonce,
                      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS="--remote-debugging-address=127.0.0.1 --remote-debugging-port=0")
        result.evidence = {"kind": "owned WebView CDP Enter", "osInput": False,
                           "nonce": result.nonce, "corpus": result.corpus, "mode": result.mode,
                           "typing": result.typing, "actions": result.receipts}
        return result

    def bind(self, native):
        self.native = native

    def service(self, native):
        requests = native.receipts("canvas-split-enter: ")
        if requests[:len(self.requests)] != self.requests:
            raise RuntimeError("Canvas Enter request ledger changed")
        for value in requests[len(self.requests):]:
            validate_request(value, self.fixture, self.nonce, len(self.requests), self.corpus, self.mode, self.typing)
            native.require_running()
            temporary = native._verify_temporary()
            profiles = list(temporary.glob(f"super-platinum-fixture-webview-{native.process.pid}-*"))
            if len(profiles) != 1 or not profiles[0].resolve().is_relative_to(temporary):
                raise RuntimeError("Expected one private owned WebView profile")
            paths = list(profiles[0].rglob("DevToolsActivePort"))
            if (len(paths) != 1 or not paths[0].is_file()
                    or not paths[0].resolve().is_relative_to(profiles[0].resolve())):
                raise RuntimeError("Private WebView debugger marker unavailable")
            lines = paths[0].read_text(encoding="utf-8").splitlines()
            if (len(lines) != 2 or not lines[0].isdigit() or not 0 < int(lines[0]) < 65536
                    or not re.fullmatch(r"/devtools/browser/[a-zA-Z0-9-]+", lines[1])):
                raise RuntimeError("Invalid private debugger marker")
            port = int(lines[0])
            if self.port is not None and port != self.port:
                raise RuntimeError("Private debugger identity changed")
            self.port = port
            folder = native.log.parent
            output = folder / f"enter-{value['ordinal']}.json"
            config = folder / f"enter-{value['ordinal']}-config.json"
            with config.open("x", encoding="utf-8") as stream:
                json.dump(dict(port=port, browserPath=lines[1], corpus=self.corpus, mode=self.mode,
                               typing=self.typing, expected=value, output=str(output)), stream)
            # Mark before dispatch. A timeout/error is never retried or replayed.
            self.requests.append(value)
            command = ["node", str(ROOT / "scripts/canvas_split_input.mjs"), str(config)]
            process = subprocess.run(command, cwd=ROOT, capture_output=True, text=True,
                                     timeout=25, creationflags=subprocess.CREATE_NO_WINDOW)
            receipt = json.loads(output.read_text(encoding="utf-8")) if output.exists() else {"ok": False}
            self.receipts.append(receipt)
            if process.returncode or receipt.get("ok") is not True or receipt.get("expected") != value:
                raise RuntimeError("Owned Canvas Enter failed; no replay: " + str(receipt.get("error", process.stderr)))

    def finish(self, native):
        self.service(native)
        if len(self.receipts) != 2 or any(not r.get("ok") for r in self.receipts):
            raise RuntimeError("Expected two completed native Enter actions")

    def closed(self, native):
        if native is not None and closed_receipts(native.log, "canvas-split-enter: ") != self.requests:
            raise RuntimeError("Closed Canvas Enter request ledger differs")
        if self.port is not None:
            with socket.socket() as stream:
                stream.settimeout(.5)
                closed = stream.connect_ex(("127.0.0.1", self.port)) != 0
            self.evidence["debuggerPortClosed"] = closed
            if not closed:
                raise RuntimeError("Owned WebView debugger remains open after fixture exit")
