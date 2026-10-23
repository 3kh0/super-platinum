"""Portable existing-child receipts; fake histories do not execute Automerge."""
import base64
import copy
import unittest

import canvas_end_reuse_oracle as R
import canvas_split_oracle as S
import test_canvas_end_split_oracle as end
from test_canvas_split_oracle import f, b64, body, replace, remove, merge, inventory, acknowledgement, SESSION, HEAD, CHILD_HEAD, CONT_HEAD, TITLE


def initial(corpus="ascii"):
    prior = end.initial(corpus); raw = {r["id"]: base64.b64decode(r["raw"]) for r in prior["sections"]}
    parent_wire = S.field(S.wire(raw["paragraph"]), 38, 2) + f(11, 88)
    parent = replace(replace(raw["paragraph"], 8, "aae"), 38, parent_wire)
    neighbor = replace(raw["neighbor"], 8, "aac")
    child_wire = f(1, "empty-final") + f(2, 1000) + f(3, 1000) + f(4, 999999) + f(5, b"retained-empty-child") + f(6, CHILD_HEAD) + f(8, CHILD_HEAD) + f(11, 77)
    child = f(1, "empty-final") + f(2, 1000) + f(6, "thread") + f(7, "document") + f(8, "aag") + f(9, 0) + f(10, 0) + f(12, body("", False)) + f(38, child_wire) + f(1002, 7)
    return inventory([raw["title"], neighbor, parent, child])


def request(source):
    raw = {r["id"]: S.decoded(r["raw"]) for r in source["sections"]}
    sent = []
    for identity in ("paragraph", "empty-final"):
        crdt = remove(remove(remove(S.field(raw[identity], 38, 2), 4), 7), 12)
        sent.append(f(1, identity) + f(2, 1000) + f(13, b"") + f(19, 1) + f(20, 1) + f(33, SESSION + "-0") + f(38, crdt))
    return dict(kind="reuse", parent="paragraph", child="empty-final", session=SESSION, sequence=1, formTitle="Fixture title", httpFactoryExecuted=False,
                documentData=b64(f(1, sent[0]) + f(1, sent[1]) + f(14, TITLE))), sent


def authored(sent, source, corpus="ascii", continued=False):
    original = {r["id"]: base64.b64decode(r["raw"]) for r in source["sections"]}; c = end.expected(corpus); result = []
    for outgoing in sent:
        parsed = S.wire(outgoing); identity = S.text(parsed, 1)
        row = merge(original[identity], outgoing)
        crdt = replace(replace(remove(S.field(parsed, 38, 2), 12), 3, 3000 if continued else 2000), 4, 999999)
        row = replace(replace(row, 2, 3000 if continued else 2000), 38, crdt)
        row = replace(row, 12, body(c.wire if identity == "paragraph" else c.edited if continued else "", identity == "paragraph"))
        result.append(row)
    return result


def checkpoint(source, write, corpus="ascii"):
    c = end.expected(corpus)
    return dict(version=5, scope=S.SCOPE.copy(), thread="thread", document="document", target="paragraph", session=SESSION, sequence=1,
                action={"EndReuse": dict(child="empty-final", position="aag", offset_utf16=c.offset)}, draft=c.wire,
                sections=[list(base64.b64decode(r["raw"])) for r in source["sections"]], request=list(base64.b64decode(write["documentData"])))


