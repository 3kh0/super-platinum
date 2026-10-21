"""Portable adversarial heading receipt tests; no native process or network."""
import base64
import copy
import unittest
import json
from pathlib import Path
import canvas_heading as H


def varint(n):
    out = bytearray()
    while n > 127:
        out.append((n & 127) | 128)
        n >>= 7
    out.append(n)
    return bytes(out)


def f(tag, value):
    if isinstance(value, str):
        value = value.encode()
    if isinstance(value, bytes):
        return varint(tag * 8 + 2) + varint(len(value)) + value
    return varint(tag * 8) + varint(value)


def b64(value):
    return base64.b64encode(value).decode()


def body(value):
    return f(1, f(1, value) + f(2, "ja"))


def attrs(identity):
    return f(29, "content-" + identity) + f(4, 0) + f(13, 0) + f(177, b"\x00\xff\x80") + f(177, "attrs-tail")


def source(identity, level, rtml, changed=False, content_id=False):
    raw = (f(1, identity) + f(2, 2000 if changed else 1000) + f(6, "thread") + f(7, "document")
           + f(8, identity) + f(9, 64 if level == 48 else 0) + f(10, level)
           + f(12, f(58, f(1, "Fixture title")) if level == 48 else body(rtml))
           + f(13, b"") + f(16, attrs(identity) if content_id and 1 <= level <= 3 else b"") + f(177, "opaque"))
    if changed:
        raw += f(38, f(3, 2000))
    return {"id": identity, "style": level, "sequence": 2000 if changed else 1000, "raw": b64(raw)}


def sample(content_id=False):
    original = [source(i, level, rtml, content_id=content_id) for i, level, rtml in zip(H.IDS, H.LEVELS, H.INITIAL)]
    writes, acks = [], []
    for n, rtml in enumerate(H.DESIRED):
        identity = ("heading-one", "heading-two", "heading-three", "heading-three")[n]
        row = (f(1, identity) + f(2, 1000) + f(9, 0) + f(12, body(rtml)) + f(13, b"")
               + (f(16, attrs(identity)) if content_id else b"")
               + f(19, 1) + f(20, 1) + f(33, "original-session") + f(38, f(3, 1000)))
        writes.append({"section": identity, "documentData": b64(f(1, row) + f(14, b"")),
                       "sectionFields": [1, 2, 9, 12, 13] + ([16] if content_id else []) + [19, 20, 33, 38], "desired": rtml, "sequence": n + 1})
        if n < 3:
            acks.append(b64(f(1, identity) + f(2, 2000) + f(6, "thread") + f(7, "document")
                            + f(12, body(rtml)) + f(38, f(3, 2000))))
    result = {"original": original, "layout": {"viewport": [668, 380]}, "stages": []}
    for n, name in enumerate(H.STAGES):
        accepted = (0, 0, 0, 0, 1, 2, 3, 3, 3, 3)[n]
        attempted = (0, 0, 0, 1, 1, 2, 3, 4, 4, 4)[n]
        sections = [source(i, l, H.DESIRED[j-1] if 1 <= j <= accepted else r, 1 <= j <= accepted, content_id)
                    for j, (i, l, r) in enumerate(zip(H.IDS, H.LEVELS, H.INITIAL))]
        rows = [{"id": row["id"], "text": H.plain(H.content(H.decoded(row["raw"]))),
                 "tag": f"H{row['style']}" if row["id"].startswith("heading-") else "DIV",
                 "nested": False, "editLabel": "Edit heading"} for row in sections[1:]]
        native = None
        if n in (1, 2, 3, 7, 8, 9):
            wanted = {1: "<b>Cancelled 中🙂</b>", 2: "<i>Retained stale 日🦊</i>", 3: H.DESIRED[0],
                      7: H.DESIRED[3], 8: H.DESIRED[3], 9: H.DESIRED[3]}[n]
            elements = [{"type": "text", "text": c, "style": {"bold": b, "italic": i}} for c, b, i in H.styled(wanted)]
            blocks = [{"type": "rich_text", "elements": [{"type": "rich_text_section", "elements": elements}]}]
            native = {"label": "Heading", "toolbarLabel": "Editing heading", "visible": True, "blocks": blocks,
                      "text": "Unconfirmed 中🙂 keep this", "generation": "7", "visit": "1",
                      "editable": n < 3, "saveDisabled": n >= 3, "cancelDisabled": n >= 3}
        source_proof = {"attempts": attempted, "reads": (1, 1, 1, 2, 3, 4, 5, 5, 6, 7)[n],
                        "uncertain": n >= 7, "writes": writes[:attempted], "acknowledgements": acks[:accepted],
                        "sections": sections}
        result["stages"].append({"ok": True, "stage": name, "ordinal": n, "noSession": True,
                                 "noTransport": True, "fixture": source_proof, "editorLayout": {"ok": True},
                                 "geometry": {"ok": True, "noOverflow": True, "viewport": [668, 380],
                                              "rows": rows, "native": native}})
    result["fixture"] = copy.deepcopy(result["stages"][-1]["fixture"])
    return copy.deepcopy(result)


