"""Independent end-Enter receipt checks; no middle SplitInfo or parent edit.

Raw CRDT custody is checked here. Core executes Automerge history semantics;
opaque byte equality alone does not prove those semantics.
"""
import base64
from dataclasses import dataclass
import re

import canvas_split_oracle as shared
from canvas_heading import require, wire, decoded, field, text


@dataclass(frozen=True)
class Corpus:
    seed: str
    wire_seed: str
    edited: str
    wire_edited: str
    offset: int


def corpus_config(corpus="ascii"):
    if corpus == "ascii":
        return Corpus("Keep this", "Keep this", "New paragraph", "New paragraph", 9)
    if corpus == "unicode":
        return Corpus("Café & 日本😀", "Café &amp; 日本😀", "New 日本😀 paragraph", "New 日本😀 paragraph", 11)
    raise RuntimeError("Unknown independently pinned end-Enter corpus")


def labels(value, corpus):
    require(value.get("mode") == "end" and value.get("corpus") == corpus, "End receipt mode/corpus mismatch")


def inventory(value, original=None, child=None, edited=False, stale=False, corpus="ascii"):
    expected = corpus_config(corpus)
    require(isinstance(value, dict) and tuple(value.get(k) for k in ("file", "thread", "document"))
            == ("Ffixture", "thread", "document"), "End source owner changed")
    thread, document = decoded(value.get("threadRaw")), decoded(value.get("documentRaw"))
    require(text(thread, 1) == "thread" and text(thread, 10) == "document"
            and text(document, 1) == "document" and text(document, 6) == "thread"
            and text(document, 42) == "xyz" and text(wire(field(document, 69, 2)), 1) == "Fixture title", "End document changed")
    ids = ["title", "paragraph", *([child] if child else []), "neighbor"]
    rows = value.get("sections")
    require(isinstance(rows, list) and [r.get("id") for r in rows] == ids, "End canonical inventory/order changed")
    result = {}
    for item in rows:
        row = decoded(item.get("raw")); identity = item["id"]
        require(text(row, 1) == identity and text(row, 6) == "thread" and text(row, 7) == "document"
                and type(item.get("sequence")) is int and item["sequence"] == field(row, 2, 0), "End canonical row association changed")
        result[identity] = row
    title = result["title"]
    require(field(title, 9, 0) == 64 and field(title, 10, 0) == 48
            and text(wire(field(wire(field(title, 12, 2)), 58, 2)), 1) == "Fixture title"
            and field(title, 2, 0) == 1000 and text(title, 8) == "aaa", "End title changed")
    neighbor = result["neighbor"]
    require(shared.body(neighbor) == "Another paragraph" and text(neighbor, 8) == "aae" and field(neighbor, 2, 0) == 1000
            and next((f[3] for f in neighbor if f[0] == 1001), None) == b"\xc8\x3e\x81\x00", "End untouched neighbor changed")
    parent = result["paragraph"]
    require(shared.body(parent) == expected.wire_seed and text(parent, 8) == "aac"
            and field(parent, 2, 0) == (3000 if stale else 2000 if child else 1000), "End parent text/revision changed")
    if child:
        row = result[child]
        require(shared.body(row, False) == (expected.wire_edited if edited else "")
                and field(row, 2, 0) == (3000 if edited else 1000)
                and text(row, 8) == text(row, 21) == "aad", "End empty/edited child changed")
    if original:
        require(value["threadRaw"] == original["threadRaw"] and value["documentRaw"] == original["documentRaw"], "End raw document replaced")
        prior = {r["id"]: r["raw"] for r in original["sections"]}
        require(all(next(r["raw"] for r in rows if r["id"] == key) == prior[key] for key in ("title", "neighbor")), "End untouched bytes changed")
    return result


