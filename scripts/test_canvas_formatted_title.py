"""Portable adversarial proof checks, no native application or network."""
import base64
import copy
from pathlib import Path
import runpy
import unittest
import canvas_formatted_title as C


def varint(n):
    data = bytearray()
    while n >= 128:
        data.append((n & 127) | 128); n >>= 7
    data.append(n)
    return bytes(data)


def f(tag, value):
    if isinstance(value, str): value = value.encode()
    return (varint(tag * 8 + 2) + varint(len(value)) + value if isinstance(value, bytes)
            else varint(tag * 8) + varint(value))


def b64(raw):
    return base64.b64encode(raw).decode()


def body(value):
    return f(1, f(1, value) + f(2, "en"))


def source(changed=False, stale=False):
    rows = []
    for identity in C.IDS:
        revision = 3000 if stale and identity == "title" else (2000 if changed and identity == "paragraph" else 1000)
        raw = f(1, identity) + f(2, revision) + f(6, "thread") + f(7, "document")
        if identity == "title":
            raw += f(9, 64) + f(10, 48) + f(12, f(58, f(1, C.STALE_TITLE if stale else C.TITLE)))
        elif identity == "parent": raw += f(10, 7)
        else:
            value = {"paragraph": C.DESIRED if changed else C.INITIAL,
                     "unsupported": "<u data-unsupported='true'>This unsupported formatting stays readable.</u>",
                     "item": "An untouched checklist item"}[identity]
            raw += f(12, body(value))
            if identity == "item": raw += f(13, f(1, "parent") + f(4, 7)) + f(16, f(2, 0))
            if changed and identity == "paragraph": raw += f(38, f(3, 2000))
        rows.append({"id": identity, "sequence": revision, "raw": b64(raw)})
    return {"file": "Ffixture", "thread": "thread", "document": "document",
            "threadRaw": b64(f(1, "thread") + f(2, 1000) + f(10, "document")),
            "documentRaw": b64(f(1, "document") + f(2, 1000) + f(6, "thread") + f(69, f(1, "Fixture title"))),
            "sections": rows}


def native(rtml, generation, editable=True, error=None):
    blocks = [{"type": "rich_text", "elements": [{"type": "rich_text_section", "elements": [
        {"type": "text", "text": c, "style": {"bold": bold, "italic": italic}}
        for c, bold, italic in C.styled(rtml)]}]}]
    return {"generation": str(generation), "visit": "1", "text": C.plain(rtml), "blocks": blocks,
            "editable": editable, "saveDisabled": not editable, "cancelDisabled": not editable,
            "error": error, "visible": True, "rect": [20, 40, 300, 50]}


def sample():
    original_source, future_source, stale_source = source(), source(True), source(True, True)
    original = {"attempts": 0, "pending": False, "preflights": [], "writes": [], "acknowledgements": [],
                "readbacks": [], "fresh": original_source, "visible": original_source,
                "visit": 1, "read": 2, "stale": False, "loading": False, "operation": None}
    rich = f(1, "rich_text") + f(2, f(1, "rich_text_section") + f(2, f(1, "text") + f(2, C.LABEL)))
    row = (f(1, "paragraph") + f(2, 1000) + f(9, 0) + f(12, body(C.DESIRED)) + f(13, b"")
           + f(19, 1) + f(20, 1) + f(33, "original-session-0") + f(38, f(3, 1000)))
    write = {"section": "paragraph", "session": "original-session", "sequence": 1, "baseSequence": 1000,
             "formTitle": C.LABEL, "documentData": b64(f(1, row) + f(14, rich)),
             "desired": C.DESIRED, "httpFactoryExecuted": False}
    ack_row = base64.b64decode(future_source["sections"][1]["raw"])
    ack = {"raw": b64(f(1, 1) + f(2, f(15, "document") + f(1, ack_row))), "authoredContent": C.DESIRED}
    preflights = [{"visit": 1, "source": original_source, "fresh": original_source, "outcome": "prepared"},
                  {"visit": 1, "source": future_source, "fresh": stale_source, "outcome": "Changed"}]
    result = {"httpFactoryExecuted": False, "original": original, "initialNative": native(C.INITIAL, 3),
              "duplicate": {"ok": True, "clicks": 2, "id": "canvas-text-save"}, "stages": []}
    for n, name in enumerate(C.STAGES):
        data = {**copy.deepcopy(original), "attempts": int(n >= 2), "pending": n == 2,
                "preflights": preflights[:(0, 0, 1, 1, 2, 2)[n]], "writes": [write] if n >= 2 else [],
                "acknowledgements": [ack] if n >= 3 else [], "readbacks": [future_source] if n >= 3 else [],
                "fresh": stale_source if n >= 4 else future_source if n >= 3 else original_source,
                "visible": future_source if n >= 3 else original_source, "read": 5 if n >= 3 else 2,
                "stale": n == 2, "operation": {"serial": 4, "stage": "Saving"} if n == 2 else None}
        editor = native(C.REFUSED if n == 4 else C.DESIRED, {0: 3, 2: 4, 4: 6}.get(n, 0), n != 2, C.ERROR if n == 4 else None) if n in (0, 2, 4) else None
        geometry = {"ok": True, "noOverflow": True, "viewport": [668, 380], "readerScroll": 0,
                    "title": C.LABEL, "titleVisible": True, "paragraph": C.plain(C.DESIRED if n >= 3 else C.INITIAL),
                    "native": editor}
        result["stages"].append({"ok": True, "stage": name, "ordinal": n, "noSession": True, "noTransport": True,
                                 "fixture": data, "geometry": geometry, "editorLayout": {"ok": True} if editor else None})
    result["beforeRefusal"] = {"native": native(C.REFUSED, 6)}
    result["afterRefusal"] = {"native": result["stages"][4]["geometry"]["native"]}
    result["fixture"] = result["stages"][-1]["fixture"]
    result["layout"] = result["stages"][-1]["geometry"]
    return copy.deepcopy(result)


