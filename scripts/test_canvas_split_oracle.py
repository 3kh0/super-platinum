"""Portable adversarial checks. Fake opaque history bytes are not Automerge proof."""
import base64
import copy
import json
import unittest
from types import SimpleNamespace

import canvas_split_oracle as C


def fixture_corpus(name="ascii"):
    # Independent literal fixtures: never derive expected ACK/native text from a sent body.
    if name == "unicode":
        return SimpleNamespace(seed="Café & 日本😀Second paragraph", left="Café & 日本😀", right="Second paragraph", edited="Café & 日本😀 edited",
                               wire_seed="Café &amp; 日本😀Second paragraph", wire_left="Café &amp; 日本😀", wire_right="Second paragraph", wire_edited="Café &amp; 日本😀 edited", visible_offset=11, wire_offset=15)
    assert name == "ascii"
    return SimpleNamespace(seed="Keep thisSecond paragraph", left="Keep this", right="Second paragraph", edited="Keep this edited",
                           wire_seed="Keep thisSecond paragraph", wire_left="Keep this", wire_right="Second paragraph", wire_edited="Keep this edited", visible_offset=9, wire_offset=9)


def vint(value):
    result = bytearray()
    while value >= 128:
        result.append((value & 127) | 128); value >>= 7
    return bytes(result + bytes([value]))


def f(tag, value):
    if isinstance(value, str): value = value.encode()
    return vint(tag * 8 + 2) + vint(len(value)) + value if isinstance(value, bytes) else vint(tag * 8) + vint(value)


def b64(value): return base64.b64encode(value).decode()
def msg(*fields): return b"".join(fields)
def replace(raw, tag, value): return b"".join(x[3] for x in C.wire(raw) if x[0] != tag) + f(tag, value)
def remove(raw, tag): return b"".join(x[3] for x in C.wire(raw) if x[0] != tag)
def merge(left, right):
    tags = {x[0] for x in C.wire(right)}
    return b"".join(x[3] for x in C.wire(left) if x[0] not in tags) + right


def body(text, language=True): return f(1, f(1, text) + (f(2, "en") if language else b""))
def blocks(*parts):
    return [{"type": "rich_text", "elements": [{"type": "rich_text_section", "elements": [{"type": "text", "text": p}]} for p in parts]}]


SESSION, CHILD = "a" * 12, "temp:C:xyz" + "b" * 25
HEAD, NEXT, CHILD_HEAD, CONT_HEAD = "1" * 64, "2" * 64, "3" * 64, "4" * 64
TITLE = f(1, "rich_text") + f(2, f(1, "rich_text_section") + f(2, f(1, "text") + f(2, "Fixture title")))


def inventory(rows):
    thread = f(1, "thread") + f(2, 1000) + f(10, "document")
    document = f(1, "document") + f(2, 1000) + f(6, "thread") + f(42, "xyz") + f(69, f(1, "Fixture title"))
    return dict(file="Ffixture", thread="thread", document="document", threadRaw=b64(thread), documentRaw=b64(document),
                sections=[dict(id=C.text(C.wire(row), 1), sequence=C.field(C.wire(row), 2, 0), raw=b64(row)) for row in rows])


def source_rows(corpus="ascii"):
    c = fixture_corpus(corpus)
    owner = f(2, 1000) + f(6, "thread") + f(7, "document")
    title = f(1, "title") + owner + f(8, "aaa") + f(9, 64) + f(10, 48) + f(12, f(58, f(1, "Fixture title")))
    parent = f(1, "paragraph") + owner + f(8, "aac") + f(12, body(c.wire_seed))
    neighbor = f(1, "neighbor") + owner + f(8, "aae") + f(12, body("Another paragraph")) + b"\xc8\x3e\x81\x00"
    return [title, parent, neighbor]


