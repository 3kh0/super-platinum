"""Independent closed H1–H3 receipt oracle; protobuf bytes retain field presence."""
import base64
import html
import math
import re
from html.parser import HTMLParser

STAGES = ("initial", "cancel-draft", "stale-draft", "h1-saving", "h1-confirmed",
          "h2-confirmed", "h3-confirmed", "uncertain", "readback-once", "readback-twice")
IDS = ("title", "heading-one", "heading-two", "heading-three", "heading-twin", "neighbor")
LEVELS = (48, 1, 2, 3, 2, 0)
INITIAL = (None, "Shared <b>heading</b> 中🙂", "Shared <b>heading</b> 中🙂",
           "Third <i>heading</i> 日🦊", "Shared <b>heading</b> 中🙂",
           "Untouched <i>neighbor</i> &amp; details.")
DESIRED = ("<b>First 中🙂</b><br><br><i>Second 日🦊</i> &amp; &lt;literal&gt;",
           "Second <i>heading 日🦊</i> &amp; details", "Third <b>heading 中🙂</b> &lt;done&gt;",
           "<b>Unconfirmed 中🙂</b> keep this")


def require(value, message):
    if not value:
        raise RuntimeError(message)


def wire(data):
    """Decode only bounded fixture wire kinds, preserving each original field span."""
    require(isinstance(data, bytes) and len(data) <= 1024 * 1024, "Invalid heading wire")
    offset = 0
    out = []

    def varint():
        nonlocal offset
        value = 0
        for shift in range(0, 70, 7):
            require(offset < len(data), "Truncated heading varint")
            byte = data[offset]
            offset += 1
            value |= (byte & 127) << shift
            if byte < 128:
                return value
        raise RuntimeError("Oversized heading varint")

    while offset < len(data):
        start = offset
        key = varint()
        tag, kind = key >> 3, key & 7
        require(tag > 0 and kind in (0, 1, 2, 5), "Unsupported heading wire field")
        if kind == 0:
            value = varint()
        else:
            size = varint() if kind == 2 else (8 if kind == 1 else 4)
            require(offset + size <= len(data), "Truncated heading field")
            value = data[offset:offset + size]
            offset += size
        out.append((tag, kind, value, data[start:offset]))
        require(len(out) <= 256, "Oversized heading field inventory")
    return out


def decoded(value):
    try:
        return wire(base64.b64decode(value, validate=True))
    except (ValueError, TypeError) as error:
        raise RuntimeError("Invalid heading base64") from error


def field(fields, tag, kind=None):
    found = [f for f in fields if f[0] == tag]
    require(len(found) == 1, f"Missing/duplicate heading field {tag}")
    if kind is not None:
        require(found[0][1] == kind, f"Wrong heading field kind {tag}")
    return found[0][2]


def text(fields, tag):
    return field(fields, tag, 2).decode("utf-8")


def content(fields):
    outer = wire(field(fields, 12, 2))
    require([x[0] for x in outer] == [1], "Unexpected heading content variant")
    inner = wire(field(outer, 1, 2))
    require([x[0] for x in inner] in ([1, 2], [2, 1]) and text(inner, 2) == "ja"
            and next(x[3] for x in inner if x[0] == 2) == b"\x12\x02ja",
            "Heading language changed")
    return text(inner, 1)


def plain(rtml):
    return html.unescape(re.sub(r"</?[bi]>", "", rtml.replace("<br>", "\n")))


def styled(rtml):
    class Runs(HTMLParser):
        def __init__(self):
            super().__init__(convert_charrefs=True)
            self.bold = self.italic = False
            self.result = []
        def handle_starttag(self, tag, attrs):
            if tag == "b": self.bold = True
            elif tag == "i": self.italic = True
            elif tag == "br": self.handle_data("\n")
            else: raise RuntimeError("Unknown fixture RTML")
        def handle_endtag(self, tag):
            if tag == "b": self.bold = False
            elif tag == "i": self.italic = False
            else: raise RuntimeError("Unknown fixture closing RTML")
        def handle_data(self, data):
            self.result.extend((c, self.bold, self.italic) for c in data)
    parser = Runs()
    parser.feed(rtml)
    return parser.result