class FormattedTitleTests(unittest.TestCase):
    def reject(self, mutate):
        value = sample(); mutate(value)
        with self.assertRaises(RuntimeError): C.validate(value)

    def test_complete_positive(self): C.validate(sample())
    def test_native_zero_visit_is_a_valid_original_identity(self):
        value = native(C.DESIRED, 1)
        value["visit"] = "0"
        C.native(value, C.DESIRED, True)
    def test_original_formatted_source_required(self):
        self.reject(lambda r: r["original"]["fresh"]["sections"][0].update(raw=source(stale=True)["sections"][0]["raw"]))
    def test_http_factory_claim_refused(self): self.reject(lambda r: r.update(httpFactoryExecuted=True))
    def test_wrong_form_title_refused(self): self.reject(lambda r: r["stages"][2]["fixture"]["writes"][0].update(formTitle=C.TITLE))
    def test_duplicate_attempt_refused(self): self.reject(lambda r: r["stages"][2]["fixture"].update(attempts=2))
    def test_cancel_write_refused(self): self.reject(lambda r: r["stages"][1]["fixture"].update(attempts=1))
    def test_fresh_title_does_not_rewrite_visible(self): self.reject(lambda r: r["stages"][4]["fixture"].update(visible=source(True, True)))
    def test_original_preflight_refused(self): self.reject(lambda r: r["stages"][2]["fixture"]["preflights"][0].update(source=source(True)))
    def test_stale_preflight_must_execute(self): self.reject(lambda r: r["stages"][4]["fixture"]["preflights"][1].update(outcome="owner-unavailable"))
    def test_refused_draft_owner_changed(self): self.reject(lambda r: r["afterRefusal"]["native"].update(generation="99"))
    def test_refused_draft_text_lost(self): self.reject(lambda r: r["stages"][4]["geometry"]["native"].update(text=""))
    def test_refused_cancel_disabled(self): self.reject(lambda r: r["stages"][4]["geometry"]["native"].update(cancelDisabled=True))
    def test_pending_editor_enabled(self): self.reject(lambda r: r["stages"][2]["geometry"]["native"].update(editable=True))
    def test_canonical_future_must_be_authored(self): self.reject(lambda r: r["stages"][3]["fixture"].update(fresh=source()))
    def test_reordered_sections_refused(self): self.reject(lambda r: r["stages"][3]["fixture"]["fresh"]["sections"].reverse())
    def test_unrelated_document_bytes_refused(self): self.reject(lambda r: r["stages"][3]["fixture"]["fresh"].update(documentRaw=b64(f(1, "document") + f(6, "thread") + f(69, f(1, "Foreign")))) )
    def test_mutation_scope_refused(self): self.reject(lambda r: r["stages"][2]["fixture"]["writes"][0].update(section="title"))
    def test_capture_layout_refused(self): self.reject(lambda r: r["stages"][0]["editorLayout"].update(ok=False))
    def test_rendered_title_decoded(self): self.reject(lambda r: r["stages"][3]["geometry"].update(title=C.TITLE))
    def test_terminal_does_not_keep_editor(self): self.reject(lambda r: r["stages"][5]["geometry"].update(native=native(C.REFUSED, 6)))
    def test_title_rich_no_styles(self):
        raw = f(1, b"row") + f(14, f(1, "rich_text") + f(2, f(1, "rich_text_section") + f(2, f(1, "text") + f(2, C.LABEL) + f(3, f(1, 1)))))
        with self.assertRaises(RuntimeError): C.rich_title(C.wire(raw))
    def test_title_double_decoding_refused(self):
        raw = f(1, b"row") + f(14, f(1, "rich_text") + f(2, f(1, "rich_text_section") + f(2, f(1, "text") + f(2, "Plan &amp; Review"))))
        with self.assertRaises(RuntimeError): C.rich_title(C.wire(raw))
    def test_duplicate_rich_leaf_refused(self):
        leaf = f(2, f(1, "text") + f(2, C.LABEL))
        raw = f(1, b"row") + f(14, f(1, "rich_text") + f(2, f(1, "rich_text_section") + leaf + leaf))
        with self.assertRaises(RuntimeError): C.rich_title(C.wire(raw))
    def test_terminal_proofs_bind_original_captured_stages(self):
        wrapper = runpy.run_path(str(Path(__file__).with_name("check-canvas-formatted-title-edit-native.py")))
        check = wrapper["validate_closed"]
        check.__globals__["_base_closed"] = lambda *args: None
        receipt = sample()
        captures = [{**s, "screenshot": f"{n}.png"} for n, s in enumerate(receipt["stages"])]
        check(None, None, None, captures, receipt)
        captures[2] = {**captures[2], "fixture": {"attempts": 77}}
        with self.assertRaises(RuntimeError): check(None, None, None, captures, receipt)


if __name__ == "__main__": unittest.main()