def receipt(scenario="success", corpus="ascii"):
    packet = end.receipt(scenario, corpus); source = initial(corpus); write, sent = request(source); c = end.expected(corpus)
    pair = authored(sent, source, corpus)
    untouched = [base64.b64decode(r["raw"]) for r in source["sections"][:2]]
    readback = inventory([*untouched, *pair])
    child_wire = S.field(S.wire(pair[1]), 38, 2)
    child_wire = replace(replace(remove(child_wire, 4), 5, b"continued-retained-child"), 12, CONT_HEAD) + f(6, CONT_HEAD)
    update = f(1, "empty-final") + f(2, 2000) + f(9, 0) + f(12, body(c.edited, False)) + f(13, b"") + f(19, 1) + f(20, 1) + f(33, "c" * 12 + "-0") + f(38, child_wire)
    second = dict(kind="text", parent="empty-final", session="c" * 12, sequence=1, formTitle="Fixture title", httpFactoryExecuted=False,
                  documentData=b64(f(1, update) + f(14, TITLE)))
    edited = authored([update], readback, corpus, True)[0]
    final_read = inventory([*untouched, pair[0], edited])
    held = dict(scope=S.SCOPE.copy(), receipt=dict(operation="same-existing-pair", checkpoint=checkpoint(source, write, corpus)))
    original = dict(packet["original"], mode="reuse", fresh=source, visible=source)
    packet.update(mode="reuse", original=copy.deepcopy(original))
    for i, stage in enumerate(packet["stages"]):
        proof = copy.deepcopy(original); name = stage["stage"]
        if i >= 2:
            fresh = copy.deepcopy(source)
            if scenario == "stale":
                row = fresh["sections"][3]; row.update(sequence=3000, raw=b64(replace(base64.b64decode(row["raw"]), 2, 3000)))
            proof["preflights"] = [dict(source=source, fresh=fresh, outcome="Changed" if scenario == "stale" else "prepared")]
            proof["fresh"] = fresh
            if scenario != "stale":
                proof.update(writes=[write], reservations=1)
                if i >= 3:
                    proof["acknowledgements"] = [acknowledgement(pair, scenario == "partial")]
                    if scenario == "success": proof.update(fresh=readback, visible=readback, readbacks=[readback], read=2)
                if i >= 5:
                    proof["preflights"].append(dict(source=readback, fresh=readback, outcome="prepared", kind="text")); proof["writes"].append(second)
                    if i == 6:
                        proof["acknowledgements"].append(acknowledgement([edited], continued=True)); proof["readbacks"].append(final_read)
                        proof.update(fresh=final_read, visible=final_read, read=3)
        pending, uncertain = name in ("saving", "continued-saving"), name == "uncertain"
        proof.update(pending=pending, stale=pending or uncertain, operation=dict(serial=1, stage="Uncertain" if uncertain else "Saving") if pending or uncertain else None)
        stage["fixture"] = copy.deepcopy(proof)
        stage["journal"] = copy.deepcopy(held if name in ("saving", "uncertain") else packet["initialJournal"])
        if stage["geometry"]["native"] is None:
            stage["geometry"]["paragraphs"] = [dict(id="neighbor", text="Another paragraph"), dict(id="paragraph", text=c.seed), dict(id="empty-final", text=c.edited if i == 6 else "")]
    packet.update(fixture=packet["stages"][-1]["fixture"], layout=packet["stages"][-1]["geometry"], finalJournal=packet["stages"][-1]["journal"])
    return packet