def end_write(value, original, corpus="ascii"):
    corpus_config(corpus)
    require(value.get("httpFactoryExecuted") is False and value.get("formTitle") == "Fixture title"
            and value.get("parent") == "paragraph" and type(value.get("sequence")) is int and value["sequence"] == 1
            and re.fullmatch(r"[0-9a-f]{12}", value.get("session", "")), "Wrong end request identity")
    child = value.get("child")
    require(isinstance(child, str) and re.fullmatch(r"temp:C:xyz[0-9a-f]{25}", child), "Wrong end child identity")
    data = decoded(value.get("documentData"))
    require([f[0] for f in data] == [1, 1, 14], "End request must retain the whole parent/child pair")
    parent, tail = shared.messages(data, 1)
    require([f[0] for f in parent] == [1, 2, 13, 19, 20, 33, 38]
            and [f[0] for f in tail] == [1, 2, 3, 8, 9, 10, 11, 12, 13, 16, 19, 20, 21, 29, 33, 35, 38], "End parent is not STATUS-only or child shape changed")
    require(text(parent, 1) == "paragraph" and field(parent, 2, 0) == 1000
            and text(tail, 1) == child and field(tail, 2, 0) == 0 and shared.body(tail, False) == ""
            and text(tail, 8) == text(tail, 21) == "aad:temp", "End child is not a new empty paragraph")
    for row in (parent, tail):
        require(field(row, 13, 2) == b"" and field(row, 19, 0) == field(row, 20, 0) == 1
                and text(row, 33) == value["session"] + "-0", "End pair trace/status changed")
    for tag in (3, 9, 10, 11, 29, 35):
        require(field(tail, tag, 0) == 0, "End child structural flags changed")
    require(field(tail, 16, 2) == b"\x20\x00", "End child attributes changed")
    shared.rich_title(data)
    before = wire(field(original, 38, 2))
    parent_crdt, child_crdt = (wire(field(row, 38, 2)) for row in (parent, tail))
    require(shared.stable(parent_crdt) == shared.stable(before, (4, 7, 12))
            and not any(f[0] in (4, 7, 10, 12) for f in parent_crdt), "End fabricated a parent history change, current head or SplitInfo")
    for crdt, identity, base in ((parent_crdt, "paragraph", 1000), (child_crdt, child, 0)):
        require(text(crdt, 1) == identity and field(crdt, 2, 0) == field(crdt, 3, 0) == base
                and field(crdt, 5, 2) and re.fullmatch(r"[0-9a-f]{64}", text(crdt, 8)), "End CRDT owner/base association changed")
    require([f[0] for f in child_crdt] == [1, 2, 3, 5, 6, 8, 12]
            and text(child_crdt, 6) == text(child_crdt, 8) == text(child_crdt, 12)
            and field(child_crdt, 5, 2) != field(parent_crdt, 5, 2), "End child is not one independent bootstrap")
    return child, parent, tail


def journal(value, write=None, original=None, corpus="ascii"):
    expected = corpus_config(corpus)
    require(isinstance(value, dict) and set(value) == {"scope", "receipt"} and value["scope"] == shared.SCOPE, "End durable scope changed")
    if write is None:
        require(value["receipt"] is None, "Unexpected end pair retained")
        return
    receipt = value["receipt"]
    require(isinstance(receipt, dict) and isinstance(receipt.get("operation"), str) and receipt["operation"], "End durable operation missing")
    c = receipt.get("checkpoint", {})
    require(set(c) == {"version", "scope", "thread", "document", "target", "session", "sequence", "action", "draft", "sections", "request"}
            and type(c.get("version")) is int and c["version"] == 4 and c.get("scope") == shared.SCOPE
            and c.get("thread") == "thread" and c.get("document") == "document" and c.get("target") == "paragraph"
            and c.get("session") == write["session"] and type(c.get("sequence")) is int and c["sequence"] == 1
            and c.get("draft") == expected.wire_seed, "End durable original draft/owner changed")
    require(c["action"] == {"End": {"child": write["child"], "position": "aad:temp", "offset_utf16": expected.offset}}, "End checkpoint lost empty-child intent/full visible offset")
    require(shared.byte_vector(c["request"]) == base64.b64decode(write["documentData"], validate=True), "End durable request differs from full pair")
    prior = {r["id"]: base64.b64decode(r["raw"], validate=True) for r in original["sections"]}
    stored = c["sections"]
    require(isinstance(stored, list) and len(stored) == len(prior)
            and {text(wire(shared.byte_vector(row)), 1): shared.byte_vector(row) for row in stored} == prior, "End durable source differs")


def witness(value, corpus="ascii"):
    expected = corpus_config(corpus)
    require(isinstance(value, dict) and value.get("beforeInputTrusted") is True and value.get("inputTrusted") is True
            and value.get("inputType") == "insertParagraph" and type(value.get("revision")) is int and value["revision"] == 1
            and type(value.get("offset")) is int and value["offset"] == expected.offset
            and shared.plain_sections(value.get("before")) == [expected.seed]
            and shared.plain_sections(value.get("after")) == [expected.seed, ""], "End native witness changed parent or boundary")
    require(all(isinstance(value.get(k), str) and value[k].isascii() and value[k].isdigit() for k in ("generation", "visit")), "End native witness owner absent")
    require(value.get("kind") == "end" and value.get("caretChildZero") is True, "End witness lacks owned child caret")
    placeholder(value.get("rawAfter"), value.get("after"), value.get("placeholder"), expected.seed)


