"""Independent typed end-pair checks, including original-runtime history decoding.

The decoder reads the submitted Automerge bytes. Native event claims, final body
text and fixture-provided success flags cannot substitute for that history.
"""
import base64
from dataclasses import dataclass
from functools import lru_cache
import hashlib
from html.parser import HTMLParser
import json
from pathlib import Path
import re
import subprocess

import canvas_end_split_oracle as end
import canvas_end_reuse_oracle as reuse
import canvas_split_oracle as shared
import canvas_reuse_base as bases
from canvas_heading import require, wire, decoded, field, text


@dataclass(frozen=True)
class Config:
    corpus: str
    mode: str
    typing: str
    reuse_base: str
    child: str
    base: int
    seed: str
    wire_seed: str
    offset: int
    updates: tuple
    continued: str


def config(corpus="ascii", mode="end", typing="correction", reuse_base="positive"):
    source = end.corpus_config(corpus); cohort = bases.config(reuse_base)
    require(reuse_base == "positive" or mode == "reuse", "Zero-base source requires existing-child reuse")
    require(mode in ("end", "reuse") and typing in ("correction", "erase"), "Unpinned typed end selection")
    updates = ("x", "") if typing == "erase" else (("New", "New draftx", "New draft", "New paragraph") if corpus == "ascii"
               else ("New", "New 日本😀 draftx", "New 日本😀 draft", "New 日本😀 paragraph"))
    return Config(corpus, mode, typing, reuse_base, cohort.child, cohort.base, source.seed, source.wire_seed, source.offset, updates,
                  "Continued paragraph" if corpus == "ascii" else "Continued 日本😀 paragraph")


def labels(value, expected, recovery=False):
    bases.labels(value, bases.config(expected.reuse_base))
    require(value.get("splitMode" if recovery else "mode") == expected.mode and value.get("corpus") == expected.corpus
            and value.get("typing") == expected.typing, "Typed receipt caller selection mismatch")


def utf16(value):
    return len(value.encode("utf-16-le")) // 2


def tags(fields):
    # Singular protobuf fields may occur in any wire order. Preserve repeated
    # member/head order in messages()/stable(); compare multiplicity here.
    return sorted(f[0] for f in fields)


@lru_cache(maxsize=64)
def decode_history(doc, original=None):
    values = (doc,) if original is None else (doc, original)
    require(all(isinstance(v, bytes) and 0 < len(v) <= 1024 * 1024 for v in values), "Typed history byte bound")
    script = Path(__file__).with_name("canvas_end_typing_history.mjs")
    result = subprocess.run(["node", str(script)], input=json.dumps([base64.b64encode(v).decode() for v in values]),
                            text=True, encoding="utf-8", capture_output=True, timeout=30)
    require(result.returncode == 0, "Original Automerge history decoder rejected fixture bytes: " + result.stderr[:500])
    require(len(result.stdout) <= 16 * 1024 * 1024, "Decoded history output bound")
    reports = json.loads(result.stdout)
    require(isinstance(reports, list) and len(reports) == len(values), "History decoder result association lost")
    for report, value in zip(reports, values):
        require(report.get("sha256") == hashlib.sha256(value).hexdigest(), "History decoder document association lost")
    return reports


