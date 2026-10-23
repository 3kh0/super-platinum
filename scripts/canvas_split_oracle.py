"""Independent raw receipt oracle for the bounded native Canvas split.

CRDT byte relationships are checked here; Automerge history semantics are checked
by the executed product validator and focused core regressions, not this parser.
"""
import base64
import json
import re

from canvas_heading import require, wire, decoded, field, text
from canvas_split_corpus import corpus_config, require_corpus

SEED, LEFT, RIGHT, EDITED = "Keep thisSecond paragraph", "Keep this", "Second paragraph", "Keep this edited"
SCOPE = {"workspace": "T1", "user": "U1", "file": "Ffixture"}
STAGES = {
    "success": ("split", "cancelled", "saving", "confirmed", "child-editing", "continued-saving", "continued-confirmed"),
    "stale": ("split", "cancelled", "refused", "cancelled-stale"),
    "partial": ("split", "cancelled", "saving", "uncertain"),
}
ERROR = "This paragraph or your access changed. Cancel and refresh."


def raw(fields):
    return b"".join(f[3] for f in fields)


def byte_vector(value):
    require(isinstance(value, list) and len(value) <= 1024 * 1024
            and all(type(v) is int and 0 <= v <= 255 for v in value), "Invalid durable byte vector")
    return bytes(value)


def messages(fields, tag):
    return [wire(f[2]) for f in fields if f[0] == tag and f[1] == 2]


def stable(fields, omitted=()):
    # Protobuf order between tags is not semantic; repeated/opaque spans remain exact.
    return {tag: [f[3] for f in fields if f[0] == tag]
            for tag in {f[0] for f in fields} - set(omitted)}


def body(row, language=True):
    outer = wire(field(row, 12, 2))
    require([f[0] for f in outer] == [1], "Wrong split content variant")
    inner = wire(field(outer, 1, 2))
    require(sorted(f[0] for f in inner) == ([1, 2] if language else [1]), "Ambiguous split text/language")
    if language:
        require(text(inner, 2) == "en", "Split language changed")
    return text(inner, 1)


def inventory(value, original=None, child=None, continued=False, stale=False, corpus="ascii"):
    expected = corpus_config(corpus)
    require(isinstance(value, dict) and tuple(value.get(k) for k in ("file", "thread", "document"))
            == ("Ffixture", "thread", "document"), "Canvas owner changed")
    thread, document = decoded(value.get("threadRaw")), decoded(value.get("documentRaw"))
    require(text(thread, 1) == "thread" and text(thread, 10) == "document"
            and text(document, 1) == "document" and text(document, 6) == "thread"
            and text(document, 42) == "xyz"
            and text(wire(field(document, 69, 2)), 1) == "Fixture title", "Original document changed")
    ids = ["title", "paragraph", *([child] if child else []), "neighbor"]
    rows = value.get("sections")
    require(isinstance(rows, list) and [r.get("id") for r in rows] == ids, "Canonical pair order/inventory changed")
    result = {}
    for item, identity in zip(rows, ids):
        row = decoded(item.get("raw"))
        require(text(row, 1) == identity and text(row, 6) == "thread" and text(row, 7) == "document"
                and type(item.get("sequence")) is int and item["sequence"] == field(row, 2, 0), "Canonical row association changed")
        result[identity] = row
    title = result["title"]
    require(field(title, 9, 0) == 64 and field(title, 10, 0) == 48
            and text(wire(field(wire(field(title, 12, 2)), 58, 2)), 1) == "Fixture title"
            and field(title, 2, 0) == 1000 and text(title, 8) == "aaa", "Canonical title changed")
    require(body(result["neighbor"]) == "Another paragraph" and text(result["neighbor"], 8) == "aae"
            and field(result["neighbor"], 2, 0) == 1000
            and next((f[3] for f in result["neighbor"] if f[0] == 1001), None) == b"\xc8\x3e\x81\x00",
            "Original opaque neighbor changed")
    require(body(result["paragraph"]) == (expected.wire_edited if continued else expected.wire_left if child else expected.wire_seed)
            and field(result["paragraph"], 2, 0) == (3000 if continued or stale else 2000 if child else 1000)
            and text(result["paragraph"], 8) == "aac", "Canonical parent changed")
    if child:
        require(body(result[child], False) == expected.wire_right and field(result[child], 2, 0) == 1000
                and text(result[child], 8) == text(result[child], 21) == "aad", "Canonical child changed")
    if original:
        require(value["threadRaw"] == original["threadRaw"] and value["documentRaw"] == original["documentRaw"], "Document source was replaced")
        originals = {r["id"]: r["raw"] for r in original["sections"]}
        require(all(next(r["raw"] for r in rows if r["id"] == identity) == originals[identity]
                    for identity in ("title", "neighbor")), "Untouched source bytes changed")
    return result


