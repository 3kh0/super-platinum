"""Fake run_phase lifecycle regressions. No processes, Win32 calls, or captures."""
import json
from pathlib import Path
import runpy
import tempfile
import types
import unittest
from unittest.mock import patch

R=runpy.run_path(str(Path(__file__).with_name('check-protected-owned-send-native.py')))


class Lifecycle(unittest.TestCase):
    def exercise(self,body_error=None,pointer_error=None,handle_error=False,terminal=None,late_terminal=False,poll_error=False,write_error=False,storage_error=None):
        events=[];closed=[];expected=R['ready_tuple']('a'*32,123,'newer-before-prepare-channel','exercise','protected-owned-send');terminal={'ok':True} if terminal is None else terminal
        class Native:
            def __init__(self,fixture,log,binary,env,*,after_exit=None):
                self.after_exit=after_exit
                self.log=log;self.cleanup_errors=[];self.exited=False
                self.polls=0
                def poll():
                    if self.exited:
                        self.polls+=1
                        if poll_error and (body_error or self.polls==3):raise OSError('final poll failed')
                    return 0 if self.exited else None
                self.process=types.SimpleNamespace(pid=123,poll=poll)
            def __enter__(self):events.append('enter');return self
            def __exit__(self,*args):
                events.append('exit');self.exited=True
                try:self.after_exit(self.log.parent/'sp-fixture-child')
                except BaseException as error:
                    self.cleanup_errors.append({'stage':'after_exit','message':str(error)})
                    if args[1] is None:raise
                    args[1].add_note(str(error))
                finally:events.append('temporary-cleanup')
            def require_running(self):
                if self.exited:raise RuntimeError('already exited')
            def receipts(self,prefix):
                return [expected] if prefix==R['WAITING'] else [terminal] if prefix==R['PREFIX'] else []
            def capture(self,path):self.require_running();events.append('failure-capture')
        owned=[]
        def factory(*a,**kw):n=Native(*a,**kw);owned.append(n);return n
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
        def audit(storage,receipt,*,temporary_root):
            self.assertTrue(owned[0].exited)
            self.assertNotIn('temporary-cleanup',events)
            self.assertEqual(temporary_root,owned[0].log.parent/'sp-fixture-child')
            events.append('storage-audit')
            if storage_error:raise storage_error
            return {'ok':True,'root':str(temporary_root/'owned')}
        persisted=[]
        persist=R['persist_evidence']
        def retain_evidence(path,evidence,primary=None):
            persisted.append(evidence)
            persist(path,evidence,primary)
        mocked=dict(NativeFixture=factory,require_contract=lambda:None,sha=lambda p:'digest',open_process=lambda pid:(api,42),normalize=normalize,persist_evidence=retain_evidence,
                    validate_artifacts=validate,validate_closed_storage=audit,validate_pointer_cleanup=lambda p:None,validate_exit=lambda *a:None,
                    png=lambda path,dimensions:events.append('failure-png-validated') or [100,100],
                    closed_receipts=lambda log,prefix:[expected] if prefix==R['WAITING'] else [terminal]* (2 if late_terminal else 1) if prefix==R['PREFIX'] else [])
        with tempfile.TemporaryDirectory() as temp,patch.object(R['sys'],'platform','win32'),patch.dict(R['run_phase'].__globals__,mocked):
            p=Path(temp);error=None;result=None
            with patch.object(Path,'write_text',side_effect=OSError('write failed')) if write_error else patch.dict({},{}):
                try:result=R['run_phase']('protected-owned-send','newer-before-prepare-channel','exercise','a'*32,p,p/'fake.exe',p,'digest')
                except BaseException as caught:error=caught
            receipt=persisted[-1]
        self.assertLess(events.index('restore'),events.index('exit'))
        self.assertLess(events.index('exit'),events.index('close-handle'))
        self.assertEqual(closed,[42]);self.assertEqual(receipt['processExited'],not poll_error)
        self.assertEqual(receipt['cleanupProcessIdentity'],dict(creation_filetime=10,exit_filetime=20))
        return result,error,receipt,events

    def test_success_restores_before_exit_and_closes_retained_handle(self):
        result,error,receipt,events=self.exercise()
        self.assertIsNone(error);self.assertTrue(result['ok']);self.assertTrue(receipt['closedLogRevalidated']);self.assertEqual(receipt['cleanupErrors'],[])
        self.assertLess(events.index('exit'),events.index('storage-audit'))
        self.assertLess(events.index('storage-audit'),events.index('temporary-cleanup'))
        self.assertTrue(receipt['closedStorage']['ok'])
        self.assertEqual(receipt['storageAuditExitIdentity'],receipt['exitIdentity'])

    def test_storage_audit_failure_is_retained_and_cleanup_still_finishes(self):
        primary=RuntimeError('SQL authority mismatch')
        result,error,receipt,events=self.exercise(storage_error=primary)
        self.assertIs(error,primary);self.assertIsNone(result);self.assertFalse(receipt['ok'])
        self.assertIn('SQL authority mismatch',receipt['error'])
        self.assertNotIn('closedStorage',receipt)
        self.assertLess(events.index('storage-audit'),events.index('temporary-cleanup'))
        self.assertLess(events.index('temporary-cleanup'),events.index('close-handle'))

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
        self.assertEqual(str(error),'Native protected-owned-send failed: '+terminal['error'])
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
        self.assertEqual(str(error),'Native protected-owned-send failed: '+terminal['error'])
        self.assertEqual(receipt['closedFailureLog']['terminals'],[terminal,terminal])
        self.assertIn('Closed failure terminal changed',receipt['failureAuditError'])
        self.assertNotIn('closedFailureEvidenceValidated',receipt);self.assertFalse(receipt['ok'])

    def test_nonboolean_failure_cannot_bypass_positive_validator(self):
        primary=RuntimeError('strict positive schema refused')
        _,error,receipt,events=self.exercise(body_error=primary,terminal={'ok':0,'error':'counterfeit'})
        self.assertIs(error,primary);self.assertIn('positive-validator',events)
        self.assertFalse(receipt['ok'])

    def test_final_poll_failure_closes_handle_and_preserves_primary(self):
        primary=RuntimeError('actual validation failure')
        _,error,receipt,_=self.exercise(body_error=primary,poll_error=True)
        self.assertIs(error,primary);self.assertFalse(receipt['ok'])
        self.assertTrue(any('final process poll' in x for x in receipt['cleanupErrors']))
        self.assertTrue(any('final process poll' in x for x in primary.__notes__))

    def test_final_poll_failure_alone_prevents_success(self):
        result,error,receipt,_=self.exercise(poll_error=True)
        self.assertIsNone(result);self.assertIsInstance(error,RuntimeError)
        self.assertFalse(receipt['ok']);self.assertIn('final process poll',str(error))

    def test_phase_write_failure_preserves_primary_and_alone_blocks_success(self):
        primary=RuntimeError('actual validation failure')
        _,error,receipt,_=self.exercise(body_error=primary,write_error=True)
        self.assertIs(error,primary);self.assertFalse(receipt['ok'])
        self.assertTrue(any('receipt write' in x for x in primary.__notes__))
        result,error,receipt,_=self.exercise(write_error=True)
        self.assertIsNone(result);self.assertIsInstance(error,RuntimeError)
        self.assertFalse(receipt['ok']);self.assertIn('receipt write',str(error))

    def test_outer_write_and_output_failures_never_mask_primary_or_succeed(self):
        for fail_write in (True,False):
            for with_primary in (True,False):
                primary=RuntimeError('phase failed') if with_primary else None
                def phase(*args):
                    if primary:raise primary
                    return {'native':{},'closedStorage':{'ok':True}}
                saved=[]
                persist=R['persist_evidence']
                def retain(path,evidence,error=None):saved.append(evidence);persist(path,evidence,error)
                overrides=dict(require_contract=lambda:None,sha=lambda p:'digest',run_phase=phase,
                               validate_closed_storage=lambda *a:{},persist_evidence=retain)
                with tempfile.TemporaryDirectory() as temp,patch.dict(R['run'].__globals__,overrides):
                    with patch.object(Path,'write_text',side_effect=OSError('write failed')) if fail_write else patch.dict({},{}):
                        with patch('builtins.print',side_effect=None if fail_write else OSError('output failed')):
                            with self.assertRaises(BaseException) as caught:
                                R['run']('protected-owned-send','newer-before-prepare-channel',Path('fake.exe'),Path(temp))
                self.assertFalse(saved[-1]['ok'])
                if primary:self.assertIs(caught.exception,primary);self.assertTrue(primary.__notes__)
                else:self.assertIsInstance(caught.exception,RuntimeError)


if __name__=='__main__':unittest.main()
