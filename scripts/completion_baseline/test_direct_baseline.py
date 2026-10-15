"""Offline protocol tests; no executable is invoked."""
import copy
import importlib.util
import json
from pathlib import Path
import unittest

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('direct_runner', HERE / 'run_direct_baseline.py')
RUNNER = importlib.util.module_from_spec(spec)
spec.loader.exec_module(RUNNER)


class Protocol(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.reference = RUNNER.VALIDATOR.reference(HERE.parents[1] / 'tmp/completion-baseline-before2/receipt.json')
        cls.expected = RUNNER.VALIDATOR.ranking(cls.reference['runs'][0])

    def data(self):
        value = copy.deepcopy(self.reference['runs'][0])
        value.update(experiment='completion-direct-v1', mode='direct')
        return value

    def validate(self, value):
        return RUNNER.validate_data(value, self.reference['rounds'], 0, 'direct', self.expected)

    def test_full_original_case_set_is_accepted_without_discarding_samples(self):
        value = self.data()
        self.assertIs(self.validate(value), value)
        self.assertEqual(len(RUNNER.VALIDATOR.ranking(value)), 72)

    def test_label_detail_and_order_are_part_of_contract(self):
        for field in ('label', 'detail'):
            value = self.data()
            value['inventories'][0]['cases'][0]['expected'][0][field] += ' changed'
            with self.assertRaises(ValueError): self.validate(value)
        value = self.data()
        value['inventories'][0]['cases'][0]['expected'].reverse()
        with self.assertRaises(ValueError): self.validate(value)

    def test_different_mode_is_rejected(self):
        value = self.data(); value['mode'] = 'legacy'
        with self.assertRaises(ValueError): self.validate(value)

    def test_missing_case_or_round_is_rejected(self):
        for remove_case in (True, False):
            value = self.data()
            if remove_case: value['inventories'][0]['cases'].pop()
            else: value['inventories'][0]['cases'][0]['samples_ms'].pop()
            with self.assertRaises(ValueError): self.validate(value)

    def test_nonfinite_and_boolean_samples_are_rejected(self):
        for bad in (float('nan'), float('inf'), True, -1):
            value = self.data(); value['inventories'][0]['cases'][0]['samples_ms'][0] = bad
            with self.assertRaises(ValueError): self.validate(value)


if __name__ == '__main__':
    unittest.main()