def requests(corpus="ascii"):
    c = fixture_corpus(corpus)
    lineage = f(1, 0) + f(2, SESSION + "00000000") + f(3, CHILD) + f(4, c.wire_offset) + f(5, json.dumps([HEAD])) + f(6, "1000-" + HEAD) + f(7, b"pre-document")
    parent_wire = f(1, "paragraph") + f(2, 1000) + f(3, 1000) + f(5, b"parent-document") + f(6, HEAD) + f(6, NEXT) + f(6, HEAD) + f(8, HEAD) + f(10, lineage) + f(12, NEXT)
    child_wire = f(1, CHILD) + f(2, 0) + f(3, 0) + f(5, b"child-document") + f(6, CHILD_HEAD) + f(8, HEAD) + f(12, CHILD_HEAD)
    parent = f(1, "paragraph") + f(2, 1000) + f(9, 0) + f(12, body(c.wire_left)) + f(13, b"") + f(19, 1) + f(20, 1) + f(33, SESSION + "-0") + f(38, parent_wire)
    child = f(1, CHILD) + f(2, 0) + f(3, 0) + f(8, "aad:temp") + f(9, 0) + f(10, 0) + f(11, 0) + f(12, body(c.wire_right, False)) + f(13, b"") + f(16, f(4, 0)) + f(19, 1) + f(20, 1) + f(21, "aad:temp") + f(29, 0) + f(33, SESSION + "-0") + f(35, 0) + f(38, child_wire)
    data = f(1, parent) + f(1, child) + f(14, TITLE)
    write = dict(documentData=b64(data), formTitle="Fixture title", session=SESSION, sequence=1, parent="paragraph", child=CHILD, httpFactoryExecuted=False)
    return write, [parent, child], lineage


def acknowledged(sent, original, continued=False, corpus="ascii"):
    c = fixture_corpus(corpus)
    result = []
    for index, outgoing in enumerate(sent):
        parent = index == 0
        row = merge(original, outgoing) if parent else outgoing
        sequence = 3000 if continued else 2000 if parent else 1000
        crdt = C.field(C.wire(outgoing), 38, 2)
        crdt = replace(replace(remove(crdt, 12), 3, sequence), 4, 999999)
        for tag, value in [(2, sequence), (6, "thread"), (7, "document"), (12, body(c.wire_edited if continued else c.wire_left if parent else c.wire_right, parent)), (38, crdt)]:
            row = replace(row, tag, value)
        if not parent:
            row = replace(replace(row, 8, "aad"), 21, "aad")
        result.append(row)
    return result


def acknowledgement(rows, partial=False, continued=False):
    data = f(15, "document") + b"".join(f(1, row) for row in (rows[:1] if partial else rows))
    return dict(raw=b64(f(1, 1) + f(2, data)), partial=partial, accepted=not partial, **({"kind": "text"} if continued else {}))


def witness(generation, corpus="ascii"):
    c = fixture_corpus(corpus)
    return dict(generation=str(generation), visit="0", offset=c.visible_offset, before=blocks(c.seed), after=blocks(c.left, c.right),
                revision=1, beforeInputTrusted=True, inputTrusted=True, inputType="insertParagraph")


def native(parts, generation, editable=True, error=None):
    return dict(generation=str(generation), visit="0", blocks=blocks(*parts), text="".join(parts), editable=editable,
                saveDisabled=not editable, cancelDisabled=not editable, error=error, visible=True)


