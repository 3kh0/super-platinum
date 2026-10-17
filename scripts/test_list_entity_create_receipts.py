"""Constructed parser tests only; these are not native receipts."""
import copy
import unittest
import ast
from pathlib import Path
import list_entity_create_receipts as r


from list_entity_create_samples import sample


class ReceiptTests(unittest.TestCase):
    def validate(self, value):
        return r.validate(value, value["fixture"], {"version": 1, "nonce": "a" * 32, "pid": 123, "fixture": value["fixture"]}, [668, 380] if value["fixture"].endswith("-narrow") else [1280, 780])

    def test_all_sixteen_closed_cases(self):
        for prefix in r.CASES:
            for suffix in ("", "-narrow"):
                self.validate(sample(prefix + suffix))
        for fixture in ("slack-list-people-field-create", r.CASES[0] + "-other", r.CASES[0] + "-narrow-narrow"):
            with self.assertRaises(ValueError):
                r.case_for(fixture)

    def test_null_omitted_and_single_retained_pair_are_distinct(self):
        for channel in (False, True):
            self.assertNotEqual(r.options(channel, "null"), r.options(channel, "omitted"))
            single = r.options(channel, "single")
            self.assertEqual(len(next(iter(single["default_value_typed"].values()))), 2)
            for prefix in (r.CASES[0 if not channel else 4], r.CASES[2 if not channel else 6], r.CASES[3 if not channel else 7]):
                value = sample(prefix)
                value["after"]["list_metadata"]["schema"][1]["options"]["default_value"] = None
                if prefix.endswith("null-create"):
                    del value["after"]["list_metadata"]["schema"][1]["options"]["default_value"]
                with self.assertRaises(ValueError): self.validate(value)

    def test_mutated_owner_stage_wire_readback_and_safety_are_refused(self):
        mutations = [
            lambda x: x["owner"].update(pid=124),
            lambda x: x.update(account=None),
            lambda x: x.update(noTransport=False),
            lambda x: x.update(httpExecuted=1),
            lambda x: x.update(viewport=[1, 2]),
            lambda x: x["stages"].pop(),
            lambda x: x["stages"][7]["geometry"].update(observedVisible=False),
            lambda x: x["captures"].update(table=x["captures"]["observed"]),
            lambda x: x["ledger"]["requests"].append(copy.deepcopy(x["ledger"]["requests"][3])),
            lambda x: x["ledger"]["requests"][3]["fields"].append(["id", "invented"]),
            lambda x: x["ledger"]["requests"][3]["fields"][5].__setitem__(1, "{}"),
            lambda x: x["ledger"]["finished"][2].update(applied=True),
            lambda x: x["population"]["records"][0]["fields"].append({"key": "people_observed"}),
            lambda x: x["configuration"]["steps"][2]["after"].update(choice=999),
            lambda x: x["configuration"]["steps"].reverse(),
            lambda x: x["configuration"]["ledgerAfter"]["requests"].append({}),
            lambda x: x["cancel"]["draft"]["model"].update(serial=99),
            lambda x: x["retired"].update(newEpoch=1),
            lambda x: x["cellCompatibility"].update(cellAttempts=1),
            lambda x: x["cellCompatibility"]["reopen"].update(selected=["U00000002"]),
            lambda x: x["cellCompatibility"]["typed"].update(multipleAdmitted=True),
        ]
        for change in mutations:
            value = copy.deepcopy(sample(r.CASES[0]))
            change(value)
            with self.assertRaises((ValueError, TypeError)):
                self.validate(value)

    def test_changed_step_order_and_truncated_single_are_refused(self):
        value = sample(r.CASES[0])
        value["configuration"]["steps"][4]["after"]["wire"]["default_value_typed"]["user"].pop()
        with self.assertRaises(ValueError): self.validate(value)

    def test_old_underspecified_geometry_and_missing_stage_custody_are_rejected(self):
        value = sample(r.CASES[0])
        for stage in value["stages"]:
            for key in ("ledger", "document", "population", "find", "draft", "custody", "configuration", "model", "initial", "typed", "reopen"):
                stage.pop(key, None)
            stage["geometry"] = {"ok": True, "viewport": value["viewport"]}
        value["stages"][7]["geometry"]["observedVisible"] = True
        with self.assertRaises(ValueError): self.validate(value)

    def test_every_emitted_stage_member_is_mandatory(self):
        base = sample(r.CASES[0])
        for i, stage in enumerate(base["stages"]):
            for key in stage:
                if key in ("stage", "geometry", "noSession", "noTransport"):
                    continue  # Existing first-level checks already require these.
                value = copy.deepcopy(base)
                del value["stages"][i][key]
                with self.subTest(stage=i, field=key), self.assertRaises(ValueError):
                    self.validate(value)
        for i in range(2, 8):
            for key in base["stages"][i]["geometry"]:
                if key in ("ok", "viewport"):
                    continue
                value = copy.deepcopy(base)
                del value["stages"][i]["geometry"][key]
                with self.subTest(stage=i, geometry=key), self.assertRaises(ValueError):
                    self.validate(value)

    def test_consistently_changed_seed_and_all_raw_copies_cannot_self_authorize(self):
        def alter(v, what):
            if isinstance(v, dict):
                if what == "document" and v.get("id") == "F_LIST_FIXTURE" and "list_metadata" in v:
                    next(c for c in v["list_metadata"]["schema"] if c["id"] == "C_NUMBER")["options"]["precision"] = 5
                if what == "population" and "records" in v and len(v["records"]) == 73:
                    v["records"] = [{"fields": []} for _ in range(73)]
                for child in v.values(): alter(child, what)
            elif isinstance(v, list):
                for child in v: alter(child, what)
        for what in ("document", "population"):
            value = sample(r.CASES[0])
            alter(value, what)
            with self.subTest(what=what), self.assertRaises(ValueError): self.validate(value)

    def test_stage_ledger_custody_owner_and_raw_state_mutations_fail(self):
        changes = [
            lambda v: v["stages"][2]["ledger"]["requests"].append({}),
            lambda v: v["stages"][3]["ledger"].update(writePending=False),
            lambda v: v["stages"][4]["ledger"]["finished"][0].update(applied=True),
            lambda v: v["stages"][5]["ledger"]["responses"].pop(),
            lambda v: v["stages"][6]["ledger"].update(readPending=True),
            lambda v: v["stages"][3]["custody"].update(user="U1"),
            lambda v: v["stages"][4]["custody"].update(intentPopulation={}),
            lambda v: v["stages"][5]["custody"].update(options={}),
            lambda v: v["stages"][6]["draft"].update(visit=2),
            lambda v: v["stages"][6]["configuration"]["model"].update(selection=3),
            lambda v: v["stages"][7]["draft"].update(observed=None),
            lambda v: v["stages"][8].update(custody={}),
            lambda v: v["retired"]["held"].update(readPending=False),
            lambda v: v["stages"][9]["ledger"]["requests"].append({}),
        ]
        for change in changes:
            value = sample(r.CASES[0]); change(value)
            with self.subTest(change=change), self.assertRaises(ValueError): self.validate(value)

    def test_configuration_dom_remove_and_phase_focus_are_independently_checked(self):
        changes = [
            lambda v: v["configuration"]["steps"][1]["action"].update(focusOutlineFits=False),
            lambda v: v["configuration"]["steps"][2]["action"].update(rect=[-20, 0, 10, 10]),
            lambda v: v["configuration"]["steps"][2]["verified"]["dom"].update(checked=[]),
            lambda v: v["configuration"]["steps"][3]["verified"]["dom"]["owner"].update(choice="900"),
            lambda v: v["configuration"]["steps"][4]["verified"]["dom"].update(removes=["U00000002"]),
            lambda v: v["stages"][0]["geometry"].update(focusRetained=False),
            lambda v: v["stages"][1]["geometry"].update(focused="slack-list-rename-close"),
            lambda v: v["stages"][3]["geometry"]["controls"][0].update(disabled=True),
            lambda v: v["stages"][4]["geometry"]["controls"][1].update(hit=False),
            lambda v: v["stages"][5]["geometry"].update(panel=[0, 0, 2000, 900]),
            lambda v: v["stages"][6]["geometry"]["focus"].update(id="slack-list-rename-name", activeId="slack-list-rename-name"),
            lambda v: v["stages"][7]["reveal"].update(text="Observed an unrelated field"),
            lambda v: v["stages"][8]["geometry"].update(rowIds=[]),
            lambda v: v["stages"][8]["geometry"]["first"].update(createdText="Copied default"),
            lambda v: v["disabledNoReplay"]["before"].update(locked=False),
        ]
        for change in changes:
            value = sample(r.CASES[0]); change(value)
            with self.subTest(change=change), self.assertRaises(ValueError): self.validate(value)
        for fixture in (r.CASES[3], r.CASES[7]):
            value = sample(fixture)
            for i, step in enumerate(value["configuration"]["steps"]):
                if step["action"]["action"]["kind"] == "remove":
                    bad = copy.deepcopy(value)
                    bad["configuration"]["steps"][i]["verified"]["dom"]["removeFocus"].update(target="wrong", defaultId="wrong")
                    with self.subTest(fixture=fixture, step=i), self.assertRaises(ValueError): self.validate(bad)

    def test_cell_generation_type_selection_and_return_proofs_are_required(self):
        changes = [
            lambda v: v["cellCompatibility"]["initial"].update(candidateTypes=["checkbox"] * 12),
            lambda v: v["cellCompatibility"]["local"].update(generation="81"),
            lambda v: v["cellCompatibility"]["local"].update(selected=["U00000001"]),
            lambda v: v["cellCompatibility"]["local"].update(save=[0, 0, 0, 32]),
            lambda v: v["cellCompatibility"]["returned"].update(instance='["","R_LIST_004"]'),
            lambda v: v["cellCompatibility"]["reopen"].update(focusOutlineFits=False),
            lambda v: v["cellCompatibility"]["typed"].update(createDefaultIds={"user": ["U00000001"]}),
            lambda v: v["stages"][10]["reopen"].update(generation="80"),
        ]
        for change in changes:
            value = sample(r.CASES[0]); change(value)
            with self.subTest(change=change), self.assertRaises(ValueError): self.validate(value)

    def test_samples_have_no_validator_or_production_constructor_dependency(self):
        tree = ast.parse(Path(__file__).with_name("list_entity_create_samples.py").read_text(encoding="utf-8"))
        modules = {alias.name for node in ast.walk(tree) if isinstance(node, ast.Import) for alias in node.names}
        modules.update(node.module for node in ast.walk(tree) if isinstance(node, ast.ImportFrom))
        self.assertEqual(modules, {"copy", "json", "pathlib"})

    def test_closed_schema_nonfinite_rectangles_and_numeric_booleans_fail(self):
        for change in [
            lambda v: v.update(unobserved_claim=True),
            lambda v: v["owner"].update(version=True),
            lambda v: v["stages"][4].update(ordinal=True),
            lambda v: v["stages"][3]["geometry"]["panel"].__setitem__(0, float("nan")),
            lambda v: v["stages"][3]["geometry"]["panel"].__setitem__(2, float("inf")),
            lambda v: v["stages"][4]["geometry"]["controls"][0].update(hit=1),
            lambda v: v["configuration"]["final"]["dom"].update(checked=True),
            lambda v: v["captures"].update(table=123),
        ]:
            value = sample(r.CASES[0]); change(value)
            with self.subTest(change=change), self.assertRaises(ValueError): self.validate(value)
        value = sample(r.CASES[0])
        value["configuration"]["steps"][2]["action"]["action"]["value"] = "U00000001"
        with self.assertRaises(ValueError): self.validate(value)


if __name__ == "__main__":
    unittest.main()
