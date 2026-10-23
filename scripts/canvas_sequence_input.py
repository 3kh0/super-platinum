"""Two non-replayed sequence visits confined to one owned offline WebView profile."""
import json
from pathlib import Path
import re
import secrets
import socket
import subprocess
import sys

from native_fixture import ROOT
from canvas_evidence import closed_receipts


HISTORIES = ("bootstrap", "retained-positive", "retained-zero")
LAYOUTS = ("no-following", "text-following", "empty-final-following", "empty-nonfinal-following")

CADENCES = ("straight", "correction-before-enter", "erase-before-enter", "revert-before-enter", "trailing-empty")
def validate_request(value, fixture, nonce, ordinal, history="bootstrap", layout="no-following", corpus="ascii", count=2, cadence="straight"):
    if (history not in HISTORIES or layout not in LAYOUTS or corpus not in ("ascii","unicode") or type(count) is not int or not 2 <= count <= 16 or cadence not in CADENCES
        or value.get("ok") is not True or value.get("fixture") != fixture or value.get("corpus") != corpus or value.get("mode") != "sequence" or value.get("typing") != "none"
        or value.get("sequenceHistory") != history or value.get("sequenceLayout") != layout or type(value.get("sequenceCount")) is not int or value["sequenceCount"] != count or value.get("sequenceCadence") != cadence
        or value.get("nonce") != nonce or type(value.get("ordinal")) is not int or value["ordinal"] != ordinal or ordinal not in (0,1)
        or value.get("fieldId") != "canvas-text-draft" or value.get("seed") != ("Keep this" if corpus=="ascii" else "Caf\u00e9 & \u65e5\u672c\U0001f600")
        or type(value.get("offset")) is not int or value["offset"] != (9 if corpus=="ascii" else 11) or type(value.get("revision")) is not int or value["revision"] != 0
        or any(not isinstance(value.get(k),str) or not value[k].isascii() or not value[k].isdigit() for k in ("generation","visit"))):
        raise RuntimeError("Invalid owned Canvas sequence request; no input sent")


class CanvasSequenceInput:
    @classmethod
    def prepare(cls, directory, fixture, checks):
        if (sys.platform != "win32" or fixture not in ("canvas-structure", "canvas-structure-narrow")
                or checks.get("SUPER_PLATINUM_CANVAS_SPLIT_CHECK") != "1"):
            raise RuntimeError("Canvas Enter harness requires the owned Windows structure fixture")
        result = cls()
        result.history = checks.get("SUPER_PLATINUM_CANVAS_SEQUENCE_HISTORY", "bootstrap")
        result.layout = checks.get("SUPER_PLATINUM_CANVAS_SEQUENCE_LAYOUT", "no-following")
        result.corpus = checks.get("SUPER_PLATINUM_CANVAS_SPLIT_CORPUS", "ascii")
        result.typing = checks.get("SUPER_PLATINUM_CANVAS_SPLIT_TYPING", "none")
        raw_count = checks.get("SUPER_PLATINUM_CANVAS_SEQUENCE_COUNT", "2")
        result.count = int(raw_count) if raw_count in tuple(map(str, range(2,17))) else 0
        result.cadence = checks.get("SUPER_PLATINUM_CANVAS_SEQUENCE_CADENCE", "straight")
        if (result.history not in HISTORIES or result.layout not in LAYOUTS
                or checks.get("SUPER_PLATINUM_CANVAS_SPLIT_MODE") != "sequence"
                or result.corpus not in ("ascii", "unicode")
                or result.typing != "none" or not 2 <= result.count <= 16 or result.cadence not in CADENCES
):
            raise RuntimeError("Unknown Canvas sequence cohort")
        result.fixture, result.nonce = fixture, secrets.token_hex(16)
        result.requests, result.receipts = [], []
        result.port = None
        checks.update(SUPER_PLATINUM_CANVAS_SPLIT_NONCE=result.nonce,
                      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS="--remote-debugging-address=127.0.0.1 --remote-debugging-port=0")
        result.evidence = {"kind": "owned WebView CDP Enter", "osInput": False,
                           "nonce": result.nonce, "corpus": result.corpus, "mode": "sequence",
                           "typing": result.typing, "sequenceHistory": result.history, "sequenceLayout": result.layout, "sequenceCount":result.count,"sequenceCadence":result.cadence, "actions": result.receipts}
        return result

    def bind(self, native):
        self.native = native

    def service(self, native):
        requests = native.receipts("canvas-split-enter: ")
        if requests[:len(self.requests)] != self.requests:
            raise RuntimeError("Canvas Enter request ledger changed")
        for value in requests[len(self.requests):]:
            validate_request(value, self.fixture, self.nonce, len(self.requests), self.history, self.layout, self.corpus, self.count,self.cadence)
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
                json.dump(dict(port=port, browserPath=lines[1], sequenceHistory=self.history, sequenceLayout=self.layout, corpus=self.corpus, sequenceCount=self.count,sequenceCadence=self.cadence, expected=value, output=str(output)), stream)
            # Mark before dispatch. A timeout/error is never retried or replayed.
            self.requests.append(value)
            command = ["node", str(ROOT / "scripts/canvas_sequence_input.mjs"), str(config)]
            process = subprocess.run(command, cwd=ROOT, capture_output=True, text=True,
                                     timeout=25, creationflags=subprocess.CREATE_NO_WINDOW)
            receipt = json.loads(output.read_text(encoding="utf-8")) if output.exists() else {"ok": False}
            self.receipts.append(receipt)
            if process.returncode or receipt.get("ok") is not True or receipt.get("expected") != value:
                raise RuntimeError("Owned Canvas Enter failed; no replay: " + str(receipt.get("error", process.stderr)))

    def finish(self, native):
        self.service(native)
        if len(self.receipts) != 2 or any(not r.get("ok") for r in self.receipts):
            raise RuntimeError("Expected two completed native sequence visits")

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