class HeadingReceiptTests(unittest.TestCase):
    def refuses(self, mutation):
        receipt = sample()
        mutation(receipt)
        with self.assertRaises(RuntimeError):
            H.validate(receipt)

    def test_complete_source_contract(self):
        H.validate(sample())

    def test_identity_style_and_sibling_mutations(self):
        for key, value in (("id", "heading-two"), ("style", 1), ("raw", "AA==")):
            with self.subTest(key=key):
                self.refuses(lambda r: r["stages"][6]["fixture"]["sections"][4].__setitem__(key, value))

    def test_content_only_request_rejects_style(self):
        def change(r):
            w = r["stages"][3]["fixture"]["writes"][0]
            row = H.field(H.decoded(w["documentData"]), 1, 2)
            w["documentData"] = b64(f(1, row + f(10, 1)))
        self.refuses(change)

    def test_semantic_nesting_and_label(self):
        for key, value in (("tag", "H2"), ("nested", True), ("text", "Wrong label"), ("editLabel", "Edit paragraph")):
            with self.subTest(key=key):
                self.refuses(lambda r: r["stages"][4]["geometry"]["rows"][0].__setitem__(key, value))

    def test_uncertain_never_unlocks_or_replaces_mount(self):
        for key, value in (("saveDisabled", False), ("cancelDisabled", False), ("editable", True),
                           ("generation", "8"), ("text", "Dropped draft"), ("blocks", [])):
            with self.subTest(key=key):
                self.refuses(lambda r: r["stages"][9]["geometry"]["native"].__setitem__(key, value))

    def test_readback_not_replayed_or_falsely_confirmed(self):
        for key, value in (("attempts", 5), ("reads", 6), ("uncertain", False)):
            with self.subTest(key=key):
                self.refuses(lambda r: r["stages"][9]["fixture"].__setitem__(key, value))

    def test_missing_or_reordered_stage(self):
        self.refuses(lambda r: r["stages"].pop())
        self.refuses(lambda r: r["stages"].reverse())

    def test_native_marks_and_softbreaks_are_exact(self):
        self.refuses(lambda r: r["stages"][3]["geometry"]["native"]["blocks"][0]["elements"][0]["elements"][0]["style"].__setitem__("bold", False))
        self.refuses(lambda r: r["stages"][3]["geometry"]["native"]["blocks"][0]["elements"][0]["elements"].pop())

    def test_mounted_heading_requires_exact_toolbar_caption(self):
        for index in (1, 2, 3, 7, 8, 9):
            for value in (None, "", "Editing paragraph", "Read view", "Editing heading "):
                with self.subTest(stage=index, value=value):
                    self.refuses(lambda r: r["stages"][index]["geometry"]["native"].__setitem__("toolbarLabel", value))

    def test_partial_merge_identity_order_only(self):
        original = H.decoded(sample()["original"][1]["raw"])
        changed = [v for v in original if v[0] not in (1, 6, 7)] + [v for tag in (1, 6, 7) for v in original if v[0] == tag]
        self.assertNotEqual(original, changed)
        self.assertEqual(H.stable(original), H.stable(changed))
        nonidentity = [v for v in changed if v[0] not in (1, 6, 7)]
        identities = [v for v in changed if v[0] in (1, 6, 7)]
        self.assertNotEqual(H.stable(original), H.stable(list(reversed(nonidentity)) + identities))
        for tag in (1, 6, 7, 8, 10, 13, 16, 177):
            with self.subTest(tag=tag):
                removed = [v for v in changed if v[0] != tag]
                self.assertNotEqual(H.stable(original), H.stable(removed))
                self.assertNotEqual(H.stable(original), H.stable(changed + [v for v in changed if v[0] == tag]))
                old = next(v for v in changed if v[0] == tag)
                replacement = f(tag, old[2] + 1 if old[1] == 0 else old[2] + b"changed")
                mutated = [v for v in changed if v[0] != tag] + H.wire(replacement)
                self.assertNotEqual(H.stable(original), H.stable(mutated))

    def test_actual_failed_original_identity_replay(self):
        path = Path(__file__).resolve().parents[1] / "tmp/canvas-heading-edit-20261019/heading-normal-a/canvas-heading.json"
        if not path.exists():
            self.skipTest("Retained local native failure unavailable")
        receipt = json.loads(path.read_text(encoding="utf-8"))
        self.assertEqual(receipt["native"]["error"], "Heading edit changed structure/opaque bytes")
        saved = receipt["stages"][3]["fixture"]
        original = H.decoded(next(r["raw"] for r in saved["sections"] if r["id"] == "heading-one"))
        merged = [v for v in original if v[0] not in (1, 2, 6, 7, 12, 38)]
        merged += [v for tag in (1, 6, 7) for v in original if v[0] == tag]
        before = [v for v in original if v[0] not in (2, 12, 38)]
        self.assertNotEqual(b"".join(v[3] for v in before), b"".join(v[3] for v in merged))
        self.assertEqual(H.stable(before), H.stable(merged))

    def test_content_replacement_preserves_language_bytes_in_either_order(self):
        for inner in (f(1, "Changed 中🙂") + f(2, "ja"), f(2, "ja") + f(1, "Changed 中🙂")):
            self.assertEqual(H.content(H.wire(f(12, f(1, inner)))), "Changed 中🙂")
        for inner in (f(1, "x") + f(2, "en"), f(1, "x") + f(2, "ja") + f(2, "ja"),
                      f(2, "ja"), f(1, "x") + f(1, "y") + f(2, "ja"),
                      f(1, "x") + f(2, "ja") + f(3, "opaque"),
                      f(1, "x") + b"\x12\x82\x00ja"):
            with self.subTest(inner=inner), self.assertRaises(RuntimeError):
                H.content(H.wire(f(12, f(1, inner))))

    def test_actual_completed_native_content_replacement(self):
        path = Path(__file__).resolve().parents[1] / "tmp/canvas-heading-edit-20261019/heading-normal-c/canvas-heading.json"
        if not path.exists():
            self.skipTest("Retained completed native receipt unavailable")
        receipt = json.loads(path.read_text(encoding="utf-8"))
        self.assertIs(receipt["native"]["ok"], True)
        H.validate(receipt["native"])

    def test_malformed_wire(self):
        for value in (b"\x80", b"\x0a\x05a", b"\x00", b"\x0f"):
            with self.subTest(value=value), self.assertRaises(RuntimeError):
                H.wire(value)


