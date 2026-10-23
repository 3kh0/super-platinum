"""Existing final-empty reuse: two STATUS updates, stable identities/history.

Positive-base checks retain opaque byte custody. Zero-base source is pinned to
the retained original bootstrap; its continuation also decodes actual history.
"""
import base64
import re

import canvas_end_split_oracle as end
import canvas_split_oracle as shared
import canvas_reuse_base as bases
from canvas_heading import require, wire, decoded, field, text

IDS = ("title", "neighbor", "paragraph", "empty-final")
POSITIONS = ("aaa", "aac", "aae", "aag")
PAIR = ("paragraph", "empty-final")


def labels(value, corpus, reuse_base="positive"):
    bases.labels(value, bases.config(reuse_base))
    require(value.get("mode") == "reuse" and value.get("corpus") == corpus, "Reuse caller-pinned mode/corpus changed")


def inventory(value, original=None, accepted=False, edited=False, stale=False, corpus="ascii", reuse_base="positive"):
    cohort = bases.config(reuse_base); ids = cohort.ids
    expected = end.corpus_config(corpus)
    require(isinstance(value, dict) and tuple(value.get(k) for k in ("file", "thread", "document"))
            == ("Ffixture", "thread", "document"), "Reuse canonical owner changed")
    thread, document = decoded(value.get("threadRaw")), decoded(value.get("documentRaw"))
    require(text(thread, 1) == "thread" and text(thread, 10) == "document" and text(document, 1) == "document"
            and text(document, 6) == "thread" and text(document, 42) == "xyz"
            and text(wire(field(document, 69, 2)), 1) == "Fixture title", "Reuse document source changed")
    rows = value.get("sections")
    require(isinstance(rows, list) and [r.get("id") for r in rows] == list(ids), "Reuse inserted/removed/reordered an identity")
    result = {}
    for item, identity, position in zip(rows, ids, POSITIONS):
        row = decoded(item.get("raw"))
        require(text(row, 1) == identity and text(row, 6) == "thread" and text(row, 7) == "document" and text(row, 8) == position
                and type(item.get("sequence")) is int and item["sequence"] == field(row, 2, 0), "Reuse row owner/position/revision changed")
        result[identity] = row
    title, neighbor, parent, child = (result[k] for k in ids)
    for row in (parent, child):
        for tag in (3, 9, 10, 35):
            if any(f[0] == tag for f in row): require(field(row, tag, 0) == 0, "Reuse member is not a live ordinary editable paragraph")
        if any(f[0] == 13 for f in row): require(field(row, 13, 2) == b"", "Reuse member gained a structural parent")
    require(field(title, 9, 0) == 64 and field(title, 10, 0) == 48 and field(title, 2, 0) == 1000
            and text(wire(field(wire(field(title, 12, 2)), 58, 2)), 1) == "Fixture title", "Reuse title changed")
    require(shared.body(neighbor) == "Another paragraph" and field(neighbor, 2, 0) == 1000
            and next((f[3] for f in neighbor if f[0] == 1001), None) == b"\xc8\x3e\x81\x00", "Reuse untouched neighbor changed")
    require(shared.body(parent) == expected.wire_seed and field(parent, 2, 0) == (2000 if accepted else 1000), "Reuse parent changed")
    require(shared.body(child, False) == (expected.wire_edited if edited else "")
            and field(child, 2, 0) == (3000 if edited or stale else 2000 if accepted else 1000), "Reused empty/edited child changed")
    if original:
        require(value["threadRaw"] == original["threadRaw"] and value["documentRaw"] == original["documentRaw"], "Reuse thread/document bytes changed")
        source = {r["id"]: r["raw"] for r in original["sections"]}
        require(all(next(r["raw"] for r in rows if r["id"] == key) == source[key] for key in ("title", "neighbor")), "Reuse untouched inventory bytes changed")
    if cohort.name == "zero" and original is None and not accepted and not stale: bases.zero_source(child)
    return result


