import importlib.util
import unittest
from pathlib import Path
from canvas_native import validate, validate_closed, private_checks
from dataclasses import replace
import tempfile
from native_check import expected_viewport

p=Path(__file__).with_name('check-message-file-actions-native.py')
spec=importlib.util.spec_from_file_location('message_file_spec',p)
module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
SPEC=module.SPEC

class Receipts(unittest.TestCase):
    def receipt(self):
        return dict(ok=True,noSession=True,noTransport=True,stages=7,commands=3,
                    layout=dict(ok=True,viewport=expected_viewport(SPEC.fixture)),
                    **{key:True for key in SPEC.required})
    def test_exact_contract(self):
        validate(SPEC,SPEC.fixture,self.receipt())
    def test_every_required_flag_is_mandatory(self):
        for key in (*SPEC.required,'noSession','noTransport','ok'):
            r=self.receipt();r[key]=False
            with self.subTest(key=key),self.assertRaises(RuntimeError):validate(SPEC,SPEC.fixture,r)
    def test_count_and_layout_fail_closed(self):
        for value in [6,8,True,'7']:
            r=self.receipt();r['stages']=value
            with self.assertRaises(RuntimeError):validate(SPEC,SPEC.fixture,r)
        r=self.receipt();r['layout']['ok']=False
        with self.assertRaises(RuntimeError):validate(SPEC,SPEC.fixture,r)
    def test_wrong_viewport_refused(self):
        r=self.receipt();r['layout']['viewport']=[1,1]
        with self.assertRaises(RuntimeError):validate(SPEC,SPEC.fixture,r)

class PrivateFiles(unittest.TestCase):
    def test_default_specs_unchanged(self):
        with tempfile.TemporaryDirectory() as d:
            checks=private_checks(replace(SPEC,private_files=()),Path(d))
            self.assertEqual(checks,{SPEC.flag:'1','SUPER_PLATINUM_CANVAS_CAPTURE_ACK_DIR':d})
    def test_saved_db_is_owned_absolute_and_not_created(self):
        with tempfile.TemporaryDirectory() as d:
            checks=private_checks(SPEC,Path(d))
            path=Path(checks['SUPER_PLATINUM_SAVED_FILE_RECOVERY_DB'])
            self.assertTrue(path.is_absolute())
            self.assertEqual(path.parent,Path(d))
            self.assertEqual(path.name,'saved-file-recovery.sqlite')
            self.assertFalse(path.exists())
    def test_traversal_reserved_duplicate_and_existing_refused(self):
        with tempfile.TemporaryDirectory() as d:
            directory=Path(d)
            for name in ['', '.', '..', '../escape', r'..\escape', '/absolute', 'C:drive']:
                with self.subTest(name=name),self.assertRaises(RuntimeError):
                    private_checks(replace(SPEC,private_files=(('SUPER_PLATINUM_TEST',name),)),directory)
            for key in [SPEC.flag,'SUPER_PLATINUM_CANVAS_CAPTURE_ACK_DIR','PATH']:
                with self.assertRaises(RuntimeError):private_checks(replace(SPEC,private_files=((key,'x'),)),directory)
            duplicate=(('SUPER_PLATINUM_TEST','a'),('SUPER_PLATINUM_TEST','b'))
            with self.assertRaises(RuntimeError):private_checks(replace(SPEC,private_files=duplicate),directory)
            (directory/'saved-file-recovery.sqlite').write_bytes(b'owned')
            with self.assertRaises(RuntimeError):private_checks(SPEC,directory)

if __name__=='__main__':unittest.main()