def placeholder(raw_blocks, canonical, proof, seed):
    """Only the measured native SPAN>BR end branch may project newline to empty."""
    require(proof == dict(blockCount=1, sectionCount=2, childHtml="<span><br></span>", spanDepth=1, caretOffset=0)
            and all(type(proof.get(k)) is int for k in ("blockCount", "sectionCount", "spanDepth", "caretOffset")),
            "End empty projection lacks exact DOM/child-caret proof")
    require(shared.plain_sections(raw_blocks) == [seed, "\n"] and shared.plain_sections(canonical) == [seed, ""],
            "End raw placeholder/canonical sections changed")
    require(raw_blocks[0]["elements"][1]["elements"] == [dict(type="text", text="\n")]
            and canonical[0]["elements"][1]["elements"] == []
            and raw_blocks[0]["elements"][0] == canonical[0]["elements"][0], "End projection rewrote more than the known empty placeholder")


def ack(value, sent, original, partial=False, continuation=False, corpus="ascii"):
    expected = corpus_config(corpus)
    require(value.get("partial") is partial and value.get("accepted") is (not partial), "End ACK admission changed")
    outer = decoded(value.get("raw"))
    require([f[0] for f in outer] == [1, 2] and field(outer, 1, 0) == 1, "End raw ACK malformed")
    data = wire(field(outer, 2, 2))
    require([f[0] for f in data] == [15] + [1] * len(sent) and text(data, 15) == "document", "End complete ACK pair missing/duplicated")
    rows = shared.messages(data, 1)
    for row, outgoing in zip(rows, sent):
        identity = text(outgoing, 1); parent = identity == "paragraph"
        sequence = 3000 if continuation else 2000 if parent else 1000
        require(text(row, 1) == identity and text(row, 6) == "thread" and text(row, 7) == "document"
                and field(row, 2, 0) == sequence
                and shared.body(row, parent) == (expected.wire_seed if parent else expected.wire_edited if continuation else ""),
                "End ACK changed unchanged parent/expected child")
        current, prior = wire(field(row, 38, 2)), wire(field(outgoing, 38, 2))
        require(field(current, 3, 0) == sequence and field(current, 4, 0) > 0
                and not any(f[0] in (10, 12) for f in current)
                and shared.stable(current, (3, 4, 12)) == shared.stable(prior, (3, 4, 12)), "End ACK lost exact history or invented lineage")
        if parent:
            require(shared.stable(row, (2, 13, 19, 20, 33, 38)) == shared.stable(original, (2, 13, 19, 20, 33, 38)), "End ACK modified parent content/opaque fields")
        elif continuation:
            require(shared.stable(row, (2, 12, 33, 38)) == shared.stable(original, (2, 12, 33, 38)), "Child edit changed structure/opaque fields")
        else:
            require(shared.stable(row, (2, 6, 7, 8, 21, 38)) == shared.stable(outgoing, (2, 6, 7, 8, 21, 38))
                    and text(row, 8) == text(row, 21) == "aad", "Empty child ACK structure changed")
    return rows


def continuation(value, child, original, corpus="ascii"):
    expected = corpus_config(corpus)
    data = decoded(value.get("documentData"))
    require(value.get("kind") == "text" and value.get("parent") == child and value.get("httpFactoryExecuted") is False
            and value.get("formTitle") == "Fixture title" and type(value.get("sequence")) is int and value["sequence"] == 1
            and re.fullmatch(r"[0-9a-f]{12}", value.get("session", "")) and [f[0] for f in data] == [1, 14], "End continuation is not ordinary child Save")
    shared.rich_title(data)
    row = wire(field(data, 1, 2))
    require([f[0] for f in row] == [1, 2, 9, 12, 13, 19, 20, 33, 38]
            and text(row, 1) == child and field(row, 2, 0) == 1000 and field(row, 9, 0) == 0
            and shared.body(row, False) == expected.wire_edited and field(row, 13, 2) == b""
            and field(row, 19, 0) == field(row, 20, 0) == 1 and text(row, 33) == value["session"] + "-0", "Ordinary child request changed identity/body")
    crdt, before = wire(field(row, 38, 2)), wire(field(original, 38, 2))
    require(text(crdt, 1) == child and field(crdt, 2, 0) == 0 and field(crdt, 3, 0) == 1000
            and text(crdt, 8) == text(before, 8) and field(crdt, 5, 2) != field(before, 5, 2)
            and not any(f[0] == 10 for f in crdt) and re.fullmatch(r"[0-9a-f]{64}", text(crdt, 12))
            and text(crdt, 12) != text(crdt, 8)
            and {text(crdt, 8), text(crdt, 12)} <= {f[2].decode() for f in crdt if f[0] == 6 and f[1] == 2}, "Child continuation lost original bootstrap custody")
    return row


