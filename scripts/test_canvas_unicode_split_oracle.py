"""Unicode gates use independently authored visible and serialized text fixtures."""
import unittest

import canvas_split_oracle as C
import test_canvas_split_oracle as F


def ordered(raw):
    return b"".join(x[3] for x in sorted(C.wire(raw), key=lambda x: x[0]))


class UnicodeSplitOracle(unittest.TestCase):
    def rejects(self, receipt, corpus="unicode"):
        with self.assertRaises(RuntimeError): C.validate(receipt, corpus)

    def test_success_stale_partial_and_ascii_can_interleave(self):
        for scenario in ("success", "stale", "partial"):
            C.validate(F.receipt(scenario, "unicode"), "unicode")
            C.validate(F.receipt(scenario))
        self.assertEqual(C.SEED, "Keep thisSecond paragraph")

    def test_corpus_is_pinned_by_caller_not_inferred_from_echo(self):
        self.rejects(F.receipt(), "unicode")
        self.rejects(F.receipt("success", "unicode"), "ascii")
        for location in ("terminal", "original", "stage"):
            for name in (None, "ascii", "unknown"):
                r = F.receipt("success", "unicode")
                target = r if location == "terminal" else r["original"] if location == "original" else r["stages"][2]["fixture"]
                if name is None: del target["corpus"]
                else: target["corpus"] = name
                self.rejects(r)
        self.rejects(F.receipt(), "unknown")

    def test_native_offset_is_visible_utf16_not_rtml_or_codepoint_count(self):
        for offset in (9, 10, 12, 15, 11.0, True):
            r = F.receipt("success", "unicode"); r["savedWitness"]["offset"] = offset
            self.rejects(r)

    def test_lineage_offset_is_serialized_utf16(self):
        write, sent, lineage = F.requests("unicode")
        for offset in (9, 10, 11, 14, 16):
            crdt = C.field(C.wire(sent[0]), 38, 2)
            parent = F.replace(sent[0], 38, F.replace(crdt, 10, F.replace(lineage, 4, offset)))
            altered = dict(write, documentData=F.b64(F.f(1, parent) + F.f(1, sent[1]) + F.f(14, F.TITLE)))
            with self.assertRaises(RuntimeError): C.split_write(altered, "unicode")

    def test_request_body_preserves_escaped_ampersand_and_composed_unicode(self):
        write, sent, _ = F.requests("unicode")
        for value in ("Café & 日本😀", "Café &amp;amp; 日本😀", "Café &amp; 日本😀", "Café &amp; 日本", "Keep this"):
            parent = ordered(F.replace(sent[0], 12, F.body(value)))
            altered = dict(write, documentData=F.b64(F.f(1, parent) + F.f(1, sent[1]) + F.f(14, F.TITLE)))
            with self.assertRaises(RuntimeError): C.split_write(altered, "unicode")

    def test_durable_drafts_are_serialized_but_caret_offset_is_visible(self):
        original = F.receipt("success", "unicode")
        checkpoint = original["stages"][2]["journal"]["receipt"]["checkpoint"]
        self.assertEqual(checkpoint["action"]["Split"]["offset_utf16"], 11)
        self.assertEqual(original["savedWitness"]["offset"], 11)
        _, sent, _ = F.requests("unicode")
        lineage = C.wire(C.field(C.wire(C.field(C.wire(sent[0]), 38, 2)), 10, 2))
        self.assertEqual(C.field(lineage, 4, 0), 15)
        C.validate(original, "unicode")
        for key, value in (("draft", "Café & 日本😀Second paragraph"), ("draft", "Café &amp; 日本😀Second paragraph")):
            r = F.receipt("partial", "unicode"); r["stages"][2]["journal"]["receipt"]["checkpoint"][key] = value
            self.rejects(r)
        for key, value in (("offset_utf16", 15), ("parent_draft", "Café & 日本😀"), ("child_draft", "wrong")):
            r = F.receipt("success", "unicode"); r["stages"][2]["journal"]["receipt"]["checkpoint"]["action"]["Split"][key] = value
            self.rejects(r)

    def test_native_visible_text_cannot_be_escaped_or_normalized(self):
        for value in ("Café &amp; 日本😀", "Café & 日本😀"):
            r = F.receipt("stale", "unicode")
            editor = r["stages"][2]["geometry"]["native"]
            editor.update(text=value + "Second paragraph", blocks=F.blocks(value, "Second paragraph"))
            self.rejects(r)

    def test_ack_and_readback_are_independently_expected_unicode(self):
        write, sent, _ = F.requests("unicode")
        original = F.source_rows("unicode")[1]
        rows = F.acknowledged(sent, original, corpus="unicode")
        for value in ("Café & 日本😀", "Café &amp; 日本😀"):
            bad = [F.replace(rows[0], 12, F.body(value)), rows[1]]
            with self.assertRaises(RuntimeError): C.ack(F.acknowledgement(bad), [C.wire(r) for r in sent], C.wire(original), corpus="unicode")
        r = F.receipt("success", "unicode")
        row = r["stages"][3]["fixture"]["readbacks"][0]["sections"][1]
        row["raw"] = F.b64(F.replace(C.raw(C.decoded(row["raw"])), 12, F.body("Keep this")))
        self.rejects(r)

    def test_ordinary_continuation_retains_unicode_body_and_lineage(self):
        r = F.receipt("success", "unicode")
        write = r["stages"][5]["fixture"]["writes"][1]
        row = C.field(C.decoded(write["documentData"]), 1, 2)
        altered = ordered(F.replace(row, 12, F.body("Café & 日本😀 edited")))
        write["documentData"] = F.b64(F.f(1, altered) + F.f(14, F.TITLE))
        self.rejects(r)


if __name__ == "__main__": unittest.main()
