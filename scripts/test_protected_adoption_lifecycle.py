"""Fake run_phase lifecycle regressions. No processes, Win32 calls, or captures."""
import json
from pathlib import Path
import runpy
import tempfile
import types
import unittest
from unittest.mock import patch

R=runpy.run_path(str(Path(__file__).with_name('check-protected-adoption-native.py')))


class Lifecycle(unittest.TestCase):
    def test_completed_contract_still_requires_exact_promoted_location(self):
        self.assertIs(R['CONTRACT_READY'],True)
        if Path(__file__).resolve().parent==R['SCRIPTS']:
            R['require_contract']()
        else:
            with self.assertRaisesRegex(RuntimeError,'preparation cannot launch'):R['require_contract']()

    def test_incomplete_contract_refuses_before_native_construction(self):
        factory = unittest.mock.Mock(side_effect=AssertionError('must not construct native process'))
        with patch.dict(R['require_contract'].__globals__, CONTRACT_READY=False), patch.dict(R['run_phase'].__globals__, NativeFixture=factory):
            with self.assertRaisesRegex(RuntimeError, 'preparation cannot launch'):
                R['run_phase']('protected-adoption','adopt-channel','exercise','a'*32,
                               Path('unused'),Path('unused.exe'),Path('unused'),'unused')
        factory.assert_not_called()

    def exercise(self,body_error=None,pointer_error=None,handle_error=False,terminal=None,late_terminal=False):
        events=[];closed=[];expected=R['ready_tuple']('a'*32,123,'adopt-channel','exercise');terminal={'ok':True} if terminal is None else terminal
        class Native:
            def __init__(self,fixture,log,binary,env):
                self.log=log;self.cleanup_errors=[];self.exited=False
                self.process=types.SimpleNamespace(pid=123,poll=lambda:0 if self.exited else None)
            def __enter__(self):events.append('enter');return self
            def __exit__(self,*args):events.append('exit');self.exited=True
            def require_running(self):
                if self.exited:raise RuntimeError('already exited')
            def receipts(self,prefix):
                return [expected] if prefix==R['WAITING'] else [terminal] if prefix==R['PREFIX'] else []
            def capture(self,path):self.require_running();events.append('failure-capture')
        owned=[]
        def factory(*a):n=Native(*a);owned.append(n);return n
        def normalize(native,evidence,stack,*args):
            evidence['pointer']={}
            def restore():
                native.require_running();events.append('restore')
                if pointer_error:raise pointer_error
            stack.callback(restore)
            return types.SimpleNamespace(check=lambda *a,**kw:None)
        def close(handle):
            closed.append(handle);events.append('close-handle');return not handle_error
        api=types.SimpleNamespace(times=lambda h:dict(creation_filetime=10,exit_filetime=20 if owned[0].exited else 0),kernel=types.SimpleNamespace(CloseHandle=close))
        def validate(*args):
            events.append('positive-validator')
            if body_error:raise body_error
            return {}
        mocked=dict(NativeFixture=factory,require_contract=lambda:None,sha=lambda p:'digest',open_process=lambda pid:(api,42),normalize=normalize,
                    validate_artifacts=validate,validate_pointer_cleanup=lambda p:None,validate_exit=lambda *a:None,
                    png=lambda path,dimensions:events.append('failure-png-validated') or [100,100],
                    closed_receipts=lambda log,prefix:[expected] if prefix==R['WAITING'] else [terminal]* (2 if late_terminal else 1) if prefix==R['PREFIX'] else [])
        with tempfile.TemporaryDirectory() as temp,patch.object(R['sys'],'platform','win32'),patch.dict(R['run_phase'].__globals__,mocked):
            p=Path(temp);error=None;result=None
            try:result=R['run_phase']('protected-adoption','adopt-channel','exercise','a'*32,p,p/'fake.exe',p,'digest')
            except BaseException as caught:error=caught
            receipt=json.loads((p/'protected-adoption-adopt-channel-exercise.json').read_text())
        self.assertLess(events.index('restore'),events.index('exit'))
        self.assertLess(events.index('exit'),events.index('close-handle'))
        self.assertEqual(closed,[42]);self.assertTrue(receipt['processExited'])
        self.assertEqual(receipt['cleanupProcessIdentity'],dict(creation_filetime=10,exit_filetime=20))
        return result,error,receipt,events

    def test_success_restores_before_exit_and_closes_retained_handle(self):
        result,error,receipt,events=self.exercise()
        self.assertIsNone(error);self.assertTrue(result['ok']);self.assertTrue(receipt['closedLogRevalidated']);self.assertEqual(receipt['cleanupErrors'],[])

    def test_pointer_cleanup_does_not_mask_primary_validator_failure(self):
        primary=RuntimeError('actual validation failure')
        result,error,receipt,events=self.exercise(primary,OSError('restore failed'))
        self.assertIs(error,primary);self.assertIsNone(result);self.assertFalse(receipt['ok'])
        self.assertEqual(receipt['error'],'RuntimeError: actual validation failure')
        self.assertIn('restore failed',receipt['pointerCleanupError']);self.assertTrue(receipt['cleanupErrors'])
        self.assertLess(events.index('failure-capture'),events.index('exit'))

    def test_pointer_failure_alone_blocks_success(self):
        failure=OSError('restore failed')
        result,error,receipt,_=self.exercise(pointer_error=failure)
        self.assertIs(error,failure);self.assertIsNone(result);self.assertFalse(receipt['ok']);self.assertIn('restore failed',receipt['error'])

    def test_handle_failure_preserves_primary_and_records_secondary(self):
        primary=RuntimeError('actual validation failure')
        _,error,receipt,_=self.exercise(primary,handle_error=True)
        self.assertIs(error,primary);self.assertIn('actual validation failure',receipt['error'])
        self.assertTrue(any('process handle cleanup' in v for v in receipt['cleanupErrors']))

    def test_handle_failure_alone_blocks_success(self):
        result,error,receipt,_=self.exercise(handle_error=True)
        self.assertIsNone(result);self.assertIsInstance(error,RuntimeError);self.assertFalse(receipt['ok'])
        self.assertIn('process handle cleanup',receipt['error'])

    def test_native_failure_surfaces_original_error_before_positive_schema(self):
        terminal={'ok':False,'error':'Protected DOM: status panel is obscured'}
        result,error,receipt,events=self.exercise(terminal=terminal)
        self.assertIsNone(result);self.assertIsInstance(error,RuntimeError)
        self.assertEqual(str(error),'Native protected-adoption failed: '+terminal['error'])
        self.assertNotIn('positive-validator',events)
        self.assertEqual(receipt['native'],terminal)
        self.assertEqual(receipt['terminalReceipts'],[terminal])
        self.assertEqual(receipt['closedFailureLog']['terminals'],[terminal])
        self.assertTrue(receipt['closedFailureEvidenceValidated'])
        self.assertEqual(receipt['binarySha256After'],'digest')
        self.assertEqual(receipt['failurePngDimensions'],[100,100])
        self.assertFalse(receipt['ok']);self.assertNotIn('closedLogRevalidated',receipt)

    def test_late_failure_terminal_is_recorded_without_masking_original_error(self):
        terminal={'ok':False,'error':'Native geometry failed'}
        _,error,receipt,_=self.exercise(terminal=terminal,late_terminal=True)
        self.assertEqual(str(error),'Native protected-adoption failed: '+terminal['error'])
        self.assertEqual(receipt['closedFailureLog']['terminals'],[terminal,terminal])
        self.assertIn('Closed failure terminal changed',receipt['failureAuditError'])
        self.assertNotIn('closedFailureEvidenceValidated',receipt);self.assertFalse(receipt['ok'])

    def test_nonboolean_failure_cannot_bypass_positive_validator(self):
        primary=RuntimeError('strict positive schema refused')
        _,error,receipt,events=self.exercise(body_error=primary,terminal={'ok':0,'error':'counterfeit'})
        self.assertIs(error,primary);self.assertIn('positive-validator',events)
        self.assertFalse(receipt['ok'])


if __name__=='__main__':unittest.main()
