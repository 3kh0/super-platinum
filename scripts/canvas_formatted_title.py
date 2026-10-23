"""Independent original-wire/title/draft oracle. No native or network actions."""
from canvas_heading import require, wire, decoded, field, text, native_runs, styled, plain

STAGES = ("editing", "cancelled", "saving", "confirmed", "title-refused", "cancelled-title")
IDS = ("title", "paragraph", "unsupported", "parent", "item")
TITLE = "<b>Plan &amp; Review</b>"
STALE_TITLE = "<i>Plan &#38; Review</i>"
LABEL = "Plan & Review"
INITIAL = "A <b>bold</b> line 🦊 日本語 &amp; details."
DESIRED = INITIAL + " <i>Added</i>"
REFUSED = DESIRED + " retained"
ERROR = "This text or your access changed. Cancel and refresh."


def content(row):
    outer = wire(field(row, 12, 2))
    require([f[0] for f in outer] == [1], "Paragraph content variant changed")
    inner = wire(field(outer, 1, 2))
    require(sorted(f[0] for f in inner) == [1, 2] and text(inner, 2) == "en",
            "Original paragraph language changed")
    return text(inner, 1)


def title_text(row):
    outer = wire(field(row, 12, 2))
    require([f[0] for f in outer] == [58], "Canonical title content variant changed")
    inner = wire(field(outer, 58, 2))
    require([f[0] for f in inner] == [1], "Canonical title source shape changed")
    return text(inner, 1)


def inventory(value):
    require(isinstance(value, dict) and (value.get("file"), value.get("thread"), value.get("document"))
            == ("Ffixture", "thread", "document"), "Canonical scope changed")
    thread, document = decoded(value.get("threadRaw")), decoded(value.get("documentRaw"))
    require(text(thread, 1) == "thread" and text(thread, 10) == "document"
            and text(document, 1) == "document" and text(document, 6) == "thread"
            and text(wire(field(document, 69, 2)), 1) == "Fixture title", "Original document/title source changed")
    rows = value.get("sections")
    require(isinstance(rows, list) and len(rows) == len(IDS), "Canonical section inventory changed")
    result = {}
    for row, identity in zip(rows, IDS):
        raw = decoded(row.get("raw"))
        require(row.get("id") == identity and text(raw, 1) == identity
                and text(raw, 6) == "thread" and text(raw, 7) == "document"
                and type(row.get("sequence")) is int and row["sequence"] == field(raw, 2, 0),
                "Original canonical row/order/revision changed")
        result[identity] = raw
    require(field(result["title"], 9, 0) == 64 and field(result["title"], 10, 0) == 48,
            "Canonical title kind changed")
    require(content(result["unsupported"]) == "<u data-unsupported='true'>This unsupported formatting stays readable.</u>"
            and content(result["item"]) == "An untouched checklist item"
            and field(wire(field(result["item"], 16, 2)), 2, 0) == 0,
            "Unrelated paragraph/checklist changed")
    return result


def stable(row, mutable):
    return b"".join(f[3] for f in row if f[0] not in mutable)


def rich_title(data):
    require([f[0] for f in data] == [1, 14], "DocumentData fields changed")
    rich = wire(field(data, 14, 2))
    require([f[0] for f in rich] == [1, 2] and text(rich, 1) == "rich_text", "Title rich root changed")
    section = wire(field(rich, 2, 2))
    require([f[0] for f in section] == [1, 2] and text(section, 1) == "rich_text_section", "Title rich section changed")
    leaf = wire(field(section, 2, 2))
    require([f[0] for f in leaf] == [1, 2] and text(leaf, 1) == "text" and text(leaf, 2) == LABEL,
            "Static title is not exact unstyled decoded text")