def reuse_write(value, original, corpus="ascii", reuse_base="positive"):
    cohort = bases.config(reuse_base); pair = cohort.pair
    end.corpus_config(corpus)
    require(value.get("kind") == "reuse" and value.get("httpFactoryExecuted") is False and value.get("formTitle") == "Fixture title"
            and value.get("parent") == pair[0] and value.get("child") == pair[1]
            and type(value.get("sequence")) is int and value["sequence"] == 1
            and re.fullmatch(r"[0-9a-f]{12}", value.get("session", "")), "Reuse request identity changed or allocated new child")
    if cohort.name == "zero": bases.zero_source(original[cohort.child])
    data = decoded(value.get("documentData"))
    require([f[0] for f in data] == [1, 1, 14], "Reuse must send one complete existing pair")
    shared.rich_title(data); rows = shared.messages(data, 1)
    for row, identity in zip(rows, pair):
        require([f[0] for f in row] == [1, 2, 13, 19, 20, 33, 38]
                and text(row, 1) == identity and field(row, 2, 0) == 1000 and field(row, 13, 2) == b""
                and field(row, 19, 0) == field(row, 20, 0) == 1 and text(row, 33) == value["session"] + "-0", "Reuse member is not exact STATUS-only update")
        current, before = wire(field(row, 38, 2)), wire(field(original[identity], 38, 2))
        require(shared.stable(current) == shared.stable(before, (4, 7, 12))
                and not any(f[0] in (4, 7, 10, 12) for f in current), "Reuse fabricated bootstrap/history/head/lineage or dropped retained metadata")
        require(text(current, 1) == identity and field(current, 2, 0) == (1000 if identity == pair[0] else cohort.base) and field(current, 3, 0) == 1000
                and field(current, 5, 2) and re.fullmatch(r"[0-9a-f]{64}", text(current, 8)), "Reuse lost selected retained session base")
    return pair[1], rows[0], rows[1]


def journal(value, write=None, original=None, corpus="ascii", reuse_base="positive"):
    cohort = bases.config(reuse_base); pair = cohort.pair
    expected = end.corpus_config(corpus)
    require(isinstance(value, dict) and set(value) == {"scope", "receipt"} and value["scope"] == shared.SCOPE, "Reuse durable scope changed")
    if write is None:
        require(value["receipt"] is None, "Unexpected reuse lock retained")
        return
    receipt = value["receipt"]
    require(isinstance(receipt, dict) and isinstance(receipt.get("operation"), str) and receipt["operation"], "Reuse durable operation missing")
    c = receipt.get("checkpoint", {})
    require(set(c) == {"version", "scope", "thread", "document", "target", "session", "sequence", "action", "draft", "sections", "request"}
            and type(c.get("version")) is int and c["version"] == 5 and c.get("scope") == shared.SCOPE
            and c.get("thread") == "thread" and c.get("document") == "document" and c.get("target") == pair[0]
            and c.get("session") == write["session"] and type(c.get("sequence")) is int and c["sequence"] == 1
            and c.get("draft") == expected.wire_seed, "Reuse v5 original draft/owner changed")
    require(c["action"] == {"EndReuse": {"child": pair[1], "position": "aag", "offset_utf16": expected.offset}}, "Reuse checkpoint does not retain existing child/full visible offset")
    require(shared.byte_vector(c["request"]) == base64.b64decode(write["documentData"], validate=True), "Reuse durable pair differs from sent pair")
    prior = {r["id"]: base64.b64decode(r["raw"], validate=True) for r in original["sections"]}
    stored = c["sections"]
    require(isinstance(stored, list) and len(stored) == 4
            and {text(wire(shared.byte_vector(row)), 1): shared.byte_vector(row) for row in stored} == prior, "Reuse original four-section inventory changed")