def native_runs(blocks):
    require(isinstance(blocks, list) and len(blocks) == 1 and blocks[0].get("type") == "rich_text",
            "Unexpected native heading blocks")
    sections = blocks[0].get("elements")
    require(isinstance(sections, list) and len(sections) == 1 and sections[0].get("type") == "rich_text_section",
            "Unexpected native heading paragraphs")
    elements = sections[0].get("elements")
    require(isinstance(elements, list), "Missing native heading runs")
    result = []
    for run in elements:
        require(run.get("type") == "text" and isinstance(run.get("text"), str), "Unexpected native heading token")
        style = run.get("style", {})
        require(isinstance(style, dict) and all(k in ("bold", "italic") and type(v) is bool for k, v in style.items()),
                "Unexpected native heading style")
        result.extend((c, style.get("bold", False), style.get("italic", False)) for c in run["text"])
    return result


def inventory(rows):
    require(isinstance(rows, list) and len(rows) == 6, "Wrong heading inventory length")
    result = {}
    for row, identity, level in zip(rows, IDS, LEVELS):
        require(row.get("id") == identity and type(row.get("style")) is int and row["style"] == level,
                "Heading identity/style changed")
        parsed = decoded(row.get("raw"))
        require(text(parsed, 1) == identity and field(parsed, 10, 0) == level
                and row.get("sequence") == field(parsed, 2, 0), "Heading wire identity differs")
        result[identity] = parsed
    return result


def stable(fields):
    # Actual merge_partial appends present tags, including the repeated original
    # identity. Normalize ONLY identity order; preserve all other raw spans/order.
    kept = [f for f in fields if f[0] not in (2, 12, 38)]
    kept.sort(key=lambda f: f[0] if f[0] in (1, 6, 7) else 0)
    return b"".join(f[3] for f in kept)


def attribute_bytes(identity):
    # Independent fixture encoding: content_id29, vertical_margin4=false,
    # default_content13=0, then two ordered opaque177 occurrences.
    value = ("content-" + identity).encode("utf-8")
    require(len(value) < 128, "Oversized fixture content identity")
    return (b"\xea\x01" + bytes([len(value)]) + value + b"\x20\x00\x68\x00"
            + b"\x8a\x0b\x03\x00\xff\x80\x8a\x0b\x0aattrs-tail")