def rich_title(data):
    rich = wire(field(data, 14, 2))
    require([f[0] for f in rich] == [1, 2] and text(rich, 1) == "rich_text", "Wrong rich title root")
    section = wire(field(rich, 2, 2))
    require([f[0] for f in section] == [1, 2] and text(section, 1) == "rich_text_section", "Wrong rich title section")
    leaf = wire(field(section, 2, 2))
    require([f[0] for f in leaf] == [1, 2] and text(leaf, 1) == "text" and text(leaf, 2) == "Fixture title", "Wrong independently expected title")


def split_write(value, corpus="ascii"):
    expected = corpus_config(corpus)
    require(value.get("httpFactoryExecuted") is False and value.get("formTitle") == "Fixture title"
            and value.get("parent") == "paragraph" and type(value.get("sequence")) is int
            and value["sequence"] == 1 and re.fullmatch(r"[0-9a-f]{12}", value.get("session", "")), "Wrong split request identity")
    child = value.get("child")
    require(isinstance(child, str) and re.fullmatch(r"temp:C:xyz[0-9a-f]{25}", child), "Wrong new child identity")
    data = decoded(value.get("documentData"))
    require([f[0] for f in data] == [1, 1, 14], "Split is not one complete pair request")
    parent, tail = messages(data, 1)
    require([f[0] for f in parent] == [1, 2, 9, 12, 13, 19, 20, 33, 38]
            and [f[0] for f in tail] == [1, 2, 3, 8, 9, 10, 11, 12, 13, 16, 19, 20, 21, 29, 33, 35, 38], "Unexpected pair request fields")
    require(text(parent, 1) == "paragraph" and field(parent, 2, 0) == 1000 and body(parent) == expected.wire_left
            and text(tail, 1) == child and field(tail, 2, 0) == 0 and body(tail, False) == expected.wire_right
            and text(tail, 8) == text(tail, 21) == "aad:temp", "Wrong split body/order/base")
    for row in (parent, tail):
        require(field(row, 9, 0) == 0 and field(row, 13, 2) == b"" and field(row, 19, 0) == 1
                and field(row, 20, 0) == 1 and text(row, 33) == value["session"] + "-0", "Wrong pair trace/structure")
    for tag in (3, 10, 11, 29, 35):
        require(field(tail, tag, 0) == 0, "New child structure changed")
    require(raw(wire(field(tail, 16, 2))) == b"\x20\x00", "New child attributes changed")
    rich_title(data)
    parent_crdt, child_crdt = (wire(field(row, 38, 2)) for row in (parent, tail))
    for crdt, identity, base in ((parent_crdt, "paragraph", 1000), (child_crdt, child, 0)):
        require(text(crdt, 1) == identity and field(crdt, 2, 0) == field(crdt, 3, 0) == base
                and field(crdt, 5, 2) and re.fullmatch(r"[0-9a-f]{64}", text(crdt, 8))
                and re.fullmatch(r"[0-9a-f]{64}", text(crdt, 12)), "Missing genuine-history wire associations")
        require(all(f[0] in (1, 2, 3, 5, 6, 8, 10, 12) for f in crdt), "Unknown CRDT semantics")
        heads = [f[2].decode("utf-8") for f in crdt if f[0] == 6 and f[1] == 2]
        require(text(crdt, 12) in heads, "Queued head absent from processed wire")
    require(not any(f[0] == 10 for f in child_crdt), "Child received parent lineage")
    lineage = wire(field(parent_crdt, 10, 2))
    require(sorted(f[0] for f in lineage) == list(range(1, 8)) and field(lineage, 1, 0) == 0
            and text(lineage, 2) == value["session"] + "00000000" and text(lineage, 3) == child
            and field(lineage, 4, 0) == expected.wire_offset and text(lineage, 6) == "1000-" + text(parent_crdt, 8), "Wrong original split lineage")
    heads = json.loads(text(lineage, 5))
    require(isinstance(heads, list) and len(heads) == 1 and re.fullmatch(r"[0-9a-f]{64}", heads[0])
            and field(lineage, 7, 2) and field(lineage, 7, 2) != field(parent_crdt, 5, 2)
            and field(parent_crdt, 5, 2) != field(child_crdt, 5, 2), "Pre-split and independent histories lost")
    # Matched original collector evidence: append queued split head plus prior P.
    # Historical receipts with two parent entries remain archived evidence only.
    require([f[2].decode() for f in parent_crdt if f[0] == 6] == heads + [text(parent_crdt,12)] + heads, "Parent processed projection lost original duplicate source head")
    require([f[2].decode() for f in child_crdt if f[0] == 6] == [text(child_crdt,12)], "Child processed projection is not final head only")
    return child, parent, tail, lineage