def native(value, wanted, editable, error=None):
    require(isinstance(value, dict) and value.get("visible") is True
            and value.get("editable") is editable and value.get("saveDisabled") is (not editable)
            and value.get("cancelDisabled") is (not editable) and value.get("error") == error,
            "Original native editor/action readiness changed")
    require(isinstance(value.get("generation"), str) and value["generation"].isdigit()
            and isinstance(value.get("visit"), str) and value["visit"].isdigit(), "Native owner absent")
    require(value.get("text") == plain(wanted) and native_runs(value.get("blocks")) == styled(wanted),
            "Original native text/formatting changed")


def validate(receipt):
    require(receipt.get("httpFactoryExecuted") is False, "Native cannot claim HTTP factory execution")
    original = receipt.get("original", {})
    initial = inventory(original.get("fresh"))
    require(original.get("visible") == original["fresh"] and original.get("attempts") == 0
            and original.get("pending") is False and original.get("operation") is None
            and original.get("stale") is False and original.get("loading") is False,
            "Original canonical view was not ready")
    for key in ("writes", "preflights", "acknowledgements", "readbacks"):
        require(original.get(key) == [], "Original fixture already dispatched")
    require(title_text(initial["title"]) == TITLE and content(initial["paragraph"]) == INITIAL
            and all(field(row, 2, 0) == 1000 for row in initial.values()), "Original authored source differs")
    native(receipt.get("initialNative"), INITIAL, True)
    require(receipt.get("duplicate") == {"ok": True, "clicks": 2, "id": "canvas-text-save"}, "Actual duplicate Save probe absent")
    stages = receipt.get("stages")
    require(isinstance(stages, list) and len(stages) == 6, "Missing formatted-title stages")
    previous = {key: [] for key in ("preflights", "writes", "acknowledgements", "readbacks")}
    for n, (stage, name) in enumerate(zip(stages, STAGES)):
        require(stage.get("stage") == name and type(stage.get("ordinal")) is int and stage["ordinal"] == n
                and all(stage.get(k) is True for k in ("ok", "noSession", "noTransport")), "Stage order/isolation changed")
        source = stage.get("fixture", {})
        counts = {"preflights": (0, 0, 1, 1, 2, 2)[n], "writes": int(n >= 2),
                  "acknowledgements": int(n >= 3), "readbacks": int(n >= 3)}
        for key, count in counts.items():
            rows = source.get(key)
            require(isinstance(rows, list) and len(rows) == count and rows[:len(previous[key])] == previous[key],
                    f"Original {key} ledger changed")
            previous[key] = rows
        require(type(source.get("attempts")) is int and source["attempts"] == int(n >= 2)
                and source.get("pending") is (n == 2) and source.get("loading") is False
                and source.get("stale") is (n == 2) and source.get("visit") == original.get("visit")
                and type(source.get("read")) is int and source["read"] > 0,
                "Original visit/read/dispatch status changed")
        if n == 2:
            operation = source.get("operation", {})
            require(operation.get("stage") == "Saving" and type(operation.get("serial")) is int,
                    "Original pending operation absent")
        else:
            require(source.get("operation") is None, "Unexpected retained operation")
        current, visible = inventory(source.get("fresh")), inventory(source.get("visible"))
        for identity in IDS:
            for observed in (current, visible):
                mutable = (2, 12, 38) if identity == "paragraph" else ((2, 12) if identity == "title" else ())
                require(stable(observed[identity], mutable) == stable(initial[identity], mutable), "Unrelated canonical bytes changed")
            if identity not in ("title", "paragraph"):
                require(current[identity] == visible[identity] == initial[identity], "Unrelated section was rewritten")
        require(source["fresh"]["documentRaw"] == source["visible"]["documentRaw"] == original["fresh"]["documentRaw"]
                and source["fresh"]["threadRaw"] == source["visible"]["threadRaw"] == original["fresh"]["threadRaw"],
                "Document or thread metadata changed")
        require(visible["title"] == initial["title"], "Paragraph Save rewrote original title bytes")
        require(title_text(current["title"]) == (STALE_TITLE if n >= 4 else TITLE)
                and field(current["title"], 2, 0) == (3000 if n >= 4 else 1000), "Fresh title revision/source differs")
        body = DESIRED if n >= 3 else INITIAL
        require(content(current["paragraph"]) == content(visible["paragraph"]) == body
                and field(current["paragraph"], 2, 0) == field(visible["paragraph"], 2, 0) == (2000 if n >= 3 else 1000),
                "Canonical body or readback revision changed")
        if n < 3:
            require(source["read"] == original["read"], "Read changed before original ACK")
        else:
            require(source["read"] == stages[3]["fixture"]["read"] > original["read"], "Original readback identity changed")
        for preflight_index, preflight in enumerate(source["preflights"]):
            require(preflight.get("visit") == original["visit"]
                    and preflight.get("outcome") == ("prepared" if preflight_index == 0 else "Changed"), "Actual title preflight outcome changed")
            expected_old = original["visible"] if preflight_index == 0 else stages[3]["fixture"]["visible"]
            expected_fresh = original["fresh"] if preflight_index == 0 else stages[4]["fixture"]["fresh"]
            require(preflight.get("source") == expected_old and preflight.get("fresh") == expected_fresh,
                    "Original preflight raw source or fresh response changed")
        for write in source["writes"]:
            require(write.get("section") == "paragraph" and write.get("desired") == DESIRED
                    and write.get("formTitle") == LABEL and write.get("httpFactoryExecuted") is False
                    and write.get("baseSequence") == 1000 and write.get("sequence") == 1
                    and isinstance(write.get("session"), str) and write["session"], "Prepared original metadata changed")
            data = decoded(write.get("documentData")); rich_title(data)
            row = wire(field(data, 1, 2))
            require([f[0] for f in row] == [1, 2, 9, 12, 13, 19, 20, 33, 38]
                    and text(row, 1) == "paragraph" and field(row, 2, 0) == 1000
                    and field(row, 9, 0) == 0 and field(row, 13, 2) == b""
                    and field(row, 19, 0) == field(row, 20, 0) == 1
                    and text(row, 33) == write["session"] + "-0" and content(row) == DESIRED,
                    "Only original paragraph CONTENT may be dispatched")
        for ack in source["acknowledgements"]:
            raw = decoded(ack.get("raw")); final = wire(field(raw, 2, 2))
            row = wire(field(final, 1, 2))
            require([f[0] for f in raw] == [1, 2] and field(raw, 1, 0) == 1
                    and text(final, 15) == "document" and text(row, 1) == "paragraph"
                    and content(row) == DESIRED and ack.get("authoredContent") == DESIRED
                    and field(row, 2, 0) == field(wire(field(row, 38, 2)), 3, 0) == 2000,
                    "Independent canonical ACK differs")
        if n >= 3:
            require(source["readbacks"] == [stages[3]["fixture"]["fresh"]], "Independent full readback changed")
        geometry = stage.get("geometry", {})
        require(geometry.get("ok") is True and geometry.get("noOverflow") is True
                and geometry.get("viewport") == receipt["layout"]["viewport"]
                and geometry.get("title") == LABEL and geometry.get("paragraph") == plain(body),
                "Rendered canonical title/body differs")
        if n in (0, 2, 4):
            native(geometry.get("native"), REFUSED if n == 4 else DESIRED, n != 2, ERROR if n == 4 else None)
            require(stage.get("editorLayout", {}).get("ok") is True
                    and geometry["native"]["visit"] == str(original["visit"]), "Native editor owner/layout changed")
        else:
            require(geometry.get("native") is None and stage.get("editorLayout") is None, "Cancelled/confirmed editor remained mounted")
    before, after = receipt.get("beforeRefusal", {}).get("native"), receipt.get("afterRefusal", {}).get("native")
    native(before, REFUSED, True); native(after, REFUSED, True, ERROR)
    require(all(before[k] == after[k] for k in ("blocks", "generation", "visit"))
            and after == stages[4]["geometry"]["native"], "Title refusal replaced original draft owner")
    require(receipt.get("fixture") == stages[-1]["fixture"] and receipt.get("layout") == stages[-1]["geometry"],
            "Final original source/layout changed")
