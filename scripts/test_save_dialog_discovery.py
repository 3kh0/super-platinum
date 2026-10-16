import copy,json,tempfile,unittest
from pathlib import Path
from save_dialog_discovery import owner,validate_probe,validate_review,read_json,TITLE,settle_samples
class Review(unittest.TestCase):
    def setUp(self):
        self.owner=owner('a'*32,42,12345)
        self.target=dict(runtimeId=[42,1,5],automationId='observed-only',name='Cancel',controlType='ControlType.Button',className='Button',enabled=True,offscreen=False,invoke=True,rect=[1,2,30,40])
        self.probe=dict(self.owner,ok=True,mode='probe',hwnd=99,title=TITLE,rect=[0,0,800,600],controls=[self.target])
        self.review=dict(self.owner,version=1,action='cancel',hwnd=99,target=self.target)
    def test_exact_observed_cancel(self):
        validate_probe(self.probe,self.owner)
        self.assertEqual(validate_review(self.review,self.probe,self.owner),self.target)
    def test_unobserved_save_stale_ambiguous_disabled_and_foreign_controls(self):
        for mutation in [lambda r:r.update(action='save'),lambda r:r.update(hwnd=100),lambda r:r.update(pid=43),lambda r:r.update(creation=1),lambda r:r.update(nonce='b'*32),lambda r:r.update(extra=True),lambda r:r.update(version=True),lambda r:r['target'].update(runtimeId=[42,1,6]),lambda r:r['target'].update(name='Save'),lambda r:r['target'].update(enabled=False),lambda r:r['target'].update(offscreen=True)]:
            r=copy.deepcopy(self.review);mutation(r)
            with self.assertRaises(RuntimeError):validate_review(r,self.probe,self.owner)
        p=copy.deepcopy(self.probe);p['controls'].append(self.target)
        with self.assertRaises(RuntimeError):validate_review(self.review,p,self.owner)
    def test_even_observed_save_or_disabled_button_is_never_allowed(self):
        for changes in [dict(name='Save'),dict(enabled=False),dict(offscreen=True),dict(invoke=False),dict(controlType='ControlType.Edit')]:
            p=copy.deepcopy(self.probe);p['controls'][0].update(changes)
            r=copy.deepcopy(self.review);r['target']=p['controls'][0]
            with self.assertRaises(RuntimeError):validate_review(r,p,self.owner)
    def test_probe_bounds_owner_and_runtime_identity(self):
        for change in [dict(pid=True),dict(hwnd=0),dict(title='Save'),dict(rect=[0,0,0,100]),dict(controls=[]),dict(controls=[{}]),dict(controls=[self.target]*81)]:
            with self.assertRaises(RuntimeError):validate_probe({**self.probe,**change},self.owner)
    def test_duplicate_json_and_size_are_refused(self):
        with tempfile.TemporaryDirectory() as td:
            p=Path(td)/'review.json'
            for content in ['{"action":"cancel","action":"save"}',' '*65537]:
                p.write_text(content,encoding='utf-8')
                with self.assertRaises(RuntimeError):read_json(p)
            p.write_text(json.dumps(self.review),encoding='utf-8')
            self.assertEqual(read_json(p),self.review)
    def test_owner_types_are_not_coerced(self):
        for args in [('a'*31,1,1),('A'*32,1,1),('a'*32,True,1),('a'*32,1,False)]:
            with self.assertRaises(RuntimeError):owner(*args)
class Settle(unittest.TestCase):
    def run_samples(self,values):
        self.now=0.;self.seen=[]
        def sample(index):
            self.seen.append(index)
            key=values(index)
            return None if key is None else {'fingerprint':key,'index':index}
        def sleep(seconds):self.now+=seconds
        return settle_samples(sample,clock=lambda:self.now,sleep=sleep)
    def test_stable_pixels_still_wait_for_minimum_observation_age(self):
        result,receipt=self.run_samples(lambda _:('same pixels','same geometry'))
        self.assertGreaterEqual(receipt['elapsedSeconds'],.8)
        self.assertGreaterEqual(receipt['stableSeconds'],.3)
        self.assertGreaterEqual(receipt['consecutive'],3)
        self.assertEqual(result['index'],len(self.seen)-1)
    def test_new_pixels_and_geometry_restart_stability(self):
        result,receipt=self.run_samples(lambda n:('old','rect1') if n<8 else ('new','rect2'))
        self.assertEqual(result['fingerprint'],('new','rect2'))
        self.assertGreaterEqual(receipt['elapsedSeconds'],1.1)
        self.assertGreaterEqual(receipt['stableSeconds'],.3)
    def test_ineligible_ownership_sample_resets_the_stable_sequence(self):
        result,receipt=self.run_samples(lambda n:None if n==8 else 'same')
        self.assertGreaterEqual(result['index'],12)
        self.assertGreaterEqual(receipt['stableSeconds'],.3)
    def test_continual_change_or_invalid_ownership_never_passes(self):
        for value in [lambda n:n,lambda _:None]:
            with self.assertRaisesRegex(RuntimeError,'did not settle'):self.run_samples(value)
            self.assertEqual(len(self.seen),40)
    def test_process_pointer_or_capture_errors_are_not_retried(self):
        calls=[]
        def fail(index):calls.append(index);raise RuntimeError('identity changed')
        with self.assertRaisesRegex(RuntimeError,'identity changed'):settle_samples(fail)
        self.assertEqual(calls,[0])
    def test_sample_overrunning_deadline_cannot_be_accepted(self):
        self.now=0.;calls=[]
        def sample(index):
            calls.append(index);self.now+=7
            return {'fingerprint':'same'}
        with self.assertRaisesRegex(RuntimeError,'did not settle'):
            settle_samples(sample,clock=lambda:self.now,sleep=lambda _:None)
        self.assertEqual(calls,[0])
if __name__=='__main__':unittest.main()
