"""Artificial receipt mutations test the oracle, not native rendering."""
import copy
import importlib.util
from pathlib import Path
import unittest

from test_slack_list_board_geometry import rect

SPEC = importlib.util.spec_from_file_location(
    "record_viewport_check", Path(__file__).with_name("check-slack-list-native.py"))
CHECK = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CHECK)


def sample(fixture="slack-list"):
    viewport = CHECK.expected_viewport(fixture)
    width = viewport[0]
    field = rect(30, 200, width - 60, 40)
    scroller = rect(20, 180, width - 40, 100)
    client = {k: scroller[k] for k in ("left", "top", "right", "bottom")}
    result = {}
    for kind, selected in (("table", "Engineering"), ("board", "Design")):
        measurement = dict(
            ok=True, current=True, originalBounds=True, fullField=True, fieldHit=True,
            identity=dict(record="R_LIST_002", column="C_MULTI", open="7", action="9"),
            viewport=viewport, field=field,
            label=dict(text="Teams", rect=rect(32, 210, 80, 20)),
            value=dict(text=selected, rect=rect(150, 210, 100, 20)),
            edit=dict(rect=rect(width - 60, 204, 25, 32), disabled=False),
            scroll=dict(rect=scroller, client=client, clientWidth=width-40,
                        clientHeight=100, scrollWidth=width-40, scrollHeight=500,
                        scrollLeft=0, scrollTop=200),
            deltas=dict(top=20, bottom=40, left=10, right=10))
        refresh, close = rect(30, 40, 80, 32), rect(120, 40, 32, 32)
        header = dict(restored=True)
        for key, own, other in (("refresh", refresh, close), ("close", close, refresh)):
            header[key] = dict(
                ok=True, bounded=True, contained=True, hit=True, separate=True, focused=True,
                id="slack-list-" + key, text="Refresh" if key == "refresh" else "",
                ariaLabel=None if key == "refresh" else "Close list", disabled=False,
                rect=own, otherRect=other, header=rect(20, 30, width-40, 50),
                headerClient=dict(left=20, top=30, right=width-20, bottom=80), scroll=[0, 0])
        result[kind] = dict(before=copy.deepcopy(measurement),
                            after=copy.deepcopy(measurement), header=header)
    return dict(recordDetails=dict(captureGeometry=result))


class RecordViewport(unittest.TestCase):
    def setUp(self):
        self.receipt = sample()

    def validate(self):
        CHECK.validate_record_captures(self.receipt, "slack-list")

    def mutate_measurement(self, change):
        proof = self.receipt["recordDetails"]["captureGeometry"]["board"]
        change(proof["before"])
        proof["after"] = copy.deepcopy(proof["before"])

    def test_normal_and_narrow_operands(self):
        self.validate()
        CHECK.validate_record_captures(sample("slack-list-narrow"), "slack-list-narrow")
        self.assertEqual(len(CHECK.STAGES), 29)

    def test_original_offscreen_scroller_bug(self):
        def shift(value):
            # The old bounds predicate still holds: field entirely inside scroller.
            for r in (value["field"], value["scroll"]["rect"], value["scroll"]["client"]):
                for k in ("top", "bottom"):
                    r[k] += 1000
                if "y" in r:
                    r["y"] += 1000
        self.mutate_measurement(shift)
        with self.assertRaisesRegex(RuntimeError, "viewport"):
            self.validate()

    def test_fractionally_clipped_field_refused_without_tolerance(self):
        self.mutate_measurement(lambda v: v["scroll"]["client"].update(top=200.1))
        with self.assertRaises(RuntimeError):
            self.validate()

    def test_wrong_original_record_column_or_selection(self):
        for change in (
            lambda v: v["identity"].update(record="R_LIST_999"),
            lambda v: v["identity"].update(column="C_NAME"),
            lambda v: v["value"].update(text="Engineering"),
            lambda v: v["label"].update(text="Other"),
        ):
            self.receipt = sample()
            self.mutate_measurement(change)
            with self.assertRaises(RuntimeError):
                self.validate()

    def test_mutation_during_capture(self):
        self.receipt["recordDetails"]["captureGeometry"]["board"]["after"]["identity"]["action"] = "10"
        with self.assertRaisesRegex(RuntimeError, "changed across"):
            self.validate()

    def test_forged_clearance_or_client_size(self):
        for change in (
            lambda v: v["deltas"].update(top=99),
            lambda v: v["scroll"].update(clientHeight=999),
            lambda v: v["field"].update(width=float("nan")),
        ):
            self.receipt = sample()
            self.mutate_measurement(change)
            with self.assertRaises(RuntimeError):
                self.validate()

    def test_stale_unreachable_or_disabled_field(self):
        for change in (
            lambda v: v.update(current=False),
            lambda v: v.update(fieldHit=False),
            lambda v: v["edit"].update(disabled=True),
        ):
            self.receipt = sample()
            self.mutate_measurement(change)
            with self.assertRaises(RuntimeError):
                self.validate()

    def test_revealed_controls_original_identity_hit_and_bounds(self):
        for change in (
            lambda v: v.update(id="other"),
            lambda v: v.update(hit=False),
            lambda v: v.update(focused=False),
            lambda v: v.update(disabled=True),
            lambda v: v["headerClient"].update(top=45),
            lambda v: v.update(otherRect=copy.deepcopy(v["rect"])),
        ):
            self.receipt = sample()
            change(self.receipt["recordDetails"]["captureGeometry"]["board"]["header"]["refresh"])
            with self.assertRaises(RuntimeError):
                self.validate()

    def test_close_accessible_label_and_restoration(self):
        header = self.receipt["recordDetails"]["captureGeometry"]["table"]["header"]
        header["close"]["ariaLabel"] = "Close other"
        with self.assertRaises(RuntimeError):
            self.validate()
        header["close"]["ariaLabel"] = "Close list"
        header["restored"] = False
        with self.assertRaises(RuntimeError):
            self.validate()

    def test_missing_either_capture_geometry(self):
        for kind in ("table", "board"):
            self.receipt = sample()
            del self.receipt["recordDetails"]["captureGeometry"][kind]
            with self.assertRaises(RuntimeError):
                self.validate()


if __name__ == "__main__":
    unittest.main()