def ack(value, sent, original, partial=False, continued=False, corpus="ascii"):
    expected = corpus_config(corpus)
    require(value.get("partial") is partial and value.get("accepted") is (not partial), "Wrong ACK admission status")
    parsed = decoded(value.get("raw"))
    require([f[0] for f in parsed] == [1, 2] and field(parsed, 1, 0) == 1, "Wrong original ACK")
    data = wire(field(parsed, 2, 2))
    require(text(data, 15) == "document" and [f[0] for f in data] == [15] + [1] * len(sent), "ACK pair missing/duplicated")
    rows = messages(data, 1)
    for row, outgoing in zip(rows, sent):
        identity = text(outgoing, 1)
        parent = identity == "paragraph"
        require(text(row, 1) == identity and text(row, 6) == "thread" and text(row, 7) == "document"
                and body(row, parent) == (expected.wire_edited if continued else expected.wire_left if parent else expected.wire_right)
                and field(row, 2, 0) == (3000 if continued else 2000 if parent else 1000), "ACK original body/association changed")
        incoming, prior = wire(field(row, 38, 2)), wire(field(outgoing, 38, 2))
        require(field(incoming, 3, 0) == field(row, 2, 0) and field(incoming, 4, 0) > 0
                and not any(f[0] == 12 for f in incoming)
                and stable(incoming, (3, 4, 12)) == stable(prior, (3, 4, 12)), "ACK lost exact sealed history/lineage")
        if parent:
            require(stable(row, (1, 2, 9, 12, 13, 19, 20, 33, 38))
                    == stable(original, (1, 2, 9, 12, 13, 19, 20, 33, 38)), "Parent opaque source changed")
    return rows


def native(value, parts, editable, error=None):
    require(isinstance(value, dict) and value.get("visible") is True and value.get("editable") is editable
            and value.get("saveDisabled") is (not editable) and value.get("cancelDisabled") is (not editable)
            and value.get("error") == error, "Native split controls/draft state changed")
    require(value.get("text") == "".join(parts), "Native split text changed")
    require(plain_sections(value.get("blocks")) == parts, "Native split paragraphs/format changed")


def plain_sections(blocks):
    require(isinstance(blocks, list) and len(blocks) == 1 and blocks[0].get("type") == "rich_text", "Wrong native rich root")
    result = []
    for section in blocks[0].get("elements", []):
        require(section.get("type") == "rich_text_section" and isinstance(section.get("elements"), list), "Wrong native section")
        text_parts = []
        for leaf in section["elements"]:
            require(leaf.get("type") == "text" and isinstance(leaf.get("text"), str)
                    and all(v is False for v in leaf.get("style", {}).values()), "Native text gained formatting/token")
            text_parts.append(leaf["text"])
        result.append("".join(text_parts))
    return result