def semantic_history(current, source, updates, session, *, new=False):
    """Walk real historical views; only the known plain, linear cohort is allowed."""
    doc = field(current, 5, 2); actor = session + "00000000"
    reports = decode_history(doc, None if new else field(source, 5, 2))
    report = reports[0]; changes = report.get("changes")
    require(isinstance(changes, list) and changes, "Missing actual Automerge history")
    prefix = changes[:1] if new else reports[1]["changes"]
    require(prefix and len(changes) > len(prefix), "Typed operation lost all effective edits")
    if not new:
        require([c["raw"] for c in changes[:len(prefix)]] == [c["raw"] for c in prefix], "Typed child reset/reordered original history")
        require(reports[1]["heads"] == [prefix[-1]["hash"]], "Original child is not the admitted clean linear history")
        require(text(current, 8) == text(source, 8) and text(source, 8) in {c["hash"] for c in prefix}
                and all(f[2].decode() in {c["hash"] for c in prefix} for f in source if f[0] == 6), "Typed original metadata heads do not name retained history")
    require(prefix[-1]["text"] == "\ufeff" and prefix[-1]["deleted"] is False and prefix[-1]["marks"] == [], "Typed child did not start empty")
    if new:
        first = prefix[0]
        require(first["deps"] == [] and first["seq"] == 1 and first["actor"] == actor and first["keys"] == ["deleted", "text"], "New child bootstrap owner/shape changed")
        require(text(current, 8) == first["hash"], "New child base head is not bootstrap")
    actual = changes[len(prefix):]
    # Filled from source-equivalent splice boundaries: a replacement may have a
    # deletion and insertion change, both of which must remain in the document.
    expected_states = mutation_states(updates)
    require([c.get("text") for c in actual] == [s + "\ufeff" for s in expected_states], "Typed ordered history omitted/reordered/squashed a mutation")
    require([c.get("patches") for c in actual] == [[p] for p in mutation_patches(updates)], "Typed history changed original per-patch mutation order")
    previous = prefix[-1]; actor_seq = max((c["seq"] for c in prefix if c["actor"] == actor), default=0)
    for change in actual:
        actor_seq += 1
        require(change["actor"] == actor and change["seq"] == actor_seq and change["deps"] == [previous["hash"]]
                and change["deleted"] is False and change["keys"] == ["deleted", "text"] and change["marks"] == [], "Typed actor/dependency/plain-text custody changed")
        require(change["ops"] and all(op.get("action") in ("set", "del") and op.get("obj") != "_root" for op in change["ops"]), "Typed history changed document structure")
        previous = change
    require(report["heads"] == [previous["hash"]] and text(current, 12) == previous["hash"], "Typed current head not final actual history head")
    return report


def mutation_states(updates):
    """Exact original four-patch replacement vectors, not generic diff guesses."""
    if tuple(updates) == ("x", ""): return ["x", ""]
    for prefix in ("New ", "New 日本😀 "):
        if tuple(updates) == ("New", prefix + "draftx", prefix + "draft", prefix + "paragraph"):
            return ["New", prefix + "draftx", prefix + "draft", prefix + "paragdraft", prefix + "paragraft", prefix + "paragra", prefix + "paragraph"]
    raise RuntimeError("Unknown independently pinned mutation sequence")


def mutation_patches(updates):
    if tuple(updates) == ("x", ""):
        return [dict(action="splice", path=["text", 0], value="x"), dict(action="del", path=["text", 0])]
    mutation_states(updates)  # Reject any unpinned sequence before deriving offsets.
    start = 4 if updates[1] == "New draftx" else 9
    return [dict(action="splice", path=["text", 0], value="New"),
            dict(action="splice", path=["text", 3], value=" draftx" if start == 4 else " 日本😀 draftx"),
            dict(action="del", path=["text", start + 5]),
            dict(action="splice", path=["text", start], value="parag"),
            dict(action="del", path=["text", start + 5]),
            dict(action="del", path=["text", start + 7], length=2),
            dict(action="splice", path=["text", start + 7], value="ph")]


def inventory(value, expected, original=None, child=None, accepted=False, continued=False, stale=False):
    if not accepted:
        return (reuse.inventory(value, original, stale=stale, corpus=expected.corpus, reuse_base=expected.reuse_base) if expected.mode == "reuse"
                else end.inventory(value, original, stale=stale, corpus=expected.corpus))
    ids = ["title", "neighbor", "paragraph", expected.child] if expected.mode == "reuse" else ["title", "paragraph", child, "neighbor"]
    require(isinstance(value, dict) and all(value.get(k) == original[k] for k in original if k != "sections"), "Typed canonical owner/document changed")
    rows = value.get("sections"); require(isinstance(rows, list) and [r.get("id") for r in rows] == ids, "Typed canonical inventory identity/order changed")
    prior = {r["id"]: r for r in original["sections"]}; result = {}
    for item in rows:
        identity = item["id"]; row = decoded(item.get("raw")); result[identity] = row
        require(text(row, 1) == identity and text(row, 6) == "thread" and text(row, 7) == "document"
                and type(item.get("sequence")) is int and item["sequence"] == field(row, 2, 0), "Typed canonical row association changed")
        if identity in ("title", "neighbor"): require(item == prior[identity], "Typed edit changed untouched inventory bytes")
        else:
            parent = identity == "paragraph"
            require(shared.body(row, parent) == (expected.wire_seed if parent else expected.continued if continued else expected.updates[-1])
                    and field(row, 2, 0) == (2000 if parent else 3000 if continued else 2000 if expected.mode == "reuse" else 1000), "Typed canonical body/revision changed")
            position = ("aae" if parent else "aag") if expected.mode == "reuse" else ("aac" if parent else "aad")
            require(text(row, 8) == position, "Typed canonical position changed")
    return result


