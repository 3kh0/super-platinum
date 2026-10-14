"""Private temporary-file validator tests; no native app or clipboard access."""
import importlib.util
import os
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('file_paste_check', Path(__file__).with_name('check-file-paste-native.py'))
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class FilePasteTests(unittest.TestCase):
    def fixture(self, root):
        paths = []
        for index, (name, data) in enumerate(module.EXPECTED):
            path = root / f'100-200-{index}-pasted.txt'
            path.write_bytes(data)
            paths.append(str(path))
        return {'paths': paths, 'names': [name for name, _ in module.EXPECTED]}

    def test_native_failure_is_retained_before_missing_stage_error(self):
        evidence = {'stages': []}
        receipt = {'ok': False, 'error': 'precise native failure'}
        with self.assertRaisesRegex(RuntimeError, 'precise native failure'):
            module.validate_final(evidence, module.SPEC.fixture, [receipt])
        self.assertEqual(evidence['native'], receipt)

    def test_exact_bytes_and_two_distinct_files(self):
        with tempfile.TemporaryDirectory(prefix='sp-file-paste-test-') as directory:
            root = Path(directory).resolve()
            receipt = self.fixture(root)
            result = module.verify_files(root, receipt)
            self.assertEqual([entry['bytes'] for entry in result], [5, 8])

    @unittest.skipUnless(os.name == 'nt', 'Windows verbatim paths')
    def test_rust_windows_canonical_paths_match_runner_root(self):
        with tempfile.TemporaryDirectory(prefix='sp-file-paste-test-') as directory:
            root = Path(directory).resolve()
            receipt = self.fixture(root)
            receipt['paths'] = ['\\\\?\\' + path for path in receipt['paths']]
            self.assertEqual(len(module.verify_files(root, receipt)), 2)

    def test_names_must_match_original_metadata(self):
        with tempfile.TemporaryDirectory(prefix='sp-file-paste-test-') as directory:
            root = Path(directory).resolve()
            receipt = self.fixture(root)
            for names in [None, [], ['channel.txt', 'thread.txt'], list(reversed(receipt['names']))]:
                with self.subTest(names=names), self.assertRaises(RuntimeError):
                    module.verify_files(root, {**receipt, 'names': names})

    def test_extra_or_wrong_bytes_rejected(self):
        with tempfile.TemporaryDirectory(prefix='sp-file-paste-test-') as directory:
            root = Path(directory).resolve()
            receipt = self.fixture(root)
            extra = root / 'stale.txt'
            extra.write_bytes(b'8')
            with self.assertRaises(RuntimeError):
                module.verify_files(root, receipt)
            extra.unlink()
            Path(receipt['paths'][0]).write_bytes(b'wrong')
            with self.assertRaises(RuntimeError):
                module.verify_files(root, receipt)

    def test_outside_relative_and_duplicate_paths_rejected(self):
        with tempfile.TemporaryDirectory(prefix='sp-file-paste-test-') as directory:
            parent = Path(directory).resolve()
            root = parent / 'private'
            root.mkdir()
            receipt = self.fixture(root)
            outside = parent / 'outside-channel.txt'
            outside.write_bytes(module.EXPECTED[0][1])
            for paths in [[str(outside), receipt['paths'][1]],
                          ['relative-channel.txt', receipt['paths'][1]],
                          [receipt['paths'][0], receipt['paths'][0]]]:
                with self.subTest(paths=paths), self.assertRaises(RuntimeError):
                    module.verify_files(root, {'paths': paths, 'names': receipt['names']})

    def test_missing_and_wrong_shape_paths_rejected(self):
        with tempfile.TemporaryDirectory(prefix='sp-file-paste-test-') as directory:
            root = Path(directory).resolve()
            for receipt in [{}, {'paths': []}, {'paths': [1, 2]}, {'paths': ['a', 'b', 'c']}]:
                with self.subTest(receipt=receipt), self.assertRaises(RuntimeError):
                    module.verify_files(root, receipt)


if __name__ == '__main__':
    unittest.main()