def witness(value, corpus="ascii"):
    expected = corpus_config(corpus)
    require(isinstance(value, dict) and value.get("beforeInputTrusted") is True and value.get("inputTrusted") is True
            and value.get("inputType") == "insertParagraph" and type(value.get("revision")) is int
            and value["revision"] == 1 and type(value.get("offset")) is int and value["offset"] == expected.visible_offset,
            "Missing original trusted Enter witness")
    require(plain_sections(value.get("before")) == [expected.seed] and plain_sections(value.get("after")) == [expected.left, expected.right], "Witness does not preserve actual split")
    require(all(isinstance(value.get(k), str) and value[k].isascii() and value[k].isdigit()
                for k in ("generation", "visit")), "Witness ownership absent")


def journal(value, write=None, original=None, corpus="ascii"):
    expected_corpus = corpus_config(corpus)
    require(isinstance(value, dict) and set(value) == {"scope", "receipt"} and value.get("scope") == SCOPE, "Durable scope changed")
    if write is None:
        require(value.get("receipt") is None, "Unexpected durable pair retained")
        return
    receipt = value.get("receipt", {})
    require(isinstance(receipt, dict) and isinstance(receipt.get("operation"), str) and receipt["operation"], "Durable operation identity absent")
    c = receipt.get("checkpoint", {})
    require(isinstance(c, dict) and set(c) == {"version", "scope", "thread", "document", "target", "session", "sequence", "action", "draft", "sections", "request"}
            and type(c.get("version")) is int and c["version"] == 3
            and c.get("scope") == SCOPE and c.get("thread") == "thread"
            and c.get("document") == "document" and c.get("target") == "paragraph"
            and c.get("session") == write["session"] and type(c.get("sequence")) is int
            and c["sequence"] == 1 and c.get("draft") == expected_corpus.wire_seed,
            "Durable whole-pair scope/draft changed")
    # The inert checkpoint retains the visible caret; SplitInfo.field4 alone
    # uses the serialized RTML offset. Escaped drafts remain serialized RTML.
    require(c.get("action") == {"Split": {"child": write["child"], "position": "aad:temp", "offset_utf16": expected_corpus.visible_offset,
                                         "parent_draft": expected_corpus.wire_left, "child_draft": expected_corpus.wire_right}}, "Durable split lost one half")
    require(byte_vector(c.get("request")) == base64.b64decode(write["documentData"], validate=True), "Durable request differs from actual dispatch")
    expected = {r["id"]: base64.b64decode(r["raw"], validate=True) for r in original["sections"]}
    stored = c.get("sections", [])
    require(isinstance(stored, list) and len(stored) == len(expected)
            and {text(wire(byte_vector(row)), 1): byte_vector(row) for row in stored} == expected,
            "Durable source inventory changed")


def parent_mode_label(value,parent_mode):
    require(parent_mode in ('bootstrap','retained') and value.get('parentMode', 'bootstrap')==parent_mode, 'Middle parent source cohort changed')


def parent_source_binding(original,parent,lineage,parent_mode):
    sessions=messages(original,38)
    if parent_mode=='bootstrap':
        require(not sessions,'Bootstrap middle source contains retained Session')
        return
    require(len(sessions)==1,'Retained middle source Session missing')
    source=sessions[0];current=wire(field(parent,38,2))
    require(field(source,5,2)==field(lineage,7,2) and field(source,4,0)>0
            and stable(source,(4,5,6,7,10,12))==stable(current,(4,5,6,7,10,12))
            and [f[2].decode() for f in source if f[0]==6]==json.loads(text(lineage,5)),
            'Retained middle split replaced original history/metadata')