def typed_write(value, original, expected):
    require(value.get("kind") == expected.mode and value.get("httpFactoryExecuted") is False and value.get("formTitle") == "Fixture title"
            and value.get("parent") == "paragraph" and type(value.get("sequence")) is int and value["sequence"] == 1
            and re.fullmatch(r"[0-9a-f]{12}", value.get("session", "")), "Typed pair request identity changed")
    child = value.get("child"); is_reuse = expected.mode == "reuse"
    require(child == expected.child if is_reuse else isinstance(child, str) and re.fullmatch(r"temp:C:xyz[0-9a-f]{25}", child), "Typed child identity changed")
    data = decoded(value.get("documentData")); shared.rich_title(data)
    require(tags(data) == [1, 1, 14], "Typed Save is not one whole pair")
    parent, tail = shared.messages(data, 1)
    require(tags(parent) == [1, 2, 13, 19, 20, 33, 38] and text(parent, 1) == "paragraph" and field(parent, 2, 0) == 1000, "Typed parent not unchanged STATUS projection")
    require(tags(tail) == ([1, 2, 9, 12, 13, 19, 20, 33, 38] if is_reuse else [1, 2, 3, 8, 9, 10, 11, 12, 13, 16, 19, 20, 21, 29, 33, 35, 38])
            and text(tail, 1) == child and field(tail, 2, 0) == (1000 if is_reuse else 0)
            and field(tail, 9, 0) == 0 and shared.body(tail, False) == expected.updates[-1], "Typed child full CONTENT projection/body changed")
    for row in (parent, tail):
        require(field(row, 13, 2) == b"" and field(row, 19, 0) == field(row, 20, 0) == 1 and text(row, 33) == value["session"] + "-0", "Typed trace/status changed")
    if not is_reuse:
        require(text(tail, 8) == text(tail, 21) == "aad:temp" and field(tail, 16, 2) == b"\x20\x00"
                and all(field(tail, tag, 0) == 0 for tag in (3, 10, 11, 29, 35)), "Typed new child creation fields changed")
    before = wire(field(original["paragraph"], 38, 2)); parent_crdt = wire(field(parent, 38, 2)); current = wire(field(tail, 38, 2))
    require(shared.stable(parent_crdt) == shared.stable(before, (4, 7, 12)) and not any(f[0] in (4, 7, 10, 12) for f in parent_crdt), "Typed pair fabricated parent history")
    require(text(current, 1) == child and field(current, 2, 0) == (expected.base if is_reuse else 0) and field(current, 3, 0) == (1000 if is_reuse else 0)
            and not any(f[0] in (4, 7, 10) for f in current), "Typed child CRDT owner/base changed")
    source = wire(field(original[child], 38, 2)) if is_reuse else None
    if expected.reuse_base == "zero": bases.zero_source(original[child])
    processed = [f[2] for f in current if f[0] == 6]
    if is_reuse:
        require(shared.stable(current, (5, 6, 12)) == shared.stable(source, (4, 5, 6, 7, 12)), "Typed reuse lost original session/base metadata")
        prior = [f[2] for f in source if f[0] == 6]
        require(processed == prior + [field(current, 12, 2)] + prior, "Typed reuse lost ordered duplicate save heads")
    else:
        require(tags(current) == [1, 2, 3, 5, 6, 8, 12] and processed == [field(current, 12, 2)], "Typed new child head projection changed")
    semantic_history(current, source, expected.updates, value["session"], new=not is_reuse)
    return child, parent, tail


