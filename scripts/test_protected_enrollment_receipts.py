"""Pure lifecycle tests. Never starts a desktop process."""
import copy
from pathlib import Path
import runpy
import tempfile
import unittest
from unittest.mock import patch

R = runpy.run_path(str(Path(__file__).with_name("check-protected-enrollment-native.py")))
C = runpy.run_path(str(Path(__file__).with_name("protected_enrollment_receipts.py")))
D = runpy.run_path(str(Path(__file__).with_name("protected_enrollment_test_data.py")))


class Semantic(unittest.TestCase):
    def reject(self,edit,scenario='enroll-channel'):
        value=D['good'](scenario);edit(value)
        with self.assertRaises(RuntimeError):D['validate'](value)

    def test_complete_six_case_two_size_contract(self):
        for scenario in C['SCENARIOS']:
            for fixture in C['FIXTURES']:
                with self.subTest(scenario=scenario,fixture=fixture):D['validate'](D['good'](scenario,fixture))
        self.assertEqual(sum(len(C['capture_names'](s)) for s in C['SCENARIOS'])*2,80)

    def test_whole_saved_inventory_and_sibling_preservation(self):
        for key,value in [('clock',9),('blocked',C['local']('unexpected')),('alternates',[{}]),('uncommitted',C['local']('extra'))]:
            self.reject(lambda v:v['snapshots'][1]['durable']['thread'].update({key:value}))
        self.reject(lambda v:v['finalInventory']['channel']['row']['state'].update(acknowledged_local_revision=1))
        self.reject(lambda v:v['finalInventory']['channel']['row']['lineage'].update(basis='Local'))

    def test_consent_requires_original_actual_capture_and_lock_without_clear(self):
        self.reject(lambda v:v['snapshots'][1]['projection']['nativeAdmission'][0]['evidence']['before'].update(input=7))
        self.reject(lambda v:v['snapshots'][-1]['projection']['nativeAdmission'].pop())
        self.reject(lambda v:v['snapshots'][-1]['projection']['nativeAdmission'].append(dict(kind='clear-ack',evidence={})))
        self.reject(lambda v:v['snapshots'][2]['projection']['nativeAdmission'].append(dict(kind='lock',evidence={})), 'late-typing-channel')

    def test_late_input_preserves_native_backward_selection_and_saved_clock(self):
        self.reject(lambda v:v['details']['typed']['before'].update(text='wrong original'),'late-typing-channel')
        self.reject(lambda v:v['snapshots'][3]['native']['before'].update(start=0),'late-typing-channel')
        self.reject(lambda v:v['details']['typed']['native'].update(direction='forward'),'late-typing-channel')
        self.reject(lambda v:v['snapshots'][3]['projection']['localWriters'][0].update(serial=1),'late-typing-channel')
        self.reject(lambda v:v['snapshots'][-1]['durable']['channel'].update(clock=4),'late-typing-channel')
        self.reject(lambda v:v['snapshots'][-1]['composer']['proof']['current'].update(input=0),'late-typing-channel')

    def test_owner_replacement_refuses_old_consent_and_requires_remount(self):
        self.reject(lambda v:v['snapshots'][3]['projection'].update(viewId=3),'owner-change-channel')
        self.reject(lambda v:v['snapshots'][3]['native']['native'].update(visit=v['snapshots'][0]['native']['native']['visit']),'owner-change-channel')
        self.reject(lambda v:v['details']['result'].update(error='ReviewRequired'),'owner-change-channel')
        self.reject(lambda v:v['snapshots'][3]['projection']['fixtureActions'].pop(),'owner-change-channel')

    def test_canceled_completion_requires_real_check_and_explicit_reload(self):
        self.reject(lambda v:v['details']['result'].update(current=True),'canceled-waiter-channel')
        self.reject(lambda v:v['details']['canceled'].update(pending=False),'canceled-waiter-channel')
        self.reject(lambda v:v['snapshots'][-2]['projection']['fixtureActions'].pop(),'canceled-waiter-channel')
        self.reject(lambda v:v['snapshots'][-2]['projection']['discovery'].update(status='Ready'),'canceled-waiter-channel')
        self.reject(lambda v:v['snapshots'][-1]['projection']['fixtureActions'].pop(),'canceled-waiter-channel')

    def test_failed_editor_keeps_unsaved_input_and_original_owner(self):
        self.reject(lambda v:v['snapshots'][2]['projection']['retained'][0].update(failedInputs=[]),'failed-editor-channel')
        self.reject(lambda v:v['snapshots'][2]['projection']['retained'][0].update(writerId=D['uid'](999)),'failed-editor-channel')
        self.reject(lambda v:v['snapshots'][-1]['durable']['channel'].update(local=C['local'](C['NEWER'])),'failed-editor-channel')
        self.reject(lambda v:v['details']['result'].update(phase='prepare-or-admit'),'failed-editor-channel')

    def test_modal_scroll_and_all_controls_keep_strict_geometry(self):
        self.reject(lambda v:v['snapshots'][1]['pages'][0]['surfaces']['header'].update(y=-20))
        self.reject(lambda v:v['snapshots'][1]['pages'][0]['hits']['body']['points'][0].update(owned=False))
        self.reject(lambda v:v['snapshots'][1]['pages'][1]['scroll']['after'].update(document=20))
        self.reject(lambda v:v['snapshots'][1]['pages'].pop())
        self.reject(lambda v:v['snapshots'][1]['pages'][0]['buttons']['protected-enrollment-submit']['hit']['points'][1].update(owned=False))
        self.reject(lambda v:v['snapshots'][2]['pages'][0]['buttons']['protected-enrollment-submit'].update(disabled=False))
        self.reject(lambda v:v['details']['submit'].update(bodyScroll=200))

    def test_original_whole_row_text_and_separate_pending_are_required(self):
        self.reject(lambda v:v['snapshots'][1]['pages'][0].update(text='truncated'))
        self.reject(lambda v:v['snapshots'][2]['pages'].pop(),'failed-editor-channel')
        self.reject(lambda v:v['snapshots'][2]['pages'][-1].update(text=C['INITIAL']),'failed-editor-channel')

    def test_no_network_capabilities_and_unknown_keys_fail_closed(self):
        for k in ('sessionPresent','transportPresent','cachePresent'):
            self.reject(lambda v:v['snapshots'][0]['projection'].update({k:True}))
        self.reject(lambda v:v['snapshots'][0]['projection'].update(extra='ignored'))
        self.reject(lambda v:v['snapshots'][0]['projection'].update(refused=1))
        self.reject(lambda v:v['details']['nativeInitial']['homeAnchor'].update(landed=False))

