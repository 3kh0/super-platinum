"""Portable end-Enter receipts; opaque fake histories are not Automerge proof."""
import base64
import copy
from types import SimpleNamespace
import unittest

import canvas_end_split_oracle as E
import canvas_split_oracle as S
from test_canvas_split_oracle import f, b64, body, blocks, replace, remove, merge, inventory, source_rows, acknowledgement, native, SESSION, CHILD, HEAD, CHILD_HEAD, CONT_HEAD, TITLE


def expected(corpus="ascii"):
    if corpus == "ascii":
        return SimpleNamespace(seed="Keep this", wire="Keep this", edited="New paragraph", offset=9)
    assert corpus == "unicode"
    return SimpleNamespace(seed="Café & 日本😀", wire="Café &amp; 日本😀", edited="New 日本😀 paragraph", offset=11)


def initial(corpus="ascii"):
    rows = source_rows(); c = expected(corpus)
    crdt = f(1, "paragraph") + f(2, 1000) + f(3, 1000) + f(4, 999999) + f(5, b"retained-parent-history") + f(6, HEAD) + f(8, HEAD)
    rows[1] = replace(rows[1], 12, body(c.wire)) + f(38, crdt)
    return inventory(rows)


def request(original):
    source = S.decoded(original["sections"][1]["raw"])
    parent_wire = remove(S.field(source, 38, 2), 4)
    parent = f(1, "paragraph") + f(2, 1000) + f(13, b"") + f(19, 1) + f(20, 1) + f(33, SESSION + "-0") + f(38, parent_wire)
    child_wire = f(1, CHILD) + f(2, 0) + f(3, 0) + f(5, b"empty-child-bootstrap") + f(6, CHILD_HEAD) + f(8, CHILD_HEAD) + f(12, CHILD_HEAD)
    child = f(1, CHILD) + f(2, 0) + f(3, 0) + f(8, "aad:temp") + f(9, 0) + f(10, 0) + f(11, 0) + f(12, body("", False)) + f(13, b"") + f(16, f(4, 0)) + f(19, 1) + f(20, 1) + f(21, "aad:temp") + f(29, 0) + f(33, SESSION + "-0") + f(35, 0) + f(38, child_wire)
    value = dict(kind="end", documentData=b64(f(1, parent) + f(1, child) + f(14, TITLE)), formTitle="Fixture title", session=SESSION, sequence=1, parent="paragraph", child=CHILD, httpFactoryExecuted=False)
    return value, [parent, child]


def authored_rows(sent, original, corpus="ascii", continued=False):
    result = []; c = expected(corpus)
    for outgoing in sent:
        parsed = S.wire(outgoing); parent = S.text(parsed, 1) == "paragraph"
        row = merge(original, outgoing) if parent or continued else outgoing
        sequence = 3000 if continued else 2000 if parent else 1000
        crdt = replace(replace(remove(S.field(parsed, 38, 2), 12), 3, sequence), 4, 999999)
        for tag, value in ((2, sequence), (6, "thread"), (7, "document"), (12, body(c.wire if parent else c.edited if continued else "", parent)), (38, crdt)):
            row = replace(row, tag, value)
        if not parent: row = replace(replace(row, 8, "aad"), 21, "aad")
        result.append(row)
    return result


def checkpoint(original, write, corpus="ascii"):
    c = expected(corpus)
    return dict(version=4, scope=S.SCOPE.copy(), thread="thread", document="document", target="paragraph", session=SESSION, sequence=1,
                action={"End": dict(child=CHILD, position="aad:temp", offset_utf16=c.offset)}, draft=c.wire,
                sections=[list(base64.b64decode(r["raw"])) for r in original["sections"]], request=list(base64.b64decode(write["documentData"])))


def witness(generation, corpus="ascii"):
    c = expected(corpus)
    after = blocks(c.seed, ""); after[0]["elements"][1]["elements"] = []
    return dict(generation=str(generation), visit="0", offset=c.offset, revision=1, beforeInputTrusted=True, inputTrusted=True,
                inputType="insertParagraph", before=blocks(c.seed), after=after, kind="end", caretChildZero=True,
                rawAfter=blocks(c.seed, "\n"), placeholder=dict(blockCount=1, sectionCount=2, childHtml="<span><br></span>", spanDepth=1, caretOffset=0))