def journal(value, expected, write=None, original=None):
    require(isinstance(value, dict) and set(value) == {"scope", "receipt"} and value["scope"] == shared.SCOPE, "Typed durable scope changed")
    if write is None:
        require(value["receipt"] is None, "Unexpected retained typed lock"); return
    receipt = value["receipt"]
    require(isinstance(receipt, dict) and isinstance(receipt.get("operation"), str) and receipt["operation"], "Typed operation identity missing")
    c = receipt.get("checkpoint", {})
    require(set(c) == {"version", "scope", "thread", "document", "target", "session", "sequence", "action", "draft", "sections", "request"}
            and type(c.get("version")) is int and c["version"] == 6 and c.get("scope") == shared.SCOPE and c.get("thread") == "thread"
            and c.get("document") == "document" and c.get("target") == "paragraph" and c.get("session") == write["session"]
            and type(c.get("sequence")) is int and c["sequence"] == 1 and c.get("draft") == expected.wire_seed, "Typed v6 checkpoint owner/original parent changed")
    require(c["action"] == {"EndTyping": dict(child=write["child"], position="aag" if expected.mode == "reuse" else "aad:temp",
              offset_utf16=expected.offset, reused=expected.mode == "reuse", child_updates=list(expected.updates))}, "Typed checkpoint lost ordered updates/end intent")
    action = c["action"]["EndTyping"]
    require(type(action["offset_utf16"]) is int and type(action["reused"]) is bool
            and isinstance(action["child_updates"], list) and all(type(s) is str for s in action["child_updates"]), "Typed checkpoint action scalar types changed")
    require(shared.byte_vector(c["request"]) == base64.b64decode(write["documentData"], validate=True), "Typed durable request differs from sent whole pair")
    prior = {r["id"]: base64.b64decode(r["raw"], validate=True) for r in original["sections"]}
    require(isinstance(c["sections"], list) and len(c["sections"]) == len(prior)
            and {text(wire(shared.byte_vector(r)), 1): shared.byte_vector(r) for r in c["sections"]} == prior, "Typed durable original inventory changed")


class PlainDom(HTMLParser):
    def __init__(self, value):
        super().__init__(convert_charrefs=True)
        self.root = []; self.stack = [self.root]
        require(isinstance(value, str) and len(value) <= 131072, "Typed DOM byte bound")
        self.feed(value); self.close(); require(len(self.stack) == 1, "Unclosed typed DOM")

    def handle_starttag(self, tag, attrs):
        require(tag in ("div", "span", "br"), "Typed DOM gained semantic/rich element")
        node = [tag, attrs, []]; self.stack[-1].append(node)
        if tag != "br": self.stack.append(node[2])

    def handle_endtag(self, tag):
        require(len(self.stack) > 1 and self.stack[-2][-1][0] == tag, "Mismatched typed DOM")
        self.stack.pop()

    def handle_data(self, data): self.stack[-1].append(data)
    def handle_comment(self, data): raise RuntimeError("Typed DOM comment not plain source")


def plain_dom(value, expected, child_text, placeholder):
    root = PlainDom(value).root
    require(len(root) == 1 and root[0][:2] == ["div", [("class", "rich-editor-block")]], "Typed DOM block structure changed")
    sections = root[0][2]
    require(len(sections) == 2 and all(isinstance(n, list) and n[:2] == ["div", [("class", "rich-editor-section")]] for n in sections), "Typed DOM section count/structure changed")
    def visible(nodes, depth=0):
        require(depth <= 16, "Typed DOM nesting exceeds bound")
        output = ""
        for node in nodes:
            if isinstance(node, str): output += node
            else:
                require(node[:2] == ["span", []], "Typed plain text contains formatting/break")
                output += visible(node[2], depth + 1)
        return output
    require(visible(sections[0][2]) == expected.seed, "Typed DOM parent changed")
    if child_text:
        require(placeholder is None and visible(sections[1][2]) == child_text, "Typed DOM child differs from shared projection")
    else:
        require(isinstance(placeholder, dict) and placeholder.get("blockCount") == 1 and placeholder.get("sectionCount") == 2
                and placeholder.get("caretOffset") == 0, "Typed empty child lacks exact placeholder")
        nodes = sections[1][2]; depth = 0
        while len(nodes) == 1 and isinstance(nodes[0], list) and nodes[0][:2] == ["span", []]: nodes = nodes[0][2]; depth += 1
        require(nodes == [["br", [], []]] and type(placeholder.get("spanDepth")) is int and placeholder["spanDepth"] == depth
                and placeholder.get("childHtml") == "<span>" * depth + "<br>" + "</span>" * depth, "Typed empty projection hid real content/multiple breaks")
    return sections[0]