def validate(receipt, content_id=False):
    original = inventory(receipt.get("original"))
    for identity, expected in zip(IDS[1:], INITIAL[1:]):
        require(content(original[identity]) == expected, "Original heading source differs")
        expected_attrs = attribute_bytes(identity) if content_id and identity.startswith("heading-") else b""
        require(field(original[identity], 16, 2) == expected_attrs, "Original heading attributes differ")
    stages = receipt.get("stages")
    require(isinstance(stages, list) and len(stages) == len(STAGES), "Missing heading stages")
    attempts = (0, 0, 0, 1, 1, 2, 3, 4, 4, 4)
    accepted = (0, 0, 0, 0, 1, 2, 3, 3, 3, 3)
    reads = (1, 1, 1, 2, 3, 4, 5, 5, 6, 7)
    previous_writes = []
    previous_acks = []
    for index, (stage, name) in enumerate(zip(stages, STAGES)):
        require(stage.get("stage") == name and type(stage.get("ordinal")) is int and stage["ordinal"] == index
                and all(stage.get(k) is True for k in ("ok", "noSession", "noTransport")), "Wrong heading stage")
        source = stage.get("fixture", {})
        require(type(source.get("attempts")) is int and source["attempts"] == attempts[index]
                and type(source.get("reads")) is int and source["reads"] == reads[index]
                and source.get("uncertain") is (index >= 7), "Wrong heading dispatch/read counts")
        writes, acks = source.get("writes"), source.get("acknowledgements")
        require(isinstance(writes, list) and len(writes) == attempts[index] and isinstance(acks, list)
                and len(acks) == accepted[index] and writes[:len(previous_writes)] == previous_writes
                and acks[:len(previous_acks)] == previous_acks, "Heading immutable receipts changed")
        previous_writes, previous_acks = writes, acks
        for n, write in enumerate(writes):
            identity = ("heading-one", "heading-two", "heading-three", "heading-three")[n]
            data = decoded(write.get("documentData"))
            row = wire(field(data, 1, 2))
            tags = [f[0] for f in row]
            expected_tags = [1, 2, 9, 12, 13] + ([16] if content_id else []) + [19, 20, 33, 38]
            require(tags == expected_tags and write.get("sectionFields") == tags
                    and text(row, 1) == identity and write.get("section") == identity
                    and write.get("desired") == DESIRED[n] and content(row) == DESIRED[n]
                    and field(row, 9, 0) == 0 and field(row, 13, 2) == b"", "Wrong CONTENT-only heading request")
            if content_id:
                require(field(row, 16, 2) == field(original[identity], 16, 2)
                        == attribute_bytes(identity), "Outgoing full heading attributes changed")
        current = inventory(source.get("sections"))
        for i, identity in enumerate(IDS):
            old, new = original[identity], current[identity]
            require(stable(old) == stable(new), "Heading structure/unknown fields changed")
            changed = 1 <= i <= accepted[index]
            if changed:
                require(content(new) == DESIRED[i-1] and field(new, 2, 0) == 2000,
                        "Heading accepted text/revision differs")
                ack = decoded(acks[i-1])
                require([f[0] for f in ack] == [1, 2, 6, 7, 12, 38]
                        and text(ack, 1) == identity and field(ack, 2, 0) == 2000
                        and content(ack) == DESIRED[i-1], "Synthetic partial ACK differs")
            else:
                require(old == new, "Untouched or unconfirmed heading mutated")
        geometry = stage.get("geometry", {})
        require(geometry.get("ok") is True and geometry.get("noOverflow") is True
                and geometry.get("viewport") == receipt["layout"]["viewport"], "Invalid heading geometry")
        rows = geometry.get("rows")
        require(isinstance(rows, list) and len(rows) == 5, "Missing rendered heading identities")
        for row, identity in zip(rows, IDS[1:]):
            require(row.get("id") == identity and row.get("text") == plain(content(current[identity])), "Rendered canonical heading text differs")
            if identity.startswith("heading-"):
                level = LEVELS[IDS.index(identity)]
                require(row.get("tag") == f"H{level}" and row.get("nested") is False
                        and row.get("editLabel") == "Edit heading", "Invalid semantic heading")
        native = geometry.get("native")
        editing = index in (1, 2, 3, 7, 8, 9)
        require(isinstance(native, dict) if editing else native is None, "Wrong mounted heading editor")
        if editing:
            require(native.get("label") == "Heading" and native.get("toolbarLabel") == "Editing heading" and native.get("visible") is True
                    and isinstance(native.get("blocks"), list) and native["blocks"], "Heading native draft missing")
            require(stage.get("editorLayout", {}).get("ok") is True, "Heading editor controls clipped")
            expected_native = {1: "<b>Cancelled 中🙂</b>", 2: "<i>Retained stale 日🦊</i>",
                               3: DESIRED[0], 7: DESIRED[3], 8: DESIRED[3], 9: DESIRED[3]}[index]
            require(native_runs(native["blocks"]) == styled(expected_native), "Native heading text/marks changed")
            if index in (3, 7, 8, 9):
                require(native.get("saveDisabled") is True and native.get("cancelDisabled") is True
                        and native.get("editable") is False, "Pending heading was editable/replayable")
            if index in (7, 8, 9):
                require(native.get("text") == "Unconfirmed 中🙂 keep this", "Uncertain draft lost")
                require(native == stages[7]["geometry"]["native"], "Readback replaced uncertain native editor")
    require(receipt.get("fixture") == stages[-1]["fixture"], "Terminal heading source changed")
