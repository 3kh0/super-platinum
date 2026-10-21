"""Mutation tests of measured receipt validation, not native acceptance."""
import copy
import math
import unittest

from slack_list_board_geometry import EXPECTED, validate_geometry


def rect(x, y, width, height):
    return dict(x=x, y=y, left=x, top=y, right=x + width, bottom=y + height,
                width=width, height=height)


def paint(r):
    return dict(rect=r, inView=True, hit=True)


def text(value, bounds):
    return dict(text=value, lines=1, ranges=[
        rect(bounds["x"] + 2, bounds["y"] + 6, max(5, len(value) * 6), 16)])


def sample(viewport=(1280, 780)):
    """Explicit artificial operands exercise the oracle; never a screenshot claim."""
    bounds = rect(20, 25, 112, 32)
    add = dict(ok=True, **paint(bounds), label=dict(
        text="Add group", lines=1, ranges=[rect(51, 33, 63, 16)]),
        icon=rect(27, 31, 20, 20), controlHeight=32, intrinsic=112,
        box=dict(gap=4, paddingLeft=12, paddingRight=11, borderLeft=1, borderRight=1),
        disabled=False, view="V_BOARD_MULTI", value="")
    lanes = []
    for key, name, count in EXPECTED:
        heading, counter = rect(20, 76, 120, 28), rect(150, 76, 30, 28)
        row = dict(ok=True, key=key, lane=rect(10, 70, 264, 250),
                   label=text(name, heading), heading=paint(heading),
                   counter=paint(counter), count=text(count, counter),
                   header=paint(rect(10, 70, 264, 120)), actions=[],
                   identities=True, nonoverlap=True, actionBounds=True,
                   cards=rect(10, 190, 264, 130))
        kinds = (["name", "color", "delete"] if key else []) + ["left", "right", "add"]
        for index, kind in enumerate(kinds):
            label = "" if kind == "add" else kind.capitalize()
            r = rect(20 + (index % 3) * 70, 108 + (index // 3) * 40,
                     32 if kind == "add" else 64, 32)
            action = dict(**paint(r), text=label, label=text(label, r) if label
                          else dict(text="", ranges=[], lines=0), disabled=False,
                          choice=None, value=None, view=None, delete=None,
                          move=None, laneKey=None, add=kind == "add",
                          ariaLabel=None, focusStable=True)
            if kind in ("name", "color"):
                action.update(choice=kind, value=key, view="V_BOARD_MULTI")
            elif kind == "delete":
                action.update(delete=key, view="V_BOARD_MULTI")
            else:
                action["laneKey"] = key
                if kind != "add":
                    action.update(move=kind, ariaLabel="Move lane " + kind)
                else:
                    action["ariaLabel"] = "Add item to " + name
            row["actions"].append(action)
        lanes.append(row)
    refresh, close = rect(20, 10, 80, 32), rect(20, 50, 32, 32)
    header = dict(restored=True)
    for kind, own, other in (("refresh", refresh, close), ("close", close, refresh)):
        header[kind + "Reachable"] = dict(
            ok=True, rect=own, otherRect=other,
            header=rect(10, 5, viewport[0] - 20, 80),
            id="slack-list-" + kind, text="Refresh" if kind == "refresh" else "",
            ariaLabel="Close list" if kind == "close" else None,
            contained=True, bounded=True, noOverlap=True, hit=True, focused=True,
            scroll=[0, 30])
    return dict(layout=dict(ok=True, bounded=True, headerNoOverlap=True,
                            viewport=list(viewport), header=header),
                boardViews=dict(headerGeometry=dict(ok=True, restored=True,
                                                     addGroup=add, lanes=lanes)))


class Geometry(unittest.TestCase):
    def setUp(self):
        self.value = sample()
        self.board = self.value["boardViews"]["headerGeometry"]
        self.lane = self.board["lanes"][0]
        self.action = self.lane["actions"][0]

    def validate(self, value=None):
        value = self.value if value is None else value
        validate_geometry(value, value["layout"]["viewport"])

    def refused(self, mutation):
        value = copy.deepcopy(self.value)
        mutation(value)
        with self.assertRaises(RuntimeError):
            self.validate(value)

    def test_normal_narrow_and_vertical_wrapped_header(self):
        for viewport in [(1280, 780), (668, 380)]:
            with self.subTest(viewport=viewport):
                self.validate(sample(viewport))

    def test_vertical_letter_stack_cannot_forge_one_line(self):
        self.lane["label"]["ranges"] = [rect(22, 78 + i * 2, 5, 2)
                                        for i in range(len("Engineering"))]
        with self.assertRaises(RuntimeError):
            self.validate()

    def test_short_label_multiple_actual_lines(self):
        self.lane["label"]["ranges"].append(rect(22, 98, 12, 4))
        self.lane["label"]["lines"] = 2
        with self.assertRaises(RuntimeError):
            self.validate()

    def test_label_outside_heading_even_if_in_view_true(self):
        self.lane["label"]["ranges"] = [rect(21, 82, 160, 16)]
        with self.assertRaises(RuntimeError):
            self.validate()

    def test_lost_changed_reordered_original_lane_ids_labels_counts(self):
        for field, changed in [("key", "OTHER"), ("label", {"text": "Other"}),
                               ("count", {"text": "9"})]:
            with self.subTest(field=field):
                self.refused(lambda v: v["boardViews"]["headerGeometry"]["lanes"][0]
                             .__setitem__(field, changed))
        self.board["lanes"].reverse()
        with self.assertRaises(RuntimeError):
            self.validate()

    def test_missing_extra_lanes_actions(self):
        for path in ["lanes", "actions"]:
            for add in [False, True]:
                v = copy.deepcopy(self.value)
                target = v["boardViews"]["headerGeometry"]["lanes"]
                if path == "actions":
                    target = target[0]["actions"]
                target.append(copy.deepcopy(target[0])) if add else target.pop()
                with self.subTest(path=path, add=add), self.assertRaises(RuntimeError):
                    self.validate(v)

    def test_each_original_action_identity_is_checked(self):
        for index in range(6):
            for field in ["choice", "value", "view", "delete", "move", "laneKey", "add"]:
                v = copy.deepcopy(self.value)
                v["boardViews"]["headerGeometry"]["lanes"][0]["actions"][index][field] = "foreign"
                with self.subTest(index=index, field=field), self.assertRaises(RuntimeError):
                    self.validate(v)

    def test_lost_accessible_move_and_add_labels(self):
        for index in [3, 4, 5]:
            v = copy.deepcopy(self.value)
            v["boardViews"]["headerGeometry"]["lanes"][0]["actions"][index]["ariaLabel"] = ""
            with self.subTest(index=index), self.assertRaises(RuntimeError):
                self.validate(v)

    def test_overlapping_actions_rejected_despite_true_flag(self):
        self.lane["actions"][1]["rect"] = copy.deepcopy(self.action["rect"])
        self.lane["actions"][1]["label"] = text("Color", self.action["rect"])
        with self.assertRaises(RuntimeError):
            self.validate()

    def test_actions_outside_original_lane_even_with_enlarged_header(self):
        self.lane["header"]["rect"] = rect(10, 70, 400, 120)
        self.action["rect"] = rect(300, 108, 64, 32)
        self.action["label"] = text("Name", self.action["rect"])
        with self.assertRaises(RuntimeError):
            self.validate()

    def test_identity_and_action_overlap(self):
        self.action["rect"] = rect(20, 76, 64, 32)
        self.action["label"] = text("Name", self.action["rect"])
        with self.assertRaises(RuntimeError):
            self.validate()

    def test_control_height_and_lane_add_width(self):
        for index, r in [(0, rect(20, 108, 64, 40)), (5, rect(160, 148, 64, 32))]:
            v = copy.deepcopy(self.value)
            v["boardViews"]["headerGeometry"]["lanes"][0]["actions"][index]["rect"] = r
            with self.subTest(index=index), self.assertRaises(RuntimeError):
                self.validate(v)

    def test_enabled_action_requires_hit_and_stable_focus(self):
        for field in ["hit", "focusStable", "inView"]:
            v = copy.deepcopy(self.value)
            v["boardViews"]["headerGeometry"]["lanes"][0]["actions"][0][field] = False
            with self.subTest(field=field), self.assertRaises(RuntimeError):
                self.validate(v)

    def test_disabled_action_can_lack_hit_but_not_geometry(self):
        self.action.update(disabled=True, hit=False)
        self.validate()
        self.action["rect"] = rect(-100, 108, 64, 32)
        with self.assertRaises(RuntimeError):
            self.validate()

    def test_oversized_add_group_and_forged_intrinsic(self):
        for forge in [False, True]:
            v = copy.deepcopy(self.value)
            add = v["boardViews"]["headerGeometry"]["addGroup"]
            add["rect"] = rect(20, 25, 600, 32)
            if forge:
                add["intrinsic"] = 600
            with self.subTest(forge=forge), self.assertRaises(RuntimeError):
                self.validate(v)

    def test_add_group_icon_stacks_or_overlaps_text(self):
        for r in [rect(27, 10, 20, 20), rect(60, 31, 20, 20)]:
            v = copy.deepcopy(self.value)
            v["boardViews"]["headerGeometry"]["addGroup"]["icon"] = r
            with self.subTest(rect=r), self.assertRaises(RuntimeError):
                self.validate(v)

    def test_add_group_foreign_view_value_or_text(self):
        for field, changed in [("view", "V_OTHER"), ("value", "OTHER"),
                               ("label", {"text": "Add item"}), ("disabled", True)]:
            v = copy.deepcopy(self.value)
            v["boardViews"]["headerGeometry"]["addGroup"][field] = changed
            with self.subTest(field=field), self.assertRaises(RuntimeError):
                self.validate(v)

    def test_card_pane_positive_and_below_header(self):
        for r in [rect(10, 189, 264, 0), rect(10, 140, 264, 100)]:
            v = copy.deepcopy(self.value)
            v["boardViews"]["headerGeometry"]["lanes"][0]["cards"] = r
            with self.subTest(rect=r), self.assertRaises(RuntimeError):
                self.validate(v)

    def test_missing_or_nonfinite_geometry(self):
        for value in [None, math.nan, math.inf, True, -1, 0]:
            v = copy.deepcopy(self.value)
            v["boardViews"]["headerGeometry"]["lanes"][0]["heading"]["rect"]["width"] = value
            with self.subTest(value=value), self.assertRaises(RuntimeError):
                self.validate(v)

    def test_forged_range_count_or_dimensions(self):
        for mutate in [lambda l: l["label"].update(lines=True),
                       lambda l: l["label"].update(ranges=[]),
                       lambda l: l["heading"]["rect"].update(right=900)]:
            v = copy.deepcopy(self.value)
            mutate(v["boardViews"]["headerGeometry"]["lanes"][0])
            with self.assertRaises(RuntimeError):
                self.validate(v)

    def test_header_original_control_identity(self):
        for kind in ["refresh", "close"]:
            for field in ["id", "text"]:
                v = copy.deepcopy(self.value)
                v["layout"]["header"][kind + "Reachable"][field] = "foreign"
                with self.subTest(kind=kind, field=field), self.assertRaises(RuntimeError):
                    self.validate(v)

    def test_header_unreachable_false_flags(self):
        for kind in ["refresh", "close"]:
            for field in ["hit", "focused", "contained", "bounded", "noOverlap"]:
                v = copy.deepcopy(self.value)
                v["layout"]["header"][kind + "Reachable"][field] = False
                with self.subTest(kind=kind, field=field), self.assertRaises(RuntimeError):
                    self.validate(v)

    def test_header_raw_overlap_rejected_despite_true_flags(self):
        self.value["layout"]["header"]["refreshReachable"]["otherRect"] = rect(40, 10, 32, 32)
        with self.assertRaises(RuntimeError):
            self.validate()

    def test_header_raw_offscreen_or_outside_parent(self):
        for r in [rect(20, 800, 80, 32), rect(1250, 10, 80, 32), rect(20, 82, 80, 32)]:
            v = copy.deepcopy(self.value)
            v["layout"]["header"]["refreshReachable"]["rect"] = r
            with self.subTest(rect=r), self.assertRaises(RuntimeError):
                self.validate(v)

    def test_original_scroll_focus_restoration_required(self):
        for path in ["board", "header"]:
            v = copy.deepcopy(self.value)
            target = (v["boardViews"]["headerGeometry"] if path == "board"
                      else v["layout"]["header"])
            target["restored"] = False
            with self.subTest(path=path), self.assertRaises(RuntimeError):
                self.validate(v)


if __name__ == "__main__":
    unittest.main()