def receipt(scenario="success", corpus="ascii"):
    c = fixture_corpus(corpus)
    initial_rows = source_rows(corpus); initial = inventory(initial_rows)
    original = dict(scenario=scenario, pending=False, preflights=[], writes=[], acknowledgements=[], readbacks=[], reservations=0,
                    fresh=initial, visible=initial, visit=0, read=1, stale=False, loading=False, capacityReason=None, operation=None)
    if corpus != "ascii": original["corpus"] = corpus
    empty_journal = dict(scope=C.SCOPE.copy(), receipt=None)
    write, sent, lineage = requests(corpus); rows = acknowledged(sent, initial_rows[1], corpus=corpus); readback = inventory([initial_rows[0], *rows, initial_rows[2]])
    cp = dict(version=3, scope=C.SCOPE.copy(), thread="thread", document="document", target="paragraph", session=SESSION,
              sequence=1, action={"Split": dict(child=CHILD, position="aad:temp", offset_utf16=c.visible_offset, parent_draft=c.wire_left, child_draft=c.wire_right)},
              draft=c.wire_seed, sections=[list(row) for row in initial_rows], request=list(base64.b64decode(write["documentData"])))
    held_journal = dict(scope=C.SCOPE.copy(), receipt=dict(operation="owned-operation", checkpoint=cp))
    continued_wire = replace(replace(remove(C.field(C.wire(rows[0]), 38, 2), 4), 5, b"continued-document"), 12, CONT_HEAD)
    continued_wire += f(6, CONT_HEAD)
    continued = replace(replace(replace(sent[0], 2, 2000), 12, body(c.wire_edited)), 38, continued_wire)
    continued = replace(continued, 33, "c" * 12 + "-0")
    continued = b"".join(x[3] for x in sorted(C.wire(continued), key=lambda x: x[0]))
    text_write = dict(kind="text", parent="paragraph", session="c" * 12, sequence=1, formTitle="Fixture title", httpFactoryExecuted=False,
                      documentData=b64(f(1, continued) + f(14, TITLE)))
    final_parent = acknowledged([continued], rows[0], True, corpus)[0]
    final_read = inventory([initial_rows[0], final_parent, rows[1], initial_rows[2]])
    stages = []
    for i, name in enumerate(C.STAGES[scenario]):
        source = copy.deepcopy(original)
        if i >= 2:
            fresh = copy.deepcopy(initial)
            if scenario == "stale":
                fresh["sections"][1].update(sequence=3000, raw=b64(replace(initial_rows[1], 2, 3000)))
            source["preflights"] = [dict(source=initial, fresh=fresh, outcome="Changed" if scenario == "stale" else "prepared")]
            source["fresh"] = fresh
            if scenario != "stale":
                source.update(writes=[write], reservations=1)
                if i >= 3:
                    source["acknowledgements"] = [acknowledgement(rows, scenario == "partial")]
                    if scenario == "success":
                        source.update(readbacks=[readback], fresh=readback, visible=readback, read=2)
                if i >= 5:
                    source["preflights"].append(dict(source=readback, fresh=readback, outcome="prepared", kind="text"))
                    source["writes"].append(text_write)
                    if i == 6:
                        source["acknowledgements"].append(acknowledgement([final_parent], continued=True))
                        source["readbacks"].append(final_read); source.update(fresh=final_read, visible=final_read, read=3)
        pending, uncertain = name in ("saving", "continued-saving"), name == "uncertain"
        source.update(pending=pending, stale=pending or uncertain, operation=dict(serial=1, stage="Uncertain" if uncertain else "Saving") if pending or uncertain else None)
        parts = [c.left, c.right] if name in ("split", "saving", "refused", "uncertain") else [c.right] if name == "child-editing" else [c.edited] if name == "continued-saving" else None
        geometry = dict(ok=True, noOverflow=True, native=native(parts, 1 if name == "split" else 3 if name == "child-editing" else 4 if name == "continued-saving" else 2,
                                                               not (pending or uncertain), C.ERROR if name == "refused" else None) if parts else None)
        if parts is None:
            geometry["paragraphs"] = [dict(id="paragraph", text=c.edited if i == 6 else c.left if scenario == "success" and i >= 3 else c.seed)]
            if scenario == "success" and i >= 3: geometry["paragraphs"].append(dict(id=CHILD, text=c.right))
            geometry["paragraphs"].append(dict(id="neighbor", text="Another paragraph"))
        stages.append(dict(ok=True, stage=name, ordinal=i, noSession=True, noTransport=True, fixture=copy.deepcopy(source), geometry=geometry,
                           journal=copy.deepcopy(held_journal if name in ("saving", "uncertain") else empty_journal)))
    return dict(scenario=scenario, httpFactoryExecuted=False, trustedNativeEnter=True, original=original, initialJournal=empty_journal,
                cancelledWitness=witness(1, corpus), savedWitness=witness(2, corpus), duplicate=dict(ok=True, clicks=2), stages=stages,
                fixture=stages[-1]["fixture"], layout=stages[-1]["geometry"], finalJournal=stages[-1]["journal"], **({"corpus": corpus} if corpus != "ascii" else {}))


