"""Composition expectations from actual helper source; pure synthetic evidence only."""
import ast
import copy
from pathlib import Path
import runpy
import unittest

P = Path(__file__).resolve().parent
OLD = runpy.run_path(str(P / "test_protected_adoption_receipts.py"))
D = OLD["D"]
C = D["C"]


def actual_function(path, name, namespace):
    """Exercise staged shared functions without writing or changing active modules."""
    tree = ast.parse(path.read_text(encoding="utf-8"), filename=str(path))
    node = next(x for x in tree.body if isinstance(x, ast.FunctionDef) and x.name == name)
    scope = dict(namespace)
    exec(compile(ast.Module(body=[node], type_ignores=[]), str(path), "exec"), scope)
    return scope[name]


def candidate_helpers(contract):
    source = P if (P / "check-draft-update-recovery-native.py").is_file() else C["SCRIPTS"]
    base = contract["BASE"]
    native = actual_function(source / "check-draft-update-recovery-native.py", "native", base["native"].__globals__)
    geometry = actual_function(source / "protected_drafts_receipts.py", "geometry", {**base["geometry"].__globals__, "native": native})
    base.update(native=native, geometry=geometry)
    contract["native"] = native
    contract["validate"].__globals__.update(native=native, BASE=base)
    return native, geometry


NATIVE, GEOMETRY = candidate_helpers(C)
# Run the unchanged public rejection corpus against the candidate common functions.
AdoptionReceipts = OLD["AdoptionReceipts"]


class Composition(unittest.TestCase):
    def reject(self, edit, scenario="native-refusals-channel"):
        v = D["good"](scenario)
        edit(v)
        with self.assertRaises((RuntimeError, KeyError, TypeError, ValueError)):
            D["validate"](v)

    def test_only_four_observations_are_composing(self):
        for fixture in C["FIXTURES"]:
            v = D["good"]("native-refusals-channel", fixture)
            active = [r for r in v["snapshots"] if r["native"]["native"]["composing"]]
            self.assertEqual([r["stage"] for r in active], ["composition-active"])
            self.assertEqual([k for k, x in v["details"].items() if isinstance(x, dict) and isinstance(x.get("native"), dict) and x["native"].get("composing")], ["compositionResultWritable", "compositionStart"])
            D["validate"](v)

    def test_active_false_rejected_in_each_independent_observation(self):
        for key in ("native", "composer"):
            with self.subTest(key=key):
                self.reject(lambda v: next(r for r in v["snapshots"] if r["stage"] == "composition-active")[key]["native"].update(composing=False))
        for name in ("compositionStart", "compositionResultWritable"):
            with self.subTest(detail=name):
                self.reject(lambda v: v["details"][name]["native"].update(composing=False))

    def test_every_nonactive_stage_refuses_true(self):
        for scenario in C["SCENARIOS"]:
            for i, record in enumerate(D["good"](scenario)["snapshots"]):
                if record["stage"] == "composition-active":
                    continue
                for key in ("native", "composer"):
                    if record[key] is None:
                        continue
                    with self.subTest(scenario=scenario, stage=record["stage"], key=key):
                        self.reject(lambda v: v["snapshots"][i][key]["native"].update(composing=True), scenario)

    def test_every_nonactive_detail_refuses_true(self):
        def natives(value, path=()):
            if isinstance(value, dict):
                if "composing" in value and "utf16Length" in value:
                    yield path, value
                else:
                    for k, x in value.items():
                        yield from natives(x, path + (k,))
            elif isinstance(value, list):
                for k, x in enumerate(value):
                    yield from natives(x, path + (k,))
        for scenario in C["SCENARIOS"]:
            for path, n in natives(D["good"](scenario)["details"]):
                if n["composing"]:
                    continue
                def edit(v):
                    item = v["details"]
                    for key in path:
                        item = item[key]
                    item["composing"] = True
                with self.subTest(scenario=scenario, path=path):
                    self.reject(edit, scenario)

    def test_common_defaults_and_explicit_boolean(self):
        v = D["good"]("native-refusals-channel")
        record = next(r for r in v["snapshots"] if r["stage"] == "composition-active")
        n = record["native"]["native"]
        g = record["composer"]
        args = (n, C["INITIAL"], v["viewport"])
        with self.assertRaises(RuntimeError):
            NATIVE(*args)
        NATIVE(*args, expected_composing=True)
        with self.assertRaises(RuntimeError):
            GEOMETRY(g, v["fixture"], v["target"], C["INITIAL"])
        GEOMETRY(g, v["fixture"], v["target"], C["INITIAL"], expected_composing=True)
        for bad in (None, 0, 1, "true", [], {}):
            with self.subTest(expected=bad):
                with self.assertRaises(RuntimeError):
                    NATIVE(*args, expected_composing=bad)
                with self.assertRaises(RuntimeError):
                    GEOMETRY(g, v["fixture"], v["target"], C["INITIAL"], expected_composing=bad)
        for bad in (None, 0, 1, "true"):
            with self.subTest(observed=bad):
                self.reject(lambda x: next(r for r in x["snapshots"] if r["stage"] == "composition-active")["native"]["native"].update(composing=bad))


if __name__ == "__main__":
    unittest.main()