def typed_snapshot(value, expected, child_text):
    require(shared.plain_sections(value.get("after")) == [expected.seed, child_text], "Typed event snapshot changed parent/child")
    raw = value.get("rawAfter")
    if child_text:
        require(raw == value["after"] and value.get("placeholder") is None, "Nonempty child applied empty placeholder projection")
    else:
        require(shared.plain_sections(raw) in ([expected.seed, ""], [expected.seed, "\n"])
                and raw[0]["elements"][0] == value["after"][0]["elements"][0]
                and value["after"][0]["elements"][1]["elements"] == [], "Empty typed raw-to-canonical projection changed")
        require(raw[0]["elements"][1]["elements"] in ([], [{"type": "text", "text": "\n"}]), "Empty typed raw snapshot contains hidden runs")
    return plain_dom(value.get("html"), expected, child_text, value.get("placeholder"))


def witness(value, expected):
    end.witness(value, expected.corpus)
    typed = value.get("typing", {}); events = typed.get("events")
    require(isinstance(events, list) and len(events) == len(expected.updates), "Typed native ordered events absent")
    before = ""; parent_dom = None
    for index, (event, after) in enumerate(zip(events, expected.updates)):
        insert = index < 2 if expected.typing == "correction" else index == 0
        if expected.typing == "correction" and index == 3:
            kind, data, start, stop = "insertText", "paragraph", utf16(before) - 5, utf16(before)
            selected = (start, stop)
        elif insert:
            kind, data, start, stop = "insertText", after[len(before):], utf16(before), utf16(before)
            selected = (start, stop)
        else:
            kind, data, start, stop = "deleteContentBackward", None, utf16(before) - 1, utf16(before)
            selected = (stop, stop)
        expected_fields = dict(inputType=kind, data=data, selectionStart=selected[0], selectionEnd=selected[1], start=start, end=stop,
                               beforeText=before, afterText=after, beforeRevision=index + 1, revision=index + 2, caret=start + utf16(data or ""),
                               beforeInputTrusted=True, inputTrusted=True)
        require(all(event.get(k) == v and type(event.get(k)) is type(v) for k, v in expected_fields.items()), "Typed native event/range/revision differs from pinned action sequence")
        parent = typed_snapshot(event, expected, after)
        if parent_dom is None: parent_dom = parent
        require(parent == parent_dom, "Typed native event changed parent DOM")
        before = after
    require(type(typed.get("revision")) is int and typed["revision"] == len(events) + 1
            and all(typed.get(k) == events[-1].get(k) for k in ("after", "rawAfter", "html", "placeholder")), "Typed final snapshot not final owned event")
    return typed


