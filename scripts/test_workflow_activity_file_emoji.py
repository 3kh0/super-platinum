import copy
import unittest

import workflow_activity_file_emoji as E
import workflow_activity_files as F
from test_workflow_activity_files import destination, labels, require, request


def paint(kind="list"):
    row = copy.deepcopy(labels()["rows"][0 if kind == "list" else 2])
    row["semanticLabel"] = E.LIST_LABEL if kind == "list" else E.CANVAS_LABEL
    row["text"] = row["name"] = E.LIST_TEXT if kind == "list" else E.CANVAS_TEXT
    row.update(directChildren=6, nestedActions=0)
    specs = [
        ("text", "Révision & 中 " if kind == "list" else "Canvas & café ", None),
        ("emoji", "👋", ":wave:"), ("text", " ", None),
        ("image", "", None) if kind == "list" else ("emoji", ":activity_cold:", ":activity_cold:"),
        ("text", " ", None), ("text", ":unknown:", None),
    ]
    image = dict(tag="IMG", alt=":activity_ready_alias:", title=":activity_ready_alias:",
                 src="http://super-platinum-media.emoji/fixture?v=1", complete=True,
                 naturalWidth=16, naturalHeight=16, visible=True, rect=copy.deepcopy(row["nameRect"]))
    row["fragments"] = [
        dict(ok=True, index=i, tag="SPAN", kind=k, text=text, title=title,
             visible=True, rect=copy.deepcopy(row["nameRect"]), image=copy.deepcopy(image) if k == "image" else None)
        for i, (k, text, title) in enumerate(specs)
    ]
    return row


H = dict(require=require, rectangle=lambda rect, viewport: require(
    isinstance(rect, dict) and 0 <= rect["left"] < rect["right"] <= viewport[0]
    and 0 <= rect["top"] < rect["bottom"] <= viewport[1], "Clipped fragment"))


class FileEmojiPaintTests(unittest.TestCase):
    def rejected(self, mutate, kind="list"):
        value = paint(kind)
        mutate(value)
        with self.assertRaises(RuntimeError):
            E.validate_paint(value, [800, 600], H)

    def test_ready_alias_and_cold_fallback(self):
        for kind in ("list", "canvas"):
            E.validate_paint(paint(kind), [800, 600], H)

    def test_semantic_label_is_independent_from_painted_text(self):
        self.rejected(lambda v: v.update(semanticLabel=E.LIST_TEXT))

    def test_standard_precedence_and_unknown_spelling_are_exact(self):
        self.rejected(lambda v: v["fragments"][1].update(text=":wave:"))
        self.rejected(lambda v: v["fragments"][5].update(text="unknown"))

    def test_fragment_order_and_tooltips_cannot_drift(self):
        self.rejected(lambda v: v["fragments"].reverse())
        self.rejected(lambda v: v["fragments"][1].update(title=None))
        self.rejected(lambda v: v["fragments"][2].update(kind="emoji"))

    def test_ready_image_must_decode_from_native_media(self):
        for patch in (dict(src="https://fixture.invalid/activity-ready.png"),
                      dict(naturalWidth=0), dict(complete=False),
                      dict(alt=":activity_ready:"), dict(title="guessed")):
            self.rejected(lambda v: v["fragments"][3]["image"].update(patch))

    def test_cold_fallback_cannot_invent_image_or_drop_shortcode(self):
        self.rejected(lambda v: v["fragments"][3].update(image={}), "canvas")
        self.rejected(lambda v: v["fragments"][3].update(text=""), "canvas")

    def test_extra_actions_children_and_hidden_content_rejected(self):
        self.rejected(lambda v: v.update(nestedActions=1))
        self.rejected(lambda v: v.update(directChildren=7))
        self.rejected(lambda v: v["fragments"][1].update(visible=False))

    def test_variant_raw_receipt_remains_encoded(self):
        value = request()
        value["raw"] = F.expected_response(F.LIST, "success", originals=E.ORIGINALS)
        import json
        value["responseText"] = json.dumps(value["raw"])
        F.validate_request(value, 1, require, originals=E.ORIGINALS)
        with self.assertRaises(RuntimeError):
            F.validate_request(value, 1, require)
        value["raw"]["file"]["title"] = E.LIST_LABEL
        value["responseText"] = json.dumps(value["raw"])
        with self.assertRaises(RuntimeError):
            F.validate_request(value, 1, require, originals=E.ORIGINALS)

    def test_destination_uses_original_variant_document(self):
        value = destination()
        value["loaded"]["document"] = copy.deepcopy(E.ORIGINALS["list"]["file"])
        value["responses"][0]["responses"][0]["file"] = copy.deepcopy(E.ORIGINALS["list"]["file"])
        F.validate_destination(value, True, "destination", require, originals=E.ORIGINALS)
        value["loaded"]["document"]["title"] = E.LIST_LABEL
        with self.assertRaises(RuntimeError):
            F.validate_destination(value, True, "destination", require, originals=E.ORIGINALS)


def refused():
    return dict(kind="title-image-alt", unchangedRustState=True, noFileRead=True, noDestination=True,
                attempt=dict(refusals=0, originalAlt=":activity_ready_alias:", mutatedAlt="forged Activity file emoji",
                             semanticLabel=E.LIST_LABEL, part="1", originalConnected=True,
                             src="http://super-platinum-media.emoji/fixture?v=1"),
                native=dict(refusals=1, notice=E.REFUSAL, alt="forged Activity file emoji",
                            originalImage=True, originalName=True, originalAnchor=True),
                restored=dict(alt=":activity_ready_alias:", src="http://super-platinum-media.emoji/fixture?v=1",
                              originalImage=True, originalName=True, originalAnchor=True, originalChildren=True))


class FileEmojiRefusalTests(unittest.TestCase):
    def validate(self, value):
        E.validate_refused_paint(value, [dict(text=E.REFUSAL)], require)

    def test_actual_refusal_and_original_restoration(self):
        self.validate(refused())

    def test_dispatch_or_missing_refusal_rejected(self):
        for mutate in (lambda v: v.update(noFileRead=False),
                       lambda v: v["native"].update(refusals=0),
                       lambda v: v["native"].update(alt=":activity_ready_alias:")):
            value = refused()
            mutate(value)
            with self.assertRaises(RuntimeError):
                self.validate(value)

    def test_replacement_is_not_original_restore(self):
        for key in ("originalImage", "originalName", "originalAnchor", "originalChildren"):
            value = refused()
            value["restored"][key] = False
            with self.assertRaises(RuntimeError):
                self.validate(value)


if __name__ == "__main__":
    unittest.main()