def validate(receipt, corpus="ascii", parent_mode="bootstrap"):
    parent_mode_label(receipt,parent_mode)
    expected_corpus = corpus_config(corpus)
    require_corpus(receipt, corpus)
    scenario = receipt.get("scenario")
    require(scenario in STAGES and receipt.get("httpFactoryExecuted") is False
            and receipt.get("trustedNativeEnter") is True, "Invalid split scenario/transport claim")
    original = receipt.get("original", {})
    require_corpus(original, corpus)
    parent_mode_label(original,parent_mode)
    initial = original.get("fresh")
    initial_rows = inventory(initial, corpus=corpus)
    require(original.get("visible") == initial and original.get("pending") is False and original.get("operation") is None
            and original.get("stale") is False and original.get("loading") is False and original.get("reservations") == 0
            and original.get("capacityReason") is None, "Original split state not ready")
    keys = ("preflights", "writes", "acknowledgements", "readbacks")
    require(all(original.get(k) == [] for k in keys), "Fixture already dispatched")
    journal(receipt.get("initialJournal"), corpus=corpus)
    cancelled, saved = receipt.get("cancelledWitness"), receipt.get("savedWitness")
    witness(cancelled, corpus); witness(saved, corpus)
    require(cancelled["visit"] == saved["visit"] and cancelled["generation"] != saved["generation"], "Reopen reused native draft owner")
    require(receipt.get("duplicate") == {"ok": True, "clicks": 2}, "Duplicate Save probe absent")
    stages = receipt.get("stages")
    require(isinstance(stages, list) and len(stages) == len(STAGES[scenario]), "Missing split stages")
    previous = {k: [] for k in keys}
    child = None; sent = None; pair_rows = None; first_journal = None
    owners = {cancelled["generation"], saved["generation"]}
    for index, (stage, name) in enumerate(zip(stages, STAGES[scenario])):
        require(stage.get("stage") == name and type(stage.get("ordinal")) is int and stage["ordinal"] == index
                and all(stage.get(k) is True for k in ("ok", "noSession", "noTransport")), "Stage identity/isolation changed")
        source, geometry = stage.get("fixture", {}), stage.get("geometry", {})
        require_corpus(source, corpus)
        parent_mode_label(source,parent_mode)
        counts = (0, 0, 0, 0) if index < 2 else ((1, 0, 0, 0) if scenario == "stale" else
                 (1, 1, 0, 0) if index == 2 else (1, 1, 1, 0) if scenario == "partial" else
                 (1, 1, 1, 1) if index < 5 else (2, 2, 1, 1) if index == 5 else (2, 2, 2, 2))
        for key, count in zip(keys, counts):
            values = source.get(key)
            require(isinstance(values, list) and len(values) == count and values[:len(previous[key])] == previous[key], "Original receipt ledger changed: " + key)
            previous[key] = values
        require(source.get("scenario") == scenario and source.get("visit") == original.get("visit")
                and type(source.get("read")) is int and source["read"] > 0 and source.get("loading") is False
                and type(source.get("reservations")) is int
                and source["reservations"] == int(scenario != "stale" and index >= 2), "Dispatch/owner/reservation count changed")
        if source["preflights"]:
            first = source["preflights"][0]
            require(first.get("source") == initial and first.get("outcome") == ("Changed" if scenario == "stale" else "prepared"), "Fresh preflight not authoritative")
            inventory(first.get("fresh"), initial, stale=scenario == "stale", corpus=corpus)
            if scenario != "stale": require(first["fresh"] == initial, "Split preflight source replaced")
        if source["writes"] and sent is None:
            child, parent, tail, lineage = split_write(source["writes"][0], corpus); sent = [parent, tail]
            parent_source_binding(initial_rows["paragraph"],parent,lineage,parent_mode)
        accepted = scenario == "success" and index >= 3
        current = inventory(source.get("fresh"), initial, child if accepted else None, continued=index == 6, stale=scenario == "stale" and index >= 2, corpus=corpus)
        visible = inventory(source.get("visible"), initial, child if accepted else None, continued=index == 6, corpus=corpus)
        if not accepted:
            require(source["visible"] == initial, "Unconfirmed operation changed visible canonical source")
            if scenario == "stale" and index >= 2:
                require(source["fresh"] == source["preflights"][0]["fresh"]
                        and stable(current["paragraph"], (2,)) == stable(initial_rows["paragraph"], (2,)),
                        "Stale fixture changed more than original revision")
            else:
                require(source["fresh"] == initial, "Unconfirmed operation changed fresh canonical source")
        if source["acknowledgements"]:
            pair_rows = ack(source["acknowledgements"][0], sent[:1] if scenario == "partial" else sent, initial_rows["paragraph"], partial=scenario == "partial", corpus=corpus)
        if accepted:
            first_read = inventory(source["readbacks"][0], initial, child, corpus=corpus)
            require(all(raw(first_read[text(row, 1)]) == raw(row) for row in pair_rows), "Independent pair readback differs from ACK")
            require(source["fresh"] == source["visible"] == source["readbacks"][-1], "Accepted source/readback custody differs")
        if index >= 5:
            second = source["writes"][1]; data = decoded(second.get("documentData")); row = wire(field(data, 1, 2)); rich_title(data)
            require(second.get("kind") == "text" and second.get("parent") == "paragraph" and second.get("httpFactoryExecuted") is False
                    and second.get("formTitle") == "Fixture title" and [f[0] for f in data] == [1, 14]
                    and type(second.get("sequence")) is int and second["sequence"] == 1
                    and re.fullmatch(r"[0-9a-f]{12}", second.get("session", ""))
                    and text(row, 33) == second["session"] + "-0"
                    and [f[0] for f in row] == [1, 2, 9, 12, 13, 19, 20, 33, 38]
                    and field(row, 9, 0) == 0 and field(row, 13, 2) == b""
                    and field(row, 19, 0) == field(row, 20, 0) == 1
                    and text(row, 1) == "paragraph" and field(row, 2, 0) == 2000 and body(row) == expected_corpus.wire_edited
                    and field(wire(field(row, 38, 2)), 10, 2) == raw(lineage), "Parent continuation lost split lineage/body")
            require(source["preflights"][1].get("source") == source["preflights"][1].get("fresh") == source["readbacks"][0]
                    and source["preflights"][1].get("outcome") == "prepared", "Parent continuation preflight changed")
            if index == 6:
                rows = ack(source["acknowledgements"][1], [row], pair_rows[0], continued=True, corpus=corpus)
                require(raw(current["paragraph"]) == raw(rows[0]) and raw(current[child]) == raw(pair_rows[1]), "Continued readback replaced parent/child")
        pending = name in ("saving", "continued-saving")
        uncertain = name == "uncertain"
        require(source.get("pending") is pending and source.get("stale") is (pending or uncertain), "Pending/stale state differs")
        if pending or uncertain:
            require(source.get("operation", {}).get("stage") == ("Uncertain" if uncertain else "Saving"), "Whole operation lock missing")
        else:
            require(source.get("operation") is None, "Unexpected retained operation")
        held = name == "saving" or uncertain
        journal(stage.get("journal"), source["writes"][0] if held else None, initial, corpus)
        if name == "saving": first_journal = stage["journal"]
        if uncertain: require(stage["journal"] == first_journal, "Partial ACK changed whole durable pair")
        require(geometry.get("ok") is True and geometry.get("noOverflow") is True, "Split layout overflow")
        parts = [expected_corpus.left, expected_corpus.right] if name in ("split", "saving", "refused", "uncertain") else [expected_corpus.right] if name == "child-editing" else [expected_corpus.edited] if name == "continued-saving" else None
        if parts:
            native(geometry.get("native"), parts, not (pending or uncertain), ERROR if name == "refused" else None)
            expected_owner = cancelled if name == "split" else saved if name in ("saving", "refused", "uncertain") else None
            if expected_owner:
                require(all(geometry["native"].get(k) == expected_owner[k] for k in ("generation", "visit")), "Native witness/draft owner changed")
            else:
                owner = geometry["native"].get("generation")
                require(isinstance(owner, str) and owner.isascii() and owner.isdigit() and owner not in owners
                        and geometry["native"].get("visit") == saved["visit"], "Continuation reused native editor owner")
                owners.add(owner)
        else:
            require(geometry.get("native") is None, "Reader still contains editor")
            expected = [{"id": "paragraph", "text": expected_corpus.edited if index == 6 else expected_corpus.left if accepted else expected_corpus.seed}]
            if accepted: expected.append({"id": child, "text": expected_corpus.right})
            expected.append({"id": "neighbor", "text": "Another paragraph"})
            require(geometry.get("paragraphs") == expected, "Visible paragraph order/body differs")
        if name == "cancelled": require(source == original, "Cancel mutated original source")
    require(receipt.get("fixture") == stages[-1]["fixture"] and receipt.get("layout") == stages[-1]["geometry"]
            and receipt.get("finalJournal") == stages[-1]["journal"], "Terminal differs from original captured stage")