def ack(value, sent, original, partial=False, continuation=False, corpus="ascii", reuse_base="positive"):
    cohort = bases.config(reuse_base); pair = cohort.pair
    expected = end.corpus_config(corpus)
    require(value.get("partial") is partial and value.get("accepted") is (not partial), "Reuse ACK admission changed")
    outer = decoded(value.get("raw")); require([f[0] for f in outer] == [1, 2] and field(outer, 1, 0) == 1, "Reuse ACK malformed")
    data = wire(field(outer, 2, 2))
    require([f[0] for f in data] == [15] + [1] * len(sent) and text(data, 15) == "document", "Reuse full ACK pair missing/duplicated")
    rows = shared.messages(data, 1)
    for row, outgoing in zip(rows, sent):
        identity = text(outgoing, 1); parent = identity == pair[0]; sequence = 3000 if continuation else 2000
        require(text(row, 1) == identity and text(row, 6) == "thread" and text(row, 7) == "document" and field(row, 2, 0) == sequence
                and shared.body(row, parent) == (expected.wire_seed if parent else expected.wire_edited if continuation else ""), "Reuse ACK changed original content/identity")
        current, before = wire(field(row, 38, 2)), wire(field(outgoing, 38, 2))
        require(field(current, 3, 0) == sequence and field(current, 4, 0) > 0 and not any(f[0] in (10, 12) for f in current)
                and shared.stable(current, (3, 4, 12)) == shared.stable(before, (3, 4, 12)), "Reuse ACK changed retained history/base/processed metadata")
        omitted = (2, 12, 33, 38) if continuation else (2, 13, 19, 20, 33, 38)
        require(shared.stable(row, omitted) == shared.stable(original[identity], omitted), "Reuse ACK changed existing content/position/opaque fields")
    return rows


def continuation(value, child_source, corpus="ascii", reuse_base="positive"):
    cohort = bases.config(reuse_base); pair = cohort.pair
    expected = end.corpus_config(corpus)
    data = decoded(value.get("documentData")); shared.rich_title(data)
    require(value.get("kind") == "text" and value.get("parent") == pair[1] and value.get("httpFactoryExecuted") is False
            and value.get("formTitle") == "Fixture title" and type(value.get("sequence")) is int and value["sequence"] == 1
            and re.fullmatch(r"[0-9a-f]{12}", value.get("session", "")) and [f[0] for f in data] == [1, 14], "Reuse continuation not ordinary existing-child Save")
    row = wire(field(data, 1, 2))
    require([f[0] for f in row] == [1, 2, 9, 12, 13, 19, 20, 33, 38] and text(row, 1) == pair[1] and field(row, 2, 0) == 2000
            and field(row, 9, 0) == 0 and shared.body(row, False) == expected.wire_edited and field(row, 13, 2) == b""
            and field(row, 19, 0) == field(row, 20, 0) == 1 and text(row, 33) == value["session"] + "-0", "Reuse child edit identity/body changed")
    current, before = wire(field(row, 38, 2)), wire(field(child_source, 38, 2))
    require(text(current, 1) == pair[1] and field(current, 2, 0) == cohort.base and field(current, 3, 0) == 2000
            and shared.stable(current, (4, 5, 6, 12)) == shared.stable(before, (4, 5, 6, 12))
            and field(current, 5, 2) != field(before, 5, 2) and not any(f[0] == 10 for f in current)
            and re.fullmatch(r"[0-9a-f]{64}", text(current, 12)) and text(current, 12) != text(current, 8)
            and [(f[1], f[2]) for f in current if f[0] == 6]
            == [(f[1], f[2]) for f in before if f[0] == 6] + [(2, field(current, 12, 2))], "Reuse child edit reset original retained-base history")
    if cohort.name == "zero": bases.continuation_history(current, before, expected.wire_edited, value["session"])
    return row