def receipt(scenario="success", corpus="ascii"):
    c = expected(corpus); original_inventory = initial(corpus)
    originals = [S.raw(S.decoded(r["raw"])) for r in original_inventory["sections"]]
    original = dict(mode="end", corpus=corpus, scenario=scenario, pending=False, preflights=[], writes=[], acknowledgements=[], readbacks=[], reservations=0,
                    fresh=original_inventory, visible=original_inventory, visit=0, read=1, stale=False, loading=False, capacityReason=None, operation=None)
    write, sent = request(original_inventory)
    pair = authored_rows(sent, originals[1], corpus)
    readback = inventory([originals[0], *pair, originals[2]])
    empty_journal = dict(scope=S.SCOPE.copy(), receipt=None)
    held = dict(scope=S.SCOPE.copy(), receipt=dict(operation="whole-end-pair", checkpoint=checkpoint(original_inventory, write, corpus)))
    child_crdt = remove(S.field(S.wire(pair[1]), 38, 2), 4)
    child_crdt = replace(replace(child_crdt, 5, b"edited-child-history"), 12, CONT_HEAD) + f(6, CONT_HEAD)
    child_update = f(1, CHILD) + f(2, 1000) + f(9, 0) + f(12, body(c.edited, False)) + f(13, b"") + f(19, 1) + f(20, 1) + f(33, "c" * 12 + "-0") + f(38, child_crdt)
    second = dict(kind="text", documentData=b64(f(1, child_update) + f(14, TITLE)), formTitle="Fixture title", session="c" * 12, sequence=1, parent=CHILD, httpFactoryExecuted=False)
    edited = authored_rows([child_update], pair[1], corpus, True)[0]
    final_read = inventory([originals[0], pair[0], edited, originals[2]])
    stages = []
    for i, name in enumerate(S.STAGES[scenario]):
        source = copy.deepcopy(original)
        if i >= 2:
            fresh = copy.deepcopy(original_inventory)
            if scenario == "stale": fresh["sections"][1].update(sequence=3000, raw=b64(replace(originals[1], 2, 3000)))
            source["preflights"] = [dict(source=original_inventory, fresh=fresh, outcome="Changed" if scenario == "stale" else "prepared")]
            source["fresh"] = fresh
            if scenario != "stale":
                source.update(writes=[write], reservations=1)
                if i >= 3:
                    source["acknowledgements"] = [acknowledgement(pair, scenario == "partial")]
                    if scenario == "success": source.update(fresh=readback, visible=readback, readbacks=[readback], read=2)
                if i >= 5:
                    source["preflights"].append(dict(source=readback, fresh=readback, outcome="prepared", kind="text"))
                    source["writes"].append(second)
                    if i == 6:
                        source["acknowledgements"].append(acknowledgement([edited], continued=True))
                        source["readbacks"].append(final_read); source.update(fresh=final_read, visible=final_read, read=3)
        pending, uncertain = name in ("saving", "continued-saving"), name == "uncertain"
        source.update(pending=pending, stale=pending or uncertain, operation=dict(serial=1, stage="Uncertain" if uncertain else "Saving") if pending or uncertain else None)
        parts = [c.seed, ""] if name in ("split", "saving", "refused", "uncertain") else [""] if name == "child-editing" else [c.edited] if name == "continued-saving" else None
        geometry = dict(ok=True, noOverflow=True, native=native(parts, 1 if name == "split" else 3 if name == "child-editing" else 4 if name == "continued-saving" else 2,
                                                               not (pending or uncertain), S.ERROR if name == "refused" else None) if parts is not None else None)
        if parts is not None:
            editor = geometry["native"]
            if name in ("split", "saving", "refused", "uncertain"):
                evidence = witness(1 if name == "split" else 2, corpus)
                editor.update(blocks=evidence["after"], rawBlocks=evidence["rawAfter"], placeholder=evidence["placeholder"])
            else:
                if name == "child-editing": editor["blocks"][0]["elements"][0]["elements"] = []
                editor.update(rawBlocks=copy.deepcopy(editor["blocks"]), placeholder=None)
        if parts is None:
            geometry["paragraphs"] = [dict(id="paragraph", text=c.seed)]
            if scenario == "success" and i >= 3: geometry["paragraphs"].append(dict(id=CHILD, text=c.edited if i == 6 else ""))
            geometry["paragraphs"].append(dict(id="neighbor", text="Another paragraph"))
        stages.append(dict(ok=True, stage=name, ordinal=i, noSession=True, noTransport=True, fixture=copy.deepcopy(source), geometry=geometry,
                           journal=copy.deepcopy(held if name in ("saving", "uncertain") else empty_journal)))
    return dict(ok=True, noSession=True, noTransport=True, mode="end", corpus=corpus, scenario=scenario, httpFactoryExecuted=False, trustedNativeEnter=True,
                original=original, initialJournal=empty_journal, cancelledWitness=witness(1, corpus), savedWitness=witness(2, corpus), duplicate=dict(ok=True, clicks=2),
                stages=stages, fixture=stages[-1]["fixture"], layout=stages[-1]["geometry"], finalJournal=stages[-1]["journal"])