class Lifecycle(unittest.TestCase):
    def test_preparation_cannot_launch(self):
        if Path(C['__file__']).resolve().parent == C['SCRIPTS']:
            C['require_contract']()
        else:
            with self.assertRaises(RuntimeError): C['require_contract']()
        with patch.dict(C['require_contract'].__globals__,CONTRACT_READY=False):
            with self.assertRaises(RuntimeError):C['require_contract']()

    def test_exact_fixed_capture_contract(self):
        expected={
            "enroll-channel":("initial","dialog","prepared","result","reloaded"),
            "enroll-thread":("initial","dialog","prepared","result","reloaded"),
            "failed-editor-channel":("initial","input","dialog","result","reloaded"),
            "late-typing-channel":("initial","dialog","prepared","interleaved","result","reloaded"),
            "owner-change-channel":("initial","dialog","prepared","interleaved","result","reloaded"),
            "canceled-waiter-channel":("initial","dialog","enqueued","interleaved","result","reloaded"),
        }
        self.assertEqual(set(expected),set(C["SCENARIOS"]))
        for scenario,names in expected.items():
            self.assertEqual(C["stages"](scenario),names)
            with self.assertRaises(RuntimeError): C["stages"](scenario,"restart")

    def test_exact_ready_tuple_all_cases(self):
        for scenario in C["SCENARIOS"]:
            expected = dict(version=1,nonce="a"*32,pid=123,scenario=scenario,phase="exercise")
            self.assertEqual(R["ready_tuple"]("a"*32,123,scenario,"exercise"),expected)
            R["validate_ready"](expected,expected)
            for key,value in (("version",True),("pid",True),("nonce","b"*32),("phase","restart"),("extra",1)):
                bad=copy.deepcopy(expected);bad[key]=value
                with self.assertRaises(RuntimeError): R["validate_ready"](bad,expected)

    def test_invalid_readiness_owner_refused(self):
        for nonce,pid in (("a"*31,1),("A"*32,1),("a"*32,True),("a"*32,0)):
            with self.assertRaises(RuntimeError): R["ready_tuple"](nonce,pid,C["SCENARIOS"][0],"exercise")
        with self.assertRaises(RuntimeError): R["ready_tuple"]("a"*32,1,"consume-channel","exercise")

    def test_readiness_published_once_without_overwrite(self):
        expected=R["ready_tuple"]("a"*32,123,C["SCENARIOS"][0],"exercise")
        with tempfile.TemporaryDirectory() as temp:
            p=Path(temp);R["publish_ready"](p,expected)
            original=(p/"ready-exercise.json").read_bytes()
            with self.assertRaises(FileExistsError): R["publish_ready"](p,expected)
            self.assertEqual((p/"ready-exercise.json").read_bytes(),original)
            self.assertFalse((p/"ready-exercise.staged").exists())

if __name__ == "__main__": unittest.main()