class SplitOracle(unittest.TestCase):
    def reject(self, value):
        with self.assertRaises((RuntimeError, ValueError, TypeError, KeyError)):
            C.validate(value)

    def test_complete_original_success_stale_partial_receipts(self):
        for scenario in C.STAGES:
            with self.subTest(scenario=scenario): C.validate(receipt(scenario))

    def test_synthetic_untrusted_or_shifted_witness_refused(self):
        for key, value in (("beforeInputTrusted", False), ("inputTrusted", False), ("offset", 8), ("revision", 2), ("generation", "1")):
            r = receipt(); r["savedWitness"][key] = value; self.reject(r)

    def test_only_exact_two_native_sections(self):
        for replacement in (blocks(C.SEED), blocks(C.LEFT, C.RIGHT, "extra"), blocks(C.LEFT, "changed")):
            r = receipt(); r["stages"][0]["geometry"]["native"]["blocks"] = replacement; self.reject(r)

    def test_stale_refusal_cannot_dispatch_or_lose_draft(self):
        r = receipt("stale"); r["stages"][2]["fixture"]["writes"] = [requests()[0]]; self.reject(r)
        r = receipt("stale"); r["stages"][2]["geometry"]["native"]["text"] = C.SEED + "changed"; self.reject(r)

    def test_cancel_and_opaque_neighbor_custody(self):
        r = receipt(); r["stages"][1]["fixture"]["read"] = 4; self.reject(r)
        r = receipt(); row = r["stages"][3]["fixture"]["fresh"]["sections"][-1]; row["raw"] = b64(base64.b64decode(row["raw"])[:-4]); self.reject(r)

    def test_missing_duplicate_or_changed_pair_content(self):
        write = requests()[0]
        parsed = C.decoded(write["documentData"]); parent, child = C.messages(parsed, 1)
        for data in (f(1, C.raw(parent)) + f(14, TITLE), f(1, C.raw(parent)) * 2 + f(14, TITLE),
                     f(1, replace(C.raw(parent), 12, body("changed"))) + f(1, C.raw(child)) + f(14, TITLE)):
            with self.assertRaises(RuntimeError): C.split_write(dict(write, documentData=b64(data)))

    def test_lineage_cannot_be_replaced_or_moved_to_child(self):
        write, sent, lineage = requests()
        for tag, value in ((1, 1), (2, "f" * 20), (3, "other"), (4, 8), (6, "wrong"), (7, b"parent-document")):
            changed = replace(lineage, tag, value)
            row = replace(sent[0], 38, replace(C.field(C.wire(sent[0]), 38, 2), 10, changed))
            with self.subTest(tag=tag), self.assertRaises(RuntimeError): C.split_write(dict(write, documentData=b64(f(1, row) + f(1, sent[1]) + f(14, TITLE))))
        child = replace(sent[1], 38, C.field(C.wire(sent[1]), 38, 2) + f(10, lineage))
        with self.assertRaises(RuntimeError): C.split_write(dict(write, documentData=b64(f(1, sent[0]) + f(1, child) + f(14, TITLE))))

    def test_language_absence_is_not_empty_language(self):
        write, sent, _ = requests(); child = replace(sent[1], 12, f(1, f(1, C.RIGHT) + f(2, "")))
        with self.assertRaises(RuntimeError): C.split_write(dict(write, documentData=b64(f(1, sent[0]) + f(1, child) + f(14, TITLE))))

    def test_partial_ack_never_unlocks_or_reads_back(self):
        r = receipt("partial"); r["stages"][-1]["fixture"]["acknowledgements"][0]["accepted"] = True; self.reject(r)
        r = receipt("partial"); r["stages"][-1]["journal"]["receipt"] = None; self.reject(r)
        r = receipt("partial"); r["stages"][-1]["geometry"]["native"]["cancelDisabled"] = False; self.reject(r)
        r = receipt("partial"); row = r["stages"][-1]["fixture"]["fresh"]["sections"][1]; row["raw"] = b64(base64.b64decode(row["raw"]) + f(38, b"unaccepted")); self.reject(r)

    def test_readback_must_equal_original_ack_pair(self):
        r = receipt(); row = r["stages"][3]["fixture"]["readbacks"][0]["sections"][1]; row["raw"] = b64(replace(base64.b64decode(row["raw"]), 33, "other-trace")); self.reject(r)

    def test_durable_pair_binds_actual_request_and_sources(self):
        for key, value in (("version", 2), ("request", [0]), ("sections", []), ("draft", "changed")):
            r = receipt(); r["stages"][2]["journal"]["receipt"]["checkpoint"][key] = value; self.reject(r)
        r = receipt(); r["stages"][2]["journal"]["receipt"]["checkpoint"]["action"]["Split"]["child_draft"] = "changed"; self.reject(r)
        r = receipt(); del r["initialJournal"]["receipt"]; self.reject(r)

    def test_continuation_preserves_child_and_lineage(self):
        r = receipt(); row = r["stages"][-1]["fixture"]["fresh"]["sections"][2]; row["raw"] = b64(replace(base64.b64decode(row["raw"]), 12, body("lost", False))); self.reject(r)
        r = receipt(); write = r["stages"][5]["fixture"]["writes"][1]; data = C.decoded(write["documentData"]); row = C.field(data, 1, 2); row = replace(row, 38, remove(C.field(C.wire(row), 38, 2), 10)); write["documentData"] = b64(f(1, row) + f(14, TITLE)); self.reject(r)

    def test_terminal_and_stage_ownership_cannot_drift(self):
        r = receipt(); r["fixture"] = copy.deepcopy(r["fixture"]); r["fixture"]["read"] += 1; self.reject(r)
        r = receipt(); r["stages"][2]["geometry"]["native"]["generation"] = "77"; self.reject(r)
        r = receipt(); r["stages"][4]["geometry"]["native"]["generation"] = "2"; self.reject(r)


if __name__ == "__main__": unittest.main()