def validate(receipt, corpus="ascii"):
    expected = corpus_config(corpus); labels(receipt, corpus)
    scenario = receipt.get("scenario")
    require(scenario in shared.STAGES and all(receipt.get(k) is True for k in ("ok", "noSession", "noTransport", "trustedNativeEnter"))
            and receipt.get("httpFactoryExecuted") is False, "End scenario isolation failed")
    original = receipt.get("original", {}); labels(original, corpus)
    initial = original.get("fresh"); initial_rows = inventory(initial, corpus=corpus)
    require(original.get("visible") == initial and original.get("pending") is False and original.get("operation") is None
            and original.get("stale") is False and original.get("loading") is False and original.get("reservations") == 0
            and original.get("capacityReason") is None, "End original state not ready")
    keys = ("preflights", "writes", "acknowledgements", "readbacks")
    require(all(original.get(k) == [] for k in keys), "End fixture already dispatched")
    journal(receipt.get("initialJournal"), corpus=corpus)
    cancelled, saved = receipt.get("cancelledWitness"), receipt.get("savedWitness")
    witness(cancelled, corpus); witness(saved, corpus)
    require(cancelled["visit"] == saved["visit"] and cancelled["generation"] != saved["generation"]
            and receipt.get("duplicate") == {"ok": True, "clicks": 2}, "End witness ownership/duplicate guard missing")
    stages = receipt.get("stages")
    require(isinstance(stages, list) and len(stages) == len(shared.STAGES[scenario]), "End stages incomplete")
    previous = {k: [] for k in keys}; owners = {cancelled["generation"], saved["generation"]}
    child = sent = pair_rows = first_journal = None
    for index, (stage, name) in enumerate(zip(stages, shared.STAGES[scenario])):
        require(stage.get("stage") == name and type(stage.get("ordinal")) is int and stage["ordinal"] == index
                and all(stage.get(k) is True for k in ("ok", "noSession", "noTransport")), "End stage identity changed")
        source, geometry = stage.get("fixture", {}), stage.get("geometry", {}); labels(source, corpus)
        counts = (0, 0, 0, 0) if index < 2 else ((1, 0, 0, 0) if scenario == "stale" else
                 (1, 1, 0, 0) if index == 2 else (1, 1, 1, 0) if scenario == "partial" else
                 (1, 1, 1, 1) if index < 5 else (2, 2, 1, 1) if index == 5 else (2, 2, 2, 2))
        for key, count in zip(keys, counts):
            values = source.get(key)
            require(isinstance(values, list) and len(values) == count and values[:len(previous[key])] == previous[key], "End immutable ledger changed: " + key)
            previous[key] = values
        require(source.get("scenario") == scenario and source.get("visit") == original.get("visit") and type(source.get("read")) is int
                and source["read"] > 0 and source.get("loading") is False and type(source.get("reservations")) is int
                and source["reservations"] == int(scenario != "stale" and index >= 2), "End ownership/dispatch reservation changed")
        if source["preflights"]:
            first = source["preflights"][0]
            require(first.get("source") == initial and first.get("outcome") == ("Changed" if scenario == "stale" else "prepared"), "End fresh preflight not bound")
            inventory(first.get("fresh"), initial, stale=scenario == "stale", corpus=corpus)
            if scenario != "stale": require(first["fresh"] == initial, "End source changed before dispatch")
        if source["writes"] and sent is None:
            child, parent, tail = end_write(source["writes"][0], initial_rows["paragraph"], corpus); sent = [parent, tail]
        accepted = scenario == "success" and index >= 3
        current = inventory(source.get("fresh"), initial, child if accepted else None, edited=index == 6, stale=scenario == "stale" and index >= 2, corpus=corpus)
        inventory(source.get("visible"), initial, child if accepted else None, edited=index == 6, corpus=corpus)
        if not accepted:
            require(source["visible"] == initial, "End unaccepted edit changed visible source")
            if scenario == "stale" and index >= 2:
                require(source["fresh"] == source["preflights"][0]["fresh"]
                        and shared.stable(current["paragraph"], (2,)) == shared.stable(initial_rows["paragraph"], (2,)), "End stale source changed beyond revision")
            else: require(source["fresh"] == initial, "End unaccepted edit changed canonical source")
        if source["acknowledgements"]:
            pair_rows = ack(source["acknowledgements"][0], sent[:1] if scenario == "partial" else sent, initial_rows["paragraph"], partial=scenario == "partial", corpus=corpus)
        if accepted:
            first_read = inventory(source["readbacks"][0], initial, child, corpus=corpus)
            require(all(shared.raw(first_read[text(row, 1)]) == shared.raw(row) for row in pair_rows), "End independent readback differs from ACK pair")
            require(source["fresh"] == source["visible"] == source["readbacks"][-1], "End accepted source/readback custody differs")
        if index >= 5:
            row = continuation(source["writes"][1], child, pair_rows[1], corpus)
            require(source["preflights"][1].get("source") == source["preflights"][1].get("fresh") == source["readbacks"][0]
                    and source["preflights"][1].get("outcome") == "prepared", "Child continuation preflight changed")
            if index == 6:
                rows = ack(source["acknowledgements"][1], [row], pair_rows[1], continuation=True, corpus=corpus)
                require(shared.raw(current[child]) == shared.raw(rows[0]) and shared.raw(current["paragraph"]) == shared.raw(pair_rows[0]), "Child continuation altered parent or lost readback")
        pending, uncertain = name in ("saving", "continued-saving"), name == "uncertain"
        require(source.get("pending") is pending and source.get("stale") is (pending or uncertain), "End pending/stale state changed")
        if pending or uncertain:
            require((source.get("operation") or {}).get("stage") == ("Saving" if pending else "Uncertain"), "End operation lock changed")
        else:
            require(source.get("operation") is None, "End retained an unexpected operation")
        held = name == "saving" or uncertain
        journal(stage.get("journal"), source["writes"][0] if held else None, initial, corpus)
        if name == "saving": first_journal = stage["journal"]
        if uncertain: require(stage["journal"] == first_journal, "Partial end ACK changed durable whole pair")
        require(geometry.get("ok") is True and geometry.get("noOverflow") is True, "End layout overflow")
        parts = [expected.seed, ""] if name in ("split", "saving", "refused", "uncertain") else [""] if name == "child-editing" else [expected.edited] if name == "continued-saving" else None
        if parts is not None:
            shared.native(geometry.get("native"), parts, not (pending or uncertain), shared.ERROR if name == "refused" else None)
            owner = cancelled if name == "split" else saved if name in ("saving", "refused", "uncertain") else None
            if owner:
                require(all(geometry["native"].get(k) == owner[k] for k in ("generation", "visit")), "End native witness owner changed")
                placeholder(geometry["native"].get("rawBlocks"), geometry["native"].get("blocks"), geometry["native"].get("placeholder"), expected.seed)
                require(geometry["native"]["rawBlocks"] == owner["rawAfter"]
                        and geometry["native"]["blocks"] == owner["after"]
                        and geometry["native"]["placeholder"] == owner["placeholder"], "End native stage differs from original sealed raw witness")
            else:
                require(geometry["native"].get("rawBlocks") == geometry["native"].get("blocks")
                        and geometry["native"].get("placeholder") is None, "Ordinary child edit used an end newline projection")
                if name == "child-editing":
                    require(geometry["native"]["blocks"][0]["elements"][0]["elements"] == [], "Reopened empty child is not an ordinary empty section")
                generation = geometry["native"].get("generation")
                require(isinstance(generation, str) and generation.isascii() and generation.isdigit() and generation not in owners
                        and geometry["native"].get("visit") == saved["visit"], "End child editor owner reused")
                owners.add(generation)
        else:
            require(geometry.get("native") is None, "End reader retained editor")
            paragraphs = [dict(id="paragraph", text=expected.seed)]
            if accepted: paragraphs.append(dict(id=child, text=expected.edited if index == 6 else ""))
            paragraphs.append(dict(id="neighbor", text="Another paragraph"))
            require(geometry.get("paragraphs") == paragraphs, "End visible unchanged-parent/child order differs")
        if name == "cancelled": require(source == original, "End Cancel mutated source")
    require(receipt.get("fixture") == stages[-1]["fixture"] and receipt.get("layout") == stages[-1]["geometry"]
            and receipt.get("finalJournal") == stages[-1]["journal"], "End terminal differs from original final stage")