def ack(value, sent, original, expected, partial=False, continued=False):
    require(value.get("partial") is partial and value.get("accepted") is (not partial), "Typed ACK admission changed")
    outer = decoded(value.get("raw")); require(tags(outer) == [1, 2] and field(outer, 1, 0) == 1, "Typed ACK malformed")
    data = wire(field(outer, 2, 2)); require(tags(data) == [1] * len(sent) + [15] and text(data, 15) == "document", "Typed complete ACK pair missing/duplicated")
    rows = shared.messages(data, 1)
    for row, outgoing in zip(rows, sent):
        identity = text(outgoing, 1); parent = identity == "paragraph"
        sequence = 3000 if continued else 2000 if parent or expected.mode == "reuse" else 1000
        require(text(row, 1) == identity and text(row, 6) == "thread" and text(row, 7) == "document" and field(row, 2, 0) == sequence
                and shared.body(row, parent) == (expected.wire_seed if parent else expected.continued if continued else expected.updates[-1]), "Typed ACK body/identity/revision changed")
        current, prior = wire(field(row, 38, 2)), wire(field(outgoing, 38, 2))
        require(field(current, 3, 0) == sequence and field(current, 4, 0) > 0 and not any(f[0] in (10, 12) for f in current)
                and shared.stable(current, (3, 4, 12)) == shared.stable(prior, (3, 4, 12)), "Typed ACK lost exact submitted history/head order")
        if parent:
            omitted = (2, 13, 19, 20, 33, 38)
            require(shared.stable(row, omitted) == shared.stable(original[identity], omitted), "Typed parent ACK changed original bytes")
        elif continued or expected.mode == "reuse":
            omitted = (2, 9, 12, 13, 19, 20, 33, 38)
            require(shared.stable(row, omitted) == shared.stable(original[identity], omitted), "Typed existing-child ACK changed structure/opaque bytes")
        else:
            omitted = (2, 6, 7, 8, 21, 38)
            require(shared.stable(row, omitted) == shared.stable(outgoing, omitted) and text(row, 8) == text(row, 21) == "aad", "Typed new child ACK changed creation fields")
    return rows


def continuation(value, child, source, expected):
    data = decoded(value.get("documentData")); shared.rich_title(data)
    require(value.get("kind") == "text" and value.get("parent") == child and value.get("httpFactoryExecuted") is False
            and value.get("formTitle") == "Fixture title" and type(value.get("sequence")) is int and value["sequence"] == 1
            and re.fullmatch(r"[0-9a-f]{12}", value.get("session", "")) and tags(data) == [1, 14], "Typed continuation is not ordinary child Save")
    row = wire(field(data, 1, 2)); before = wire(field(source, 38, 2)); current = wire(field(row, 38, 2))
    require(tags(row) == [1, 2, 9, 12, 13, 19, 20, 33, 38] and text(row, 1) == child
            and field(row, 2, 0) == (2000 if expected.mode == "reuse" else 1000) and field(row, 9, 0) == 0
            and shared.body(row, False) == expected.continued and field(row, 13, 2) == b"" and field(row, 19, 0) == field(row, 20, 0) == 1
            and text(row, 33) == value["session"] + "-0", "Typed ordinary continuation body/owner changed")
    require(shared.stable(current, (4, 5, 6, 12)) == shared.stable(before, (4, 5, 6, 12)) and not any(f[0] in (4, 10) for f in current)
            and [f[2] for f in current if f[0] == 6] == [f[2] for f in before if f[0] == 6] + [field(current, 12, 2)], "Typed continuation lost original session/processed custody")
    now, prior = decode_history(field(current, 5, 2), field(before, 5, 2))
    require(len(now["changes"]) > len(prior["changes"]) and [c["raw"] for c in now["changes"][:len(prior["changes"])]] == [c["raw"] for c in prior["changes"]]
            and now["heads"] == [text(current, 12)] and now["changes"][-1]["text"] == expected.continued + "\ufeff"
            and now["changes"][-1]["deleted"] is False and now["changes"][-1]["marks"] == [], "Typed continuation erased earlier actual mutation history")
    previous = prior["changes"][-1]; actor = value["session"] + "00000000"
    sequence = max((c["seq"] for c in prior["changes"] if c["actor"] == actor), default=0)
    for change in now["changes"][len(prior["changes"]):]:
        sequence += 1
        require(change["actor"] == actor and change["seq"] == sequence and change["deps"] == [previous["hash"]]
                and change["deleted"] is False and change["keys"] == ["deleted", "text"] and change["marks"] == [], "Typed continuation actor/plain history changed")
        previous = change
    return row


