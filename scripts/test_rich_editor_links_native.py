"""Pure negative tests for exact link payload and stage evidence; no native launch."""
import copy
import importlib.util
import json
from pathlib import Path
import unittest
spec = importlib.util.spec_from_file_location("links", Path(__file__).with_name("check-rich-editor-links-native.py"))
links = importlib.util.module_from_spec(spec)
spec.loader.exec_module(links)


def geometry(stage="cancelled", narrow=False):
    fixture = "forward-links" + ("-narrow" if narrow else "")
    width, height = links.expected_viewport(fixture)
    opened = stage in ("create-form", "edit-form")
    names = (["rich-link-url", "rich-link-apply", "rich-link-cancel"] +
             (["rich-link-remove"] if stage == "edit-form" else [])) if opened else ["composer-icon-btn rich-link-open", "composer-icon-btn rich-code-toggle"]
    return {"ok": True, "viewport": [width, height], "panelOpen": opened, "focus": "rich-link-url" if opened else "rich-reminder-editor",
            "dialog": {"left": 10, "top": 10, "right": width-10, "bottom": height-10, "width": width-20, "height": height-20},
            "body": {"left": 15, "top": 15, "right": width-15, "bottom": height-15, "width": width-30, "height": height-30},
            "controls": [{"class": name, "hit": True, "rect": {"left": 20, "top": 20+i*35, "right": 120, "bottom": 50+i*35, "width": 100, "height": 30}} for i, name in enumerate(names)]}


def receipt():
    value = {"ok": True, "noSession": True, "noTransport": True, "stages": 9, "preparedRequests": 5, "layout": geometry()}
    value.update({key: True for key in links.SPEC.required})
    value["prepared"] = []
    for i, (url, code) in enumerate((("https://example.com/guide?q=1#note", False), ("https://example.com/updated", False), (None, False), (None, True), (None, False))):
        blocks = links.expected_blocks(url, code)
        form = [["token", ""], ["channel", "C_FIXTURE"], ["timestamp", "1.0"], ["share_channel", "C_LIST_NOTE_FIXTURE"], ["client_msg_id", str(i)], ["text", ""], ["blocks", json.dumps(blocks)]]
        value["prepared"].append({"ok": True, "sourceKept": True, "canvasParagraph": True, "preparedFields": 7, "blocks": blocks, "prepared": form})
    return value


class EvidenceTests(unittest.TestCase):
    def test_accepts_exact_notes_and_both_viewports(self):
        links.validate_links("forward-links", receipt())
        for narrow in (False, True):
            for stage in links.SPEC.stages:
                links.validate_geometry("forward-links" + ("-narrow" if narrow else ""), geometry(stage, narrow), stage)

    def test_rejects_missing_code_or_changed_adjacent_style(self):
        for field, value in (("code", False), ("bold", False)):
            r = receipt()
            blocks = r["prepared"][3]["blocks"]
            blocks[0]["elements"][0]["elements"][0]["style"][field] = value
            r["prepared"][3]["prepared"][6][1] = json.dumps(blocks)
            with self.assertRaises(RuntimeError): links.validate_links("forward-links", r)

    def test_rejects_missing_code_control_even_with_link_visible(self):
        g = geometry();g["controls"].pop()
        with self.assertRaises(RuntimeError): links.validate_geometry("forward-links", g, "coded")

    def test_rejects_raw_form_changes_even_with_claimed_blocks(self):
        for field, value in ((0, "secret"), (1, "C_OTHER"), (3, "C_OTHER"), (5, "invented"), (6, "[]")):
            r = receipt();r["prepared"][1]["prepared"][field][1] = value
            with self.assertRaises(RuntimeError): links.validate_links("forward-links", r)

    def test_rejects_reordered_or_reused_preparations(self):
        for mutate in (lambda r: r["prepared"].reverse(), lambda r: r["prepared"][1]["prepared"][4].__setitem__(1, "0"), lambda r: r["prepared"][0]["prepared"].reverse()):
            r = receipt();mutate(r)
            with self.assertRaises(RuntimeError): links.validate_links("forward-links", r)

    def test_rejects_missing_guards_and_boolean_counts(self):
        for key in (*links.SPEC.required, "noSession", "noTransport"):
            r = receipt();r[key] = False
            with self.assertRaises(RuntimeError): links.validate_links("forward-links", r)
        r = receipt();r["prepared"][0]["preparedFields"] = True
        with self.assertRaises(RuntimeError): links.validate_links("forward-links", r)

    def test_rejects_hidden_clipped_foreign_and_obstructed_controls(self):
        for mutate in (lambda g: g["controls"][0].__setitem__("hit", False), lambda g: g["controls"][0]["rect"].__setitem__("left", -1), lambda g: g["controls"][0]["rect"].__setitem__("width", 0), lambda g: g["dialog"].__setitem__("top", 80), lambda g: g["controls"][0]["rect"].__setitem__("top", float("nan"))):
            g = geometry();mutate(g)
            with self.assertRaises(RuntimeError): links.validate_geometry("forward-links", g, "cancelled")

    def test_rejects_body_edge_clip_with_visible_center(self):
        g = geometry();r = g["controls"][0]["rect"]
        g["body"]["top"] = r["top"] + 5
        self.assertLess(g["body"]["top"], (r["top"] + r["bottom"]) / 2)
        self.assertTrue(g["controls"][0]["hit"])
        with self.assertRaises(RuntimeError): links.validate_geometry("forward-links", g, "cancelled")

    def test_rejects_wrong_form_focus_controls_and_viewport(self):
        for mutate in (lambda g: g.__setitem__("panelOpen", False), lambda g: g.__setitem__("focus", "outside"), lambda g: g["controls"].reverse(), lambda g: g["viewport"].__setitem__(1, 1)):
            g = geometry("edit-form");mutate(g)
            with self.assertRaises(RuntimeError): links.validate_geometry("forward-links", g, "edit-form")

if __name__ == "__main__": unittest.main()