class EndSplitOracle(unittest.TestCase):
    def rejects(self, r, corpus="ascii"):
        with self.assertRaises((RuntimeError, ValueError, TypeError, KeyError)): E.validate(r, corpus)

    def test_complete_ascii_unicode_end_scenarios(self):
        for corpus in ("ascii", "unicode"):
            for scenario in S.STAGES:
                with self.subTest(corpus=corpus, scenario=scenario): E.validate(receipt(scenario, corpus), corpus)

    def test_mode_corpus_and_full_visible_offset_are_independently_pinned(self):
        r = receipt(); r["mode"] = "middle"; self.rejects(r)
        self.rejects(receipt("success", "unicode"))
        for offset in (10, 15, 27):
            r = receipt("success", "unicode"); r["savedWitness"]["offset"] = offset; self.rejects(r, "unicode")
        r = receipt(); r["savedWitness"]["inputTrusted"] = False; self.rejects(r)

    def test_parent_status_update_cannot_add_content_or_split_info(self):
        source = initial(); write, sent = request(source); parent = S.decoded(source["sections"][1]["raw"])
        crdt = S.field(S.wire(sent[0]), 38, 2)
        for bad in (sent[0] + f(12, body("Keep this")), sent[0] + f(9, 0), replace(sent[0], 38, crdt + f(10, b"fake-lineage")), replace(sent[0], 38, crdt + f(12, HEAD))):
            with self.assertRaises(RuntimeError): E.end_write(dict(write, documentData=b64(f(1, bad) + f(1, sent[1]) + f(14, TITLE))), parent)

    def test_parent_history_cannot_be_fabricated_or_removed(self):
        source = initial(); write, sent = request(source); parent = S.decoded(source["sections"][1]["raw"])
        crdt = S.field(S.wire(sent[0]), 38, 2)
        for bad in (replace(crdt, 5, b"fabricated-change"), crdt + f(6, CONT_HEAD), remove(crdt, 6)):
            with self.assertRaises(RuntimeError): E.end_write(dict(write, documentData=b64(f(1, replace(sent[0], 38, bad)) + f(1, sent[1]) + f(14, TITLE))), parent)

    def test_child_must_be_empty_one_bootstrap_without_lineage(self):
        source = initial(); write, sent = request(source); parent = S.decoded(source["sections"][1]["raw"])
        crdt = S.field(S.wire(sent[1]), 38, 2)
        for bad in (replace(crdt, 12, CONT_HEAD), crdt + f(6, CONT_HEAD), crdt + f(10, b"lineage")):
            with self.assertRaises(RuntimeError): E.end_write(dict(write, documentData=b64(f(1, sent[0]) + f(1, replace(sent[1], 38, bad)) + f(14, TITLE))), parent)
        r = receipt(); r["stages"][0]["geometry"]["native"]["blocks"] = blocks("Keep this", "not empty"); self.rejects(r)

    def test_v4_end_checkpoint_cannot_be_middle_or_single_target(self):
        for key, value in (("version", 3), ("request", [0]), ("sections", []), ("action", {"Split": {}})):
            r = receipt(); r["stages"][2]["journal"]["receipt"]["checkpoint"][key] = value; self.rejects(r)
        r = receipt("success", "unicode"); r["stages"][2]["journal"]["receipt"]["checkpoint"]["action"]["End"]["offset_utf16"] = 15; self.rejects(r, "unicode")

    def test_partial_ack_retains_exact_whole_pair_and_unchanged_source(self):
        r = receipt("partial"); r["stages"][-1]["journal"]["receipt"] = None; self.rejects(r)
        r = receipt("partial"); r["stages"][-1]["fixture"]["acknowledgements"][0]["accepted"] = True; self.rejects(r)
        r = receipt("stale"); r["stages"][2]["fixture"]["writes"] = [request(initial())[0]]; self.rejects(r)

    def test_canonical_parent_remains_exact_through_child_edit(self):
        r = receipt(); row = r["stages"][-1]["fixture"]["fresh"]["sections"][1]
        row["raw"] = b64(replace(base64.b64decode(row["raw"]), 12, body("changed"))); self.rejects(r)
        r = receipt(); write = r["stages"][5]["fixture"]["writes"][1]; write["parent"] = "paragraph"; self.rejects(r)
        r = receipt(); row = r["stages"][3]["fixture"]["readbacks"][0]["sections"][1]
        row["raw"] = b64(replace(base64.b64decode(row["raw"]), 33, "other")); self.rejects(r)

    def test_complete_ack_requires_independent_unchanged_parent_and_empty_child(self):
        r = receipt(); value = r["stages"][3]["fixture"]["acknowledgements"][0]
        data = S.wire(S.field(S.decoded(value["raw"]), 2, 2)); rows = S.messages(data, 1)
        value["raw"] = b64(f(1, 1) + f(2, f(15, "document") + f(1, S.raw(rows[0]))))
        self.rejects(r)
        r = receipt(); value = r["stages"][3]["fixture"]["acknowledgements"][0]
        data = S.wire(S.field(S.decoded(value["raw"]), 2, 2)); rows = S.messages(data, 1)
        value["raw"] = b64(f(1, 1) + f(2, f(15, "document") + f(1, S.raw(rows[0])) + f(1, replace(S.raw(rows[1]), 12, body("not empty", False)))))
        self.rejects(r)

    def test_end_newline_projection_requires_exact_raw_dom_and_caret(self):
        for key, value in (("childHtml", "<span><br><br></span>"), ("childHtml", "<b><br></b>"), ("childHtml", '<span style="font-weight:bold"><br></span>'),
                           ("childHtml", "<span>content<br></span>"), ("caretOffset", 1), ("spanDepth", 0), ("blockCount", True)):
            r = receipt(); r["savedWitness"]["placeholder"][key] = value; self.rejects(r)
        for raw in (blocks("Keep this", ""), blocks("Keep this", "\n\n"), blocks("Keep this\n", "\n")):
            r = receipt(); r["savedWitness"]["rawAfter"] = raw; self.rejects(r)
        r = receipt(); r["savedWitness"]["caretChildZero"] = False; self.rejects(r)
        r = receipt(); del r["savedWitness"]["rawAfter"]; self.rejects(r)

    def test_ordinary_reopened_child_does_not_use_end_projection(self):
        r = receipt(); r["stages"][4]["geometry"]["native"]["rawBlocks"] = blocks("\n"); self.rejects(r)
        r = receipt(); r["stages"][4]["geometry"]["native"]["placeholder"] = witness(1)["placeholder"]; self.rejects(r)
        r = receipt(); r["stages"][2]["geometry"]["native"]["rawBlocks"] = blocks("Keep this", "\n\n"); self.rejects(r)


if __name__ == "__main__": unittest.main()
