"""Synthetic receipt mutations test the oracle; they are not native acceptance."""
import copy
import importlib.util
from pathlib import Path
import unittest

from test_slack_list_board_geometry import rect

SPEC = importlib.util.spec_from_file_location(
    "editor_hover_check", Path(__file__).with_name("check-slack-list-native.py"))
CHECK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECK)


def sample(fixture="slack-list"):
    width, height = CHECK.expected_viewport(fixture)
    result = {}
    for domain, user, label, query, initial, selected in (
        ("user", "U00000001", "Alice Example", "Whitney",
         ["U00000001"], ["U00000001", "W00000003"]),
        ("channel", "U1", "Maya Chen", "gamma",
         ["C00000001"], ["C00000001", "C00000003"]),
    ):
        def measurement(text, ids, generation):
            return dict(
                generation=generation, dialogGeneration="7", query=text,
                selectionStart=len(text), selectionEnd=len(text), selected=ids,
                sameRoot=True, sameInput=True, viewport=dict(width=width, height=height),
                root=rect(20, 60, width - 40, 320),
                controls={
                    key: dict(id="slack-list-editor-" + key, text=title, disabled=False,
                              centerHit=True, rect=rect(x, 320, 80, 32))
                    for key, title, x in (("cancel", "Cancel", width - 210),
                                          ("save", "Save", width - 120))
                })
        blocked = measurement("", initial, "11")
        captured = measurement(query, selected, "17")
        result[domain + "Edits"] = dict(hover=dict(
            preHover=dict(user=user, label=label, visible=True),
            blockedEditor=dict(before=blocked, after=copy.deepcopy(blocked),
                               hoverAbsent=True, sourceConnected=True, observedMs=500),
            afterClose=dict(user=user, label=label, visible=True),
            capture=dict(before=captured, after=copy.deepcopy(captured))))
    return result


class EditorHover(unittest.TestCase):
    def validate(self, receipt, fixture="slack-list"):
        CHECK.validate_editor_hover(receipt, fixture)

    def mutate_pair(self, change, pair="capture", domain="channel"):
        receipt = sample()
        value = receipt[domain + "Edits"]["hover"][pair]
        change(value["before"])
        value["after"] = copy.deepcopy(value["before"])
        return receipt

    def test_normal_and_narrow_synthetic_operands(self):
        for fixture in ("slack-list", "slack-list-narrow"):
            self.validate(sample(fixture), fixture)
        self.assertEqual(len(CHECK.STAGES), 29)

    def test_no_hover_proof_is_not_acceptance(self):
        for key in ("preHover", "blockedEditor", "afterClose", "capture"):
            value = sample()
            del value["userEdits"]["hover"][key]
            with self.subTest(key=key), self.assertRaises(RuntimeError):
                self.validate(value)

    def test_background_hover_must_be_observed_and_return_after_close(self):
        for phase, change in (
            ("preHover", dict(visible=False)),
            ("afterClose", dict(visible=False)),
            ("afterClose", dict(user="UFOREIGN")),
            ("preHover", dict(label="Wrong name")),
            ("blockedEditor", dict(hoverAbsent=False)),
            ("blockedEditor", dict(sourceConnected=False)),
        ):
            value = sample()
            value["channelEdits"]["hover"][phase].update(change)
            with self.subTest(phase=phase, change=change), self.assertRaises(RuntimeError):
                self.validate(value)

    def test_cancel_and_save_need_original_center_hit(self):
        for control in ("cancel", "save"):
            value = self.mutate_pair(lambda v: v["controls"][control].update(centerHit=False))
            with self.subTest(control=control), self.assertRaises(RuntimeError):
                self.validate(value)

    def test_actual_bounded_hover_observation_required(self):
        for elapsed in (None, True, 499, float("inf")):
            value = sample()
            value["userEdits"]["hover"]["blockedEditor"]["observedMs"] = elapsed
            with self.subTest(elapsed=elapsed), self.assertRaises(RuntimeError):
                self.validate(value)

    def test_original_control_identity_and_enabled_state(self):
        for change in (dict(id="foreign-save"), dict(text="Continue"), dict(disabled=True),
                       dict(disabled=0)):
            value = self.mutate_pair(lambda v: v["controls"]["save"].update(change))
            with self.subTest(change=change), self.assertRaises(RuntimeError):
                self.validate(value)

    def test_controls_strictly_inside_root_and_viewport(self):
        width, height = CHECK.expected_viewport("slack-list")
        for bounds in (rect(width-50, 320, 80, 32), rect(30, height-20, 80, 32),
                       rect(19.9, 320, 80, 32), rect(30, 40, 80, 32)):
            value = self.mutate_pair(lambda v: v["controls"]["save"].update(rect=bounds))
            with self.subTest(bounds=bounds), self.assertRaises(RuntimeError):
                self.validate(value)

    def test_overlapping_controls_refused_even_with_true_hit_flags(self):
        value = self.mutate_pair(
            lambda v: v["controls"]["save"].update(rect=v["controls"]["cancel"]["rect"]))
        with self.assertRaises(RuntimeError):
            self.validate(value)

    def test_nonfinite_empty_and_inconsistent_bounds(self):
        for bounds in (rect(30, 320, 0, 32), rect(float("nan"), 320, 80, 32),
                       dict(rect(30, 320, 80, 32), right=111)):
            value = self.mutate_pair(lambda v: v["controls"]["save"].update(rect=bounds))
            with self.subTest(bounds=bounds), self.assertRaises(RuntimeError):
                self.validate(value)

    def test_native_nodes_and_original_generations_required(self):
        for change in (dict(sameRoot=False), dict(sameInput=False),
                       dict(generation=""), dict(dialogGeneration=7)):
            value = self.mutate_pair(lambda v: v.update(change))
            with self.subTest(change=change), self.assertRaises(RuntimeError):
                self.validate(value)

    def test_mutated_native_snapshot_during_hover_or_screenshot(self):
        for pair in ("blockedEditor", "capture"):
            value = sample()
            value["channelEdits"]["hover"][pair]["after"]["generation"] = "999"
            with self.subTest(pair=pair), self.assertRaises(RuntimeError):
                self.validate(value)

    def test_exact_query_selected_ids_and_selection_ranges(self):
        for change in (dict(query="gamma-lost"), dict(selected=["C00000003", "C00000001"]),
                       dict(selected=["C00000001", "CFOREIGN"]), dict(selectionStart=-1),
                       dict(selectionStart=True), dict(selectionEnd=6),
                       dict(selectionStart=3, selectionEnd=2)):
            value = self.mutate_pair(lambda v: v.update(change))
            with self.subTest(change=change), self.assertRaises(RuntimeError):
                self.validate(value)
        value = self.mutate_pair(lambda v: v.update(query="changed"), pair="blockedEditor")
        with self.assertRaises(RuntimeError):
            self.validate(value)

    def test_successor_dialog_and_wrong_viewport_refused(self):
        for change in (dict(dialogGeneration="8"), dict(viewport=dict(width=1, height=1))):
            value = self.mutate_pair(lambda v: v.update(change))
            with self.subTest(change=change), self.assertRaises(RuntimeError):
                self.validate(value)


if __name__ == "__main__":
    unittest.main()