class ReuseOracle(unittest.TestCase):
    def rejects(self, value, corpus="ascii"):
        with self.assertRaises((RuntimeError, ValueError, TypeError, KeyError)): R.validate(value, corpus)

    def test_all_three_scenarios_both_corpora_stable_four_ids(self):
        for corpus in ("ascii", "unicode"):
            for scenario in S.STAGES:
                with self.subTest(corpus=corpus, scenario=scenario): R.validate(receipt(scenario, corpus), corpus)

    def test_two_existing_status_updates_not_new_child_insertion(self):
        source = initial(); write, sent = request(source); rows = R.inventory(source)
        for index in (0, 1):
            for tag, value in ((9, 0), (12, body("", False)), (8, "aag:temp")):
                pair = sent.copy(); pair[index] += f(tag, value)
                with self.assertRaises(RuntimeError): R.reuse_write(dict(write, documentData=b64(f(1, pair[0]) + f(1, pair[1]) + f(14, TITLE))), rows)
        with self.assertRaises(RuntimeError): R.reuse_write(dict(write, child="temp:C:xyz" + "b" * 25), rows)

    def test_each_history_and_retained_sequence_is_exact(self):
        source = initial(); write, sent = request(source); rows = R.inventory(source)
        for index in (0, 1):
            crdt = S.field(S.wire(sent[index]), 38, 2)
            for altered in (replace(crdt, 5, b"new-bootstrap"), remove(crdt, 11), crdt + f(6, CONT_HEAD), crdt + f(10, b"lineage"), crdt + f(12, HEAD)):
                pair = sent.copy(); pair[index] = replace(pair[index], 38, altered)
                with self.assertRaises(RuntimeError): R.reuse_write(dict(write, documentData=b64(f(1, pair[0]) + f(1, pair[1]) + f(14, TITLE))), rows)

    def test_no_added_removed_or_moved_identity_in_readback(self):
        for mutation in ("add", "remove", "move"):
            r = receipt(); rows = r["stages"][3]["fixture"]["readbacks"][0]["sections"]
            if mutation == "add": rows.append(copy.deepcopy(rows[-1]))
            elif mutation == "remove": rows.pop()
            else: rows[1], rows[2] = rows[2], rows[1]
            self.rejects(r)

    def test_existing_final_child_must_be_live_ordinary_and_empty(self):
        for tag, value in ((3, 1), (9, 64), (10, 1), (35, 1), (13, f(1, "container"))):
            source = initial(); row = source["sections"][-1]; row["raw"] = b64(replace(base64.b64decode(row["raw"]), tag, value))
            with self.assertRaises(RuntimeError): R.inventory(source)

    def test_complete_ack_and_unchanged_contents_are_required(self):
        r = receipt(); value = r["stages"][3]["fixture"]["acknowledgements"][0]
        data = S.wire(S.field(S.decoded(value["raw"]), 2, 2)); rows = S.messages(data, 1)
        value["raw"] = b64(f(1, 1) + f(2, f(15, "document") + f(1, S.raw(rows[0])))); self.rejects(r)
        r = receipt(); row = r["stages"][3]["fixture"]["readbacks"][0]["sections"][3]
        row["raw"] = b64(replace(base64.b64decode(row["raw"]), 12, body("changed", False))); self.rejects(r)

    def test_v5_existing_pair_custody_and_visible_offset(self):
        for key, value in (("version", 4), ("action", {"End": {}}), ("sections", []), ("request", [])):
            r = receipt(); r["stages"][2]["journal"]["receipt"]["checkpoint"][key] = value; self.rejects(r)
        r = receipt("success", "unicode"); cp = r["stages"][2]["journal"]["receipt"]["checkpoint"]
        self.assertEqual(cp["action"]["EndReuse"]["offset_utf16"], 11)
        cp["action"]["EndReuse"]["offset_utf16"] = 15; self.rejects(r, "unicode")

    def test_stale_zero_dispatch_and_partial_whole_pair_retention(self):
        r = receipt("stale"); r["stages"][2]["fixture"]["writes"] = [request(initial())[0]]; self.rejects(r)
        r = receipt("partial"); r["stages"][-1]["journal"]["receipt"] = None; self.rejects(r)
        r = receipt("partial"); r["stages"][-1]["fixture"]["acknowledgements"][0]["accepted"] = True; self.rejects(r)

    def test_stale_case_binds_existing_child_not_only_parent(self):
        r = receipt("stale"); source = r["stages"][2]["fixture"]; rows = source["fresh"]["sections"]
        rows[3].update(sequence=1000, raw=b64(replace(base64.b64decode(rows[3]["raw"]), 2, 1000)))
        rows[2].update(sequence=3000, raw=b64(replace(base64.b64decode(rows[2]["raw"]), 2, 3000)))
        self.rejects(r)
        r = receipt("stale"); row = r["stages"][2]["fixture"]["fresh"]["sections"][3]
        row["raw"] = b64(replace(base64.b64decode(row["raw"]), 12, body("changed", False))); self.rejects(r)

    def test_continuation_keeps_original_positive_base_and_parent(self):
        r = receipt(); write = r["stages"][5]["fixture"]["writes"][1]; row = S.field(S.decoded(write["documentData"]), 1, 2)
        crdt = S.field(S.wire(row), 38, 2); row = replace(row, 38, replace(crdt, 2, 0))
        write["documentData"] = b64(f(1, row) + f(14, TITLE)); self.rejects(r)
        r = receipt(); row = r["stages"][-1]["fixture"]["fresh"]["sections"][2]
        row["raw"] = b64(replace(base64.b64decode(row["raw"]), 12, body("changed"))); self.rejects(r)
        r = receipt(); write = r["stages"][5]["fixture"]["writes"][1]; row = S.field(S.decoded(write["documentData"]), 1, 2)
        crdt = S.field(S.wire(row), 38, 2) + f(6, CONT_HEAD)
        write["documentData"] = b64(f(1, replace(row, 38, crdt)) + f(14, TITLE)); self.rejects(r)

    def test_end_witness_stays_sealed_and_caller_mode_pinned(self):
        r = receipt(); r["mode"] = "end"; self.rejects(r)
        self.rejects(receipt("success", "unicode"))
        r = receipt(); r["savedWitness"]["placeholder"]["childHtml"] = "<br><br>"; self.rejects(r)
        r = receipt(); r["savedWitness"]["inputTrusted"] = False; self.rejects(r)


if __name__ == "__main__": unittest.main()
