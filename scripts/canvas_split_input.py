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
           "unicode": ("Caf\u00e9 & \u65e5\u672c\U0001f600Second paragraph", 11)}


def validate_request(value, fixture, nonce, ordinal, corpus="ascii", mode="middle", typing="none", pre_enter="none", parent_mode="bootstrap", middle_parent="none", middle_cadence="parent-first", second_middle="none", second_middle_cadence="straight"):
    from canvas_second_middle_wire import selectors
    selectors(mode,typing,pre_enter,middle_parent,middle_cadence,second_middle,second_middle_cadence)
    if second_middle != "none":
        from canvas_second_middle_wire import config
        selected=config(corpus,parent_mode,second_middle,second_middle_cadence)
        if any(value.get(k)!=v for k,v in dict(secondMiddle=second_middle,secondMiddleCadence=second_middle_cadence,parentMode=parent_mode,parentId=selected.parent).items()):
            raise RuntimeError("Invalid second-middle request selector; no input sent")
        seed,offset=selected.full,selected.offset
    elif middle_parent != "none":
        from canvas_middle_parent_wire import config
        selected=config(corpus,typing,parent_mode,pre_enter,middle_parent,middle_cadence)
        if mode != "middle" or any(value.get(k)!=v for k,v in dict(preEnter=pre_enter,parentMode=parent_mode,parentId=selected.parent,middleParent=middle_parent,middleCadence=middle_cadence).items()):
            raise RuntimeError("Invalid middle parent request selector; no input sent")
        seed,offset=selected.full,selected.source_offset
    elif pre_enter != "none":
        from canvas_premiddle_wire import config
        selected = config(corpus, typing, parent_mode, pre_enter)
        if mode != "middle" or value.get("preEnter") != pre_enter or value.get("parentMode") != parent_mode or value.get("parentId") != selected.parent:
            raise RuntimeError("Invalid pre-middle request selector; no input sent")
        seed, offset = selected.full, selected.source_offset
    else:
        seed, offset = CORPORA.get(corpus, (None, None))
    if (corpus not in CORPORA or mode not in ("middle", "end", "reuse", "start")
            or typing not in ("none", "correction", "erase", "restore") or (typing == "restore" and mode not in ("middle", "start"))):
        raise RuntimeError("Unknown Canvas Enter corpus; no input sent")
    if second_middle == "none" and (value.get("secondMiddle", "none") != "none" or value.get("secondMiddleCadence", "straight") != "straight"):
        raise RuntimeError("Unexpected second-middle request; no input sent")
    if middle_parent == "none" and (value.get("middleParent", "none") != "none" or middle_cadence != "parent-first" or value.get("middleCadence", "parent-first") != "parent-first"):
        raise RuntimeError("Unexpected middle parent request; no input sent")
    if pre_enter == "none" and value.get("preEnter", "none") != "none":
        raise RuntimeError("Unexpected pre-middle request; no input sent")
    if middle_parent == "none" and pre_enter == "none" and mode == "middle" and typing != "none" and corpus == "unicode":
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
        result.pre_enter = checks.get("SUPER_PLATINUM_CANVAS_SPLIT_PRE_ENTER", "none")
        result.parent_mode = checks.get("SUPER_PLATINUM_CANVAS_SPLIT_PARENT", "bootstrap")
        if (result.corpus not in CORPORA or result.mode not in ("middle", "end", "reuse", "start")
                or result.typing not in ("none", "correction", "erase", "restore")
                or (result.typing == "restore" and result.mode not in ("middle", "start"))):
            raise RuntimeError("Unknown Canvas Enter corpus")
        if result.pre_enter not in ("none", "correction", "revert", "retype") or (result.pre_enter != "none" and result.mode != "middle"):
            raise RuntimeError("Unknown pre-middle input cohort")
        result.middle_parent=checks.get("SUPER_PLATINUM_CANVAS_SPLIT_MIDDLE_PARENT", "none")
        result.middle_cadence=checks.get("SUPER_PLATINUM_CANVAS_SPLIT_MIDDLE_CADENCE", "parent-first")
        if (result.middle_parent not in ("none", "correction", "erase", "revert") or result.middle_cadence not in ("parent-first", "child-first", "interleaved")
                or (result.middle_parent != "none" and result.mode != "middle") or (result.middle_parent == "none" and result.middle_cadence != "parent-first")):
            raise RuntimeError("Unknown middle parent input cohort")
        result.second_middle=checks.get("SUPER_PLATINUM_CANVAS_SECOND_MIDDLE", "none")
        result.second_middle_cadence=checks.get("SUPER_PLATINUM_CANVAS_SECOND_MIDDLE_CADENCE", "straight")
        from canvas_second_middle_wire import selectors
        selectors(result.mode,result.typing,result.pre_enter,result.middle_parent,result.middle_cadence,result.second_middle,result.second_middle_cadence)
        result.fixture, result.nonce = fixture, secrets.token_hex(16)
        result.requests, result.receipts = [], []
        result.port = None
        checks.update(SUPER_PLATINUM_CANVAS_SPLIT_NONCE=result.nonce,
                      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS="--remote-debugging-address=127.0.0.1 --remote-debugging-port=0")
        result.evidence = {"kind": "owned WebView CDP Enter", "osInput": False,
                           "nonce": result.nonce, "corpus": result.corpus, "mode": result.mode,
                           "secondMiddle": result.second_middle,"secondMiddleCadence":result.second_middle_cadence,"typing": result.typing, "preEnter": result.pre_enter, "middleParent": result.middle_parent, "middleCadence": result.middle_cadence, "actions": result.receipts}
        return result

    def bind(self, native):
        self.native = native

    def service(self, native):
        requests = native.receipts("canvas-split-enter: ")
        if requests[:len(self.requests)] != self.requests:
            raise RuntimeError("Canvas Enter request ledger changed")
        for value in requests[len(self.requests):]:
            validate_request(value, self.fixture, self.nonce, len(self.requests), self.corpus, self.mode, self.typing, self.pre_enter, self.parent_mode, self.middle_parent, self.middle_cadence, self.second_middle, self.second_middle_cadence)
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
                               secondMiddle=self.second_middle,secondMiddleCadence=self.second_middle_cadence,typing=self.typing, preEnter=self.pre_enter, parentMode=self.parent_mode, middleParent=self.middle_parent, middleCadence=self.middle_cadence, expected=value, output=str(output)), stream)
            # Mark before dispatch. A timeout/error is never retried or replayed.
            self.requests.append(value)
            command = ["node", str(ROOT / ("scripts/canvas_second_middle_input.mjs" if self.second_middle != "none" else "scripts/canvas_middle_parent_input.mjs" if self.middle_parent != "none" else "scripts/canvas_premiddle_input.mjs" if self.pre_enter != "none" else "scripts/canvas_split_input.mjs")), str(config)]
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
