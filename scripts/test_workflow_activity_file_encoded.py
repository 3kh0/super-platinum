import copy
import json
import unittest

import workflow_activity_file_encoded as E
import workflow_activity_files as F
import workflow_activity_list_header as L
import test_workflow_activity_files as baseline
from test_workflow_activity_files import labels, require, destination, request
from test_workflow_activity_list_header import sample as header

H = dict(require=require, rectangle=lambda r, v: None)


def encoded_labels(inert=False):
    value = labels(inert)
    for row in value["rows"]:
        text = E.LIST_TEXT if row["kind"] == "list" else E.CANVAS_TEXT
        row.update(text=text, name=text, literalProof=dict(
            ok=True, semanticLabel=text, text=text, directChildren=1,
            nestedElements=0, nestedActions=0,
            fragments=[dict(index=0, kind="text", tag="SPAN", text=text, title=None,
                            childElements=0, childNodes=1, nodeType=3)]))
    return value


def encoded_header():
    value = header(False)
    value.update(title=E.HEADER_TEXT, semanticTitle=E.HEADER_TEXT)
    value["titleProof"].update(semanticLabel=E.HEADER_TEXT, text=E.HEADER_TEXT)
    value["titleProof"]["fragments"][0]["text"] = E.HEADER_TEXT
    value["controls"][0]["text"] = E.HEADER_TEXT
    return value


class EncodedFileTests(unittest.TestCase):
    def test_original_duplicate_occurrences_are_literal_and_separately_decoded(self):
        for inert in (False, True):
            F.validate_labels(encoded_labels(inert), [1280, 800], H, inert=inert, encoded=True)

    def reject_label(self, change):
        value = encoded_labels()
        change(value)
        with self.assertRaises(RuntimeError):
            F.validate_labels(value, [1280, 800], H, encoded=True)

    def test_wrong_decode_count_cannot_pass_as_visible_label(self):
        for text in (E.HEADER_TEXT, "R\u00e9vision <b> <", E.ORIGINALS["list"]["file"]["title"]):
            self.reject_label(lambda v: v["rows"][0].update(text=text, name=text))
        self.reject_label(lambda v: v["rows"][2].update(text="Canvas <i> &lt;", name="Canvas <i> &lt;"))

    def test_exact_text_node_proof_rejects_injected_or_hidden_nodes(self):
        for key, value in (("childElements", 1), ("childNodes", 2), ("nodeType", 1),
                           ("tag", "B"), ("kind", "emoji"), ("title", "extra"),
                           ("text", E.HEADER_TEXT)):
            self.reject_label(lambda v: v["rows"][0]["literalProof"]["fragments"][0].update({key: value}))
        self.reject_label(lambda v: v["rows"][0]["literalProof"].update(nestedActions=1))
        self.reject_label(lambda v: v["rows"][0]["literalProof"].update(semanticLabel=E.HEADER_TEXT))

    def test_native_header_remains_once_decoded(self):
        L.validate(encoded_header(), [1280, 800], H, encoded=True)
        value = encoded_header()
        value["title"] = E.LIST_TEXT
        with self.assertRaises(RuntimeError):
            L.validate(value, [1280, 800], H, encoded=True)

    def test_original_request_keeps_encoded_raw_title(self):
        row = request()
        row["raw"] = copy.deepcopy(E.ORIGINALS["list"])
        row["responseText"] = json.dumps(row["raw"])
        F.validate_request(row, 1, require, originals=E.ORIGINALS)
        row["raw"]["file"]["title"] = E.LIST_TEXT
        row["responseText"] = json.dumps(row["raw"])
        with self.assertRaises(RuntimeError):
            F.validate_request(row, 1, require, originals=E.ORIGINALS)

    def test_destination_document_cannot_be_rewritten_to_either_display_title(self):
        value = destination()
        value["loaded"]["document"] = copy.deepcopy(E.ORIGINALS["list"]["file"])
        value["responses"][0]["responses"][0]["file"] = copy.deepcopy(E.ORIGINALS["list"]["file"])
        F.validate_destination(value, True, "destination", require, originals=E.ORIGINALS)
        for title in (E.LIST_TEXT, E.HEADER_TEXT):
            value["loaded"]["document"]["title"] = title
            with self.assertRaises(RuntimeError):
                F.validate_destination(value, True, "destination", require, originals=E.ORIGINALS)

    def test_composition_and_calendar_retention_use_actual_encoded_label(self):
        for stage in ("composition-refused", "calendar-refused"):
            value = baseline.FileRefusalTests.exercise(stage)
            value["originalLabels"] = encoded_labels()
            if stage == "composition-refused":
                value["inert"] = encoded_labels(True)
                value["activation"]["text"] = E.LIST_TEXT
            else:
                value["restoredLabels"] = encoded_labels()
                value["attempt"]["text"] = E.LIST_TEXT
            F.validate_refusal(stage, value, [1280, 800], H, encoded=True)

    def test_encoded_case_is_exact_ordinary_failure_ladder(self):
        self.assertEqual(F.STAGES["file-encoded"], F.STAGES["file"])
        self.assertNotIn("file-encoded-foreign", F.STAGES)


if __name__ == "__main__":
    unittest.main()
