import copy
import unittest

import workflow_activity_file_emoji as E
import workflow_activity_files as F
import workflow_activity_list_header as L
from test_workflow_activity_file_emoji import H, paint
from test_workflow_activity_files import destination, require


def sample(emoji=True, cold=False):
    proof = paint()
    for key in ("container", "ordinal", "file", "kind", "part", "interactive", "href", "name", "icon", "inline"):
        proof.pop(key, None)
    proof.update(ok=True, id="slack-list-title", tag="H2")
    if cold:
        proof["semanticLabel"] = E.LIST_COLD_LABEL
        proof["text"] = E.LIST_COLD_TEXT
        proof["fragments"][3].update(kind="emoji", text=":activity_cold:",
                                     title=":activity_cold:", image=None)
    if not emoji:
        proof.update(semanticLabel=F.LIST_LABEL, text=F.LIST_LABEL, directChildren=1)
        proof["fragments"] = [dict(proof["fragments"][0], text=F.LIST_LABEL)]
    return dict(title=proof["text"], semanticTitle=proof["semanticLabel"], titleProof=proof,
                controls=[dict(id="slack-list-title", text=proof["text"], visible=True)])


class ListHeaderTests(unittest.TestCase):
    def test_actual_plain_ready_and_cold_heading(self):
        for emoji, cold in ((False, False), (True, False), (True, True)):
            L.validate(sample(emoji, cold), [800, 600], H, emoji=emoji, cold=cold)

    def reject(self, change, *, emoji=True, cold=False):
        value = sample(emoji, cold)
        change(value)
        with self.assertRaises(RuntimeError):
            L.validate(value, [800, 600], H, emoji=emoji, cold=cold)

    def test_semantic_header_cannot_substitute_for_actual_paint(self):
        self.reject(lambda v: v.update(title=E.LIST_LABEL))
        self.reject(lambda v: v["controls"][0].update(text=E.LIST_LABEL))
        self.reject(lambda v: v["titleProof"].update(text=E.LIST_LABEL))

    def test_original_heading_identity_and_geometry_required(self):
        self.reject(lambda v: v["titleProof"].update(id="other"))
        self.reject(lambda v: v["titleProof"].update(tag="DIV"))
        self.reject(lambda v: v["titleProof"].update(visible=False))
        self.reject(lambda v: v["titleProof"]["rect"].update(left=-1))
        self.reject(lambda v: v["controls"].clear())

    def test_custom_title_image_has_real_decode_and_alt(self):
        self.reject(lambda v: v["titleProof"]["fragments"][3]["image"].update(naturalWidth=0))
        self.reject(lambda v: v["titleProof"]["fragments"][3]["image"].update(alt=":activity_ready:"))

    def test_cold_title_keeps_readable_original_shortcode(self):
        self.reject(lambda v: v["titleProof"]["fragments"][3].update(text=""), cold=True)
        self.reject(lambda v: v["titleProof"]["fragments"][3].update(image={}), cold=True)

    def test_plain_heading_cannot_gain_emoji_or_actions(self):
        self.reject(lambda v: v["titleProof"]["fragments"][0].update(kind="emoji"), emoji=False)
        self.reject(lambda v: v["titleProof"].update(nestedActions=1), emoji=False)

    def test_cold_original_document_does_not_contain_painted_title(self):
        value = destination()
        value["loaded"]["document"] = copy.deepcopy(E.COLD_ORIGINALS["list"]["file"])
        value["responses"][0]["responses"][0]["file"] = copy.deepcopy(E.COLD_ORIGINALS["list"]["file"])
        F.validate_destination(value, True, "destination", require, originals=E.COLD_ORIGINALS)
        value["loaded"]["document"]["title"] = E.LIST_COLD_TEXT
        with self.assertRaises(RuntimeError):
            F.validate_destination(value, True, "destination", require, originals=E.COLD_ORIGINALS)


if __name__ == "__main__":
    unittest.main()