def validate(receipt, corpus="ascii", mode="end", typing="correction", reuse_base="positive"):
    expected = config(corpus, mode, typing, reuse_base); labels(receipt, expected)
    scenario = receipt.get("scenario")
    require(scenario in shared.STAGES and all(receipt.get(k) is True for k in ("ok", "noSession", "noTransport", "trustedNativeEnter"))
            and receipt.get("httpFactoryExecuted") is False, "Typed scenario isolation failed")
    original = receipt.get("original", {}); labels(original, expected)
    initial = original.get("fresh"); initial_rows = inventory(initial, expected)
    require(original.get("visible") == initial and original.get("pending") is False and original.get("operation") is None
            and original.get("stale") is False and original.get("loading") is False and original.get("reservations") == 0
            and original.get("capacityReason") is None, "Typed original state not ready")
    keys = ("preflights", "writes", "acknowledgements", "readbacks")
    require(all(original.get(k) == [] for k in keys), "Typed fixture already dispatched")
    journal(receipt.get("initialJournal"), expected)
    cancelled, saved = receipt.get("cancelledWitness"), receipt.get("savedWitness")
    witness(cancelled, expected); witness(saved, expected)
    require(cancelled["visit"] == saved["visit"] and cancelled["generation"] != saved["generation"]
            and receipt.get("duplicate") == {"ok": True, "clicks": 2}, "Typed native owner/duplicate Save guard missing")
    names = ("typed-child",) + shared.STAGES[scenario][1:]; stages = receipt.get("stages")
    require(isinstance(stages, list) and len(stages) == len(names), "Typed lifecycle stages incomplete")
    previous = {k: [] for k in keys}; owners = {cancelled["generation"], saved["generation"]}
    child = expected.child if mode == "reuse" else None
    sent = pair_rows = first_journal = None
    for index, (stage, name) in enumerate(zip(stages, names)):
        require(stage.get("stage") == name and type(stage.get("ordinal")) is int and stage["ordinal"] == index
                and all(stage.get(k) is True for k in ("ok", "noSession", "noTransport")), "Typed stage identity changed")
        source, geometry = stage.get("fixture", {}), stage.get("geometry", {}); labels(source, expected)
        counts = (0, 0, 0, 0) if index < 2 else ((1, 0, 0, 0) if scenario == "stale" else
                 (1, 1, 0, 0) if index == 2 else (1, 1, 1, 0) if scenario == "partial" else
                 (1, 1, 1, 1) if index < 5 else (2, 2, 1, 1) if index == 5 else (2, 2, 2, 2))
        for key, count in zip(keys, counts):
            values = source.get(key)
            require(isinstance(values, list) and len(values) == count and values[:len(previous[key])] == previous[key], "Typed immutable ledger changed: " + key)
            previous[key] = values
        require(source.get("scenario") == scenario and source.get("visit") == original.get("visit") and type(source.get("read")) is int
                and source["read"] > 0 and source.get("loading") is False and type(source.get("reservations")) is int
                and source["reservations"] == int(scenario != "stale" and index >= 2), "Typed ownership/dispatch reservation changed")
        if source["preflights"]:
            first = source["preflights"][0]
            require(first.get("source") == initial and first.get("outcome") == ("Changed" if scenario == "stale" else "prepared"), "Typed preflight not original source")
            fresh = inventory(first.get("fresh"), expected, initial, stale=scenario == "stale")
            if scenario != "stale": require(first["fresh"] == initial, "Typed source changed before dispatch")
            else:
                identity = expected.child if mode == "reuse" else "paragraph"
                require(shared.stable(fresh[identity], (2,)) == shared.stable(initial_rows[identity], (2,))
                        and all(shared.raw(fresh[k]) == shared.raw(v) for k, v in initial_rows.items() if k != identity), "Typed stale source changed beyond intended revision")
        if source["writes"] and sent is None:
            child, parent, tail = typed_write(source["writes"][0], initial_rows, expected); sent = [parent, tail]
        accepted = scenario == "success" and index >= 3
        current = inventory(source.get("fresh"), expected, initial, child, accepted, index == 6, scenario == "stale" and index >= 2)
        inventory(source.get("visible"), expected, initial, child, accepted, index == 6)
        if not accepted:
            require(source["visible"] == initial and source["fresh"] == (source["preflights"][0]["fresh"] if scenario == "stale" and index >= 2 else initial), "Typed unaccepted operation changed canonical/visible source")
        if source["acknowledgements"]:
            pair_rows = ack(source["acknowledgements"][0], sent[:1] if scenario == "partial" else sent, initial_rows, expected, partial=scenario == "partial")
        if accepted:
            first_read = inventory(source["readbacks"][0], expected, initial, child, True)
            require(all(shared.raw(first_read[text(row, 1)]) == shared.raw(row) for row in pair_rows), "Typed independent readback differs from ACK")
            require(source["fresh"] == source["visible"] == source["readbacks"][-1], "Typed readback source custody differs")
        if index >= 5:
            pair = {text(row, 1): row for row in pair_rows}; row = continuation(source["writes"][1], child, pair[child], expected)
            require(source["preflights"][1].get("source") == source["preflights"][1].get("fresh") == source["readbacks"][0]
                    and source["preflights"][1].get("outcome") == "prepared", "Typed continuation preflight changed")
            if index == 6:
                rows = ack(source["acknowledgements"][1], [row], pair, expected, continued=True)
                require(shared.raw(current[child]) == shared.raw(rows[0]) and shared.raw(current["paragraph"]) == shared.raw(pair["paragraph"]), "Typed ordinary child continuation changed parent/readback")
        pending, uncertain = name in ("saving", "continued-saving"), name == "uncertain"
        require(source.get("pending") is pending and source.get("stale") is (pending or uncertain), "Typed pending/stale state changed")
        if pending or uncertain: require((source.get("operation") or {}).get("stage") == ("Saving" if pending else "Uncertain"), "Typed operation lock missing")
        else: require(source.get("operation") is None, "Typed unexpected retained operation")
        held = name == "saving" or uncertain
        journal(stage.get("journal"), expected, source["writes"][0] if held else None, initial)
        if name == "saving": first_journal = stage["journal"]
        if uncertain: require(stage["journal"] == first_journal, "Typed unknown outcome lost whole ordered operation")
        require(geometry.get("ok") is True and geometry.get("noOverflow") is True, "Typed layout overflow")
        typed_editor = name in ("typed-child", "saving", "refused", "uncertain")
        parts = [expected.seed, expected.updates[-1]] if typed_editor else [expected.updates[-1]] if name == "child-editing" else [expected.continued] if name == "continued-saving" else None
        if parts is not None:
            editor = geometry.get("native")
            shared.native(editor, parts, not (pending or uncertain), shared.ERROR if name == "refused" else None)
            if typed_editor:
                owner = cancelled if name == "typed-child" else saved; typed = owner["typing"]
                require(all(editor.get(k) == owner[k] for k in ("generation", "visit")) and editor.get("typing") == typed
                        and editor.get("blocks") == typed["after"] and editor.get("rawBlocks") == typed["rawAfter"]
                        and editor.get("placeholder") == typed["placeholder"], "Typed held draft lost sealed native event association")
            else:
                require(editor.get("rawBlocks") == editor.get("blocks") and editor.get("placeholder") is None and editor.get("typing") is None, "Ordinary child reopened with stale typed witness")
                generation = editor.get("generation")
                require(isinstance(generation, str) and generation.isascii() and generation.isdigit() and generation not in owners
                        and editor.get("visit") == saved["visit"], "Typed child editor recycled old owner")
                owners.add(generation)
        else:
            paragraphs = [dict(id="paragraph", text=expected.seed)]
            if accepted or mode == "reuse": paragraphs.append(dict(id=child, text=expected.continued if index == 6 else expected.updates[-1] if accepted else ""))
            neighbor = dict(id="neighbor", text="Another paragraph")
            paragraphs = [neighbor, *paragraphs] if mode == "reuse" else [*paragraphs, neighbor]
            require(geometry.get("native") is None and geometry.get("paragraphs") == paragraphs, "Typed reader content/order changed")
        if name == "cancelled": require(source == original, "Typed Cancel mutated source or dispatched")
    require(receipt.get("fixture") == stages[-1]["fixture"] and receipt.get("layout") == stages[-1]["geometry"]
            and receipt.get("finalJournal") == stages[-1]["journal"], "Typed terminal differs from final original stage")