class ContentIdHeadingTests(unittest.TestCase):
    def test_full_attributes_all_levels_and_uncertain_write(self):
        receipt = sample(content_id=True)
        H.validate(receipt, content_id=True)
        self.assertEqual(len(receipt["fixture"]["writes"]), 4)
        for identity in H.IDS[1:5]:
            self.assertEqual(attrs(identity), H.attribute_bytes(identity))

    def test_named_modes_do_not_accept_each_others_wire(self):
        with self.assertRaises(RuntimeError):
            H.validate(sample(), content_id=True)
        with self.assertRaises(RuntimeError):
            H.validate(sample(content_id=True))

    def test_outgoing_full_attributes_cannot_be_normalized_or_dropped(self):
        good = attrs("heading-one")
        mutations = (b"", attrs("heading-two"),
                     good.replace(f(4, 0), b""), good.replace(f(13, 0), b""),
                     good.replace(f(4, 0), f(4, 1)),
                     good.replace(f(177, b"\x00\xff\x80"), f(177, b"\x00\xff\x81")),
                     b"".join(x[3] for x in reversed(H.wire(good))),
                     good + f(29, "content-heading-one"))
        for replacement in mutations:
            with self.subTest(replacement=replacement):
                receipt = sample(content_id=True)
                write = receipt["stages"][3]["fixture"]["writes"][0]
                outer = H.decoded(write["documentData"])
                row = H.wire(H.field(outer, 1, 2))
                raw = b"".join(f(16, replacement) if x[0] == 16 else x[3] for x in row)
                write["documentData"] = b64(b"".join(f(1, raw) if x[0] == 1 else x[3] for x in outer))
                with self.assertRaises(RuntimeError):
                    H.validate(receipt, content_id=True)

    def test_canonical_and_duplicate_sibling_attributes_remain_original(self):
        for index, row in ((0, 1), (4, 1), (6, 4), (9, 3)):
            with self.subTest(stage=index, row=row):
                receipt = sample(content_id=True)
                target = receipt["stages"][index]["fixture"]["sections"][row]
                raw = H.decoded(target["raw"])
                target["raw"] = b64(b"".join(f(16, b"") if x[0] == 16 else x[3] for x in raw))
                with self.assertRaises(RuntimeError):
                    H.validate(receipt, content_id=True)

    def test_original_attrs_are_independently_pinned(self):
        receipt = sample(content_id=True)
        raw = H.decoded(receipt["original"][1]["raw"])
        receipt["original"][1]["raw"] = b64(b"".join(f(16, attrs("heading-two")) if x[0] == 16 else x[3] for x in raw))
        with self.assertRaises(RuntimeError):
            H.validate(receipt, content_id=True)

    def test_synthetic_partial_ack_must_not_claim_new_attributes(self):
        receipt = sample(content_id=True)
        ack = receipt["stages"][4]["fixture"]["acknowledgements"][0]
        receipt["stages"][4]["fixture"]["acknowledgements"][0] = b64(base64.b64decode(ack) + f(16, b""))
        with self.assertRaises(RuntimeError):
            H.validate(receipt, content_id=True)


if __name__ == "__main__":
    unittest.main()