def validate(receipt, corpus="ascii", reuse_base="positive"):
    cohort = bases.config(reuse_base); pair = cohort.pair
    expected = end.corpus_config(corpus); labels(receipt, corpus, reuse_base)
    scenario = receipt.get("scenario")
    require(scenario in shared.STAGES and all(receipt.get(k) is True for k in ("ok", "noSession", "noTransport", "trustedNativeEnter"))
            and receipt.get("httpFactoryExecuted") is False, "Reuse scenario isolation failed")
    original = receipt.get("original", {}); labels(original, corpus, reuse_base)
    initial = original.get("fresh"); initial_rows = inventory(initial, corpus=corpus, reuse_base=reuse_base)
    require(original.get("visible") == initial and original.get("pending") is False and original.get("operation") is None
            and original.get("stale") is False and original.get("loading") is False and original.get("reservations") == 0
            and original.get("capacityReason") is None, "Reuse original state not ready")
    keys = ("preflights", "writes", "acknowledgements", "readbacks")
    require(all(original.get(k) == [] for k in keys), "Reuse already dispatched")
    journal(receipt.get("initialJournal"), corpus=corpus, reuse_base=reuse_base)
    cancelled, saved = receipt.get("cancelledWitness"), receipt.get("savedWitness")
    end.witness(cancelled, corpus); end.witness(saved, corpus)
    require(cancelled["visit"] == saved["visit"] and cancelled["generation"] != saved["generation"]
            and receipt.get("duplicate") == {"ok": True, "clicks": 2}, "Reuse native owner/duplicate guard missing")
    stages = receipt.get("stages")
    require(isinstance(stages, list) and len(stages) == len(shared.STAGES[scenario]), "Reuse stages incomplete")
    previous = {k: [] for k in keys}; owners = {cancelled["generation"], saved["generation"]}
    sent = pair_rows = first_journal = None
    for index, (stage, name) in enumerate(zip(stages, shared.STAGES[scenario])):
        require(stage.get("stage") == name and type(stage.get("ordinal")) is int and stage["ordinal"] == index
                and all(stage.get(k) is True for k in ("ok", "noSession", "noTransport")), "Reuse stage identity changed")
        source, geometry = stage.get("fixture", {}), stage.get("geometry", {}); labels(source, corpus, reuse_base)
        counts = (0, 0, 0, 0) if index < 2 else ((1, 0, 0, 0) if scenario == "stale" else
                 (1, 1, 0, 0) if index == 2 else (1, 1, 1, 0) if scenario == "partial" else
                 (1, 1, 1, 1) if index < 5 else (2, 2, 1, 1) if index == 5 else (2, 2, 2, 2))
        for key, count in zip(keys, counts):
            values = source.get(key)
            require(isinstance(values, list) and len(values) == count and values[:len(previous[key])] == previous[key], "Reuse immutable ledger changed: " + key)
            previous[key] = values
        require(source.get("scenario") == scenario and source.get("visit") == original.get("visit") and type(source.get("read")) is int
                and source["read"] > 0 and source.get("loading") is False and type(source.get("reservations")) is int
                and source["reservations"] == int(scenario != "stale" and index >= 2), "Reuse ownership/dispatch reservation changed")
        if source["preflights"]:
            first = source["preflights"][0]
            require(first.get("source") == initial and first.get("outcome") == ("Changed" if scenario == "stale" else "prepared"), "Reuse fresh preflight not bound")
            inventory(first.get("fresh"), initial, stale=scenario == "stale", corpus=corpus, reuse_base=reuse_base)
            if scenario != "stale": require(first["fresh"] == initial, "Reuse source changed before dispatch")
        if source["writes"] and sent is None:
            _, parent, child = reuse_write(source["writes"][0], initial_rows, corpus, reuse_base); sent = [parent, child]
        accepted = scenario == "success" and index >= 3
        current = inventory(source.get("fresh"), initial, accepted=accepted, edited=index == 6, stale=scenario == "stale" and index >= 2, corpus=corpus, reuse_base=reuse_base)
        inventory(source.get("visible"), initial, accepted=accepted, edited=index == 6, corpus=corpus, reuse_base=reuse_base)
        if not accepted:
            require(source["visible"] == initial, "Reuse unaccepted operation changed visible source")
            if scenario == "stale" and index >= 2:
                require(source["fresh"] == source["preflights"][0]["fresh"]
                        and shared.raw(current[pair[0]]) == shared.raw(initial_rows[pair[0]])
                        and shared.stable(current[pair[1]], (2,)) == shared.stable(initial_rows[pair[1]], (2,)), "Reuse stale source changed beyond existing-child revision")
            else: require(source["fresh"] == initial, "Reuse unaccepted operation changed canonical source")
        if source["acknowledgements"]:
            pair_rows = ack(source["acknowledgements"][0], sent[:1] if scenario == "partial" else sent, initial_rows, partial=scenario == "partial", corpus=corpus, reuse_base=reuse_base)
        if accepted:
            first_read = inventory(source["readbacks"][0], initial, accepted=True, corpus=corpus, reuse_base=reuse_base)
            require(all(shared.raw(first_read[text(row, 1)]) == shared.raw(row) for row in pair_rows), "Reuse independent pair readback differs from ACK")
            require(source["fresh"] == source["visible"] == source["readbacks"][-1], "Reuse accepted source/readback custody differs")
        if index >= 5:
            ack_pair = {text(row, 1): row for row in pair_rows}
            row = continuation(source["writes"][1], ack_pair[pair[1]], corpus, reuse_base)
            require(source["preflights"][1].get("source") == source["preflights"][1].get("fresh") == source["readbacks"][0]
                    and source["preflights"][1].get("outcome") == "prepared", "Reuse continuation preflight changed")
            if index == 6:
                rows = ack(source["acknowledgements"][1], [row], ack_pair, continuation=True, corpus=corpus, reuse_base=reuse_base)
                require(shared.raw(current[pair[1]]) == shared.raw(rows[0]) and shared.raw(current[pair[0]]) == shared.raw(ack_pair[pair[0]]), "Reuse child edit altered parent or lost exact readback")
        pending, uncertain = name in ("saving", "continued-saving"), name == "uncertain"
        require(source.get("pending") is pending and source.get("stale") is (pending or uncertain), "Reuse pending/stale changed")
        if pending or uncertain: require((source.get("operation") or {}).get("stage") == ("Saving" if pending else "Uncertain"), "Reuse operation lock missing")
        else: require(source.get("operation") is None, "Reuse unexpected retained operation")
        held = name == "saving" or uncertain
        journal(stage.get("journal"), source["writes"][0] if held else None, initial, corpus, reuse_base)
        if name == "saving": first_journal = stage["journal"]
        if uncertain: require(stage["journal"] == first_journal, "Reuse partial ACK changed durable pair")
        require(geometry.get("ok") is True and geometry.get("noOverflow") is True, "Reuse layout overflow")
        parts = [expected.seed, ""] if name in ("split", "saving", "refused", "uncertain") else [""] if name == "child-editing" else [expected.edited] if name == "continued-saving" else None
        if parts is not None:
            editor = geometry.get("native")
            shared.native(editor, parts, not (pending or uncertain), shared.ERROR if name == "refused" else None)
            owner = cancelled if name == "split" else saved if name in ("saving", "refused", "uncertain") else None
            if owner:
                require(all(editor.get(k) == owner[k] for k in ("generation", "visit")), "Reuse native owner changed")
                end.placeholder(editor.get("rawBlocks"), editor.get("blocks"), editor.get("placeholder"), expected.seed)
                require(editor["rawBlocks"] == owner["rawAfter"] and editor["blocks"] == owner["after"]
                        and editor["placeholder"] == owner["placeholder"], "Reuse native raw event evidence changed")
            else:
                require(editor.get("rawBlocks") == editor.get("blocks") and editor.get("placeholder") is None, "Reuse ordinary child edit applied end projection")
                if name == "child-editing": require(editor["blocks"][0]["elements"][0]["elements"] == [], "Reused child not ordinary empty editor")
                generation = editor.get("generation")
                require(isinstance(generation, str) and generation.isascii() and generation.isdigit() and generation not in owners
                        and editor.get("visit") == saved["visit"], "Reuse child editor owner recycled")
                owners.add(generation)
        else:
            require(geometry.get("native") is None and geometry.get("paragraphs") == [dict(id="neighbor", text="Another paragraph"),
                    dict(id=pair[0], text=expected.seed), dict(id=pair[1], text=expected.edited if index == 6 else "")], "Reuse reader inserted/lost/reordered existing paragraphs")
        if name == "cancelled": require(source == original, "Reuse Cancel changed source")
    require(receipt.get("fixture") == stages[-1]["fixture"] and receipt.get("layout") == stages[-1]["geometry"]
            and receipt.get("finalJournal") == stages[-1]["journal"], "Reuse terminal differs from original final stage")
