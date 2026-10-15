"""Full public-path synthetic receipts and adversarial mutations; no native IO."""
import copy
from pathlib import Path
import runpy
import unittest
P=Path(__file__).resolve().parent
D=runpy.run_path(str(P/'protected_adoption_test_data.py'));C=D['C']

class AdoptionReceipts(unittest.TestCase):
    @classmethod
    def setUpClass(cls):cls.samples={s:D['good'](s) for s in C['SCENARIOS']}
    def rejected(self,scenario,edit):
        value=copy.deepcopy(self.samples[scenario]);edit(value)
        with self.assertRaises((RuntimeError,KeyError,TypeError,ValueError)):D['validate'](value)
    def test_all_seven_normal_and_narrow_public_receipts(self):
        for s in C['SCENARIOS']:
            for f in C['FIXTURES']:
                with self.subTest(scenario=s,fixture=f):D['validate'](D['good'](s,f))
    def test_exact_top_and_stage_keys(self):
        self.rejected('adopt-channel',lambda v:v.update(extra=True))
        self.rejected('adopt-channel',lambda v:v['snapshots'][0].update(extra=True))
    def test_each_stage_and_capture_required(self):
        for s in C['SCENARIOS']:
            self.rejected(s,lambda v:v['snapshots'].pop())
            self.rejected(s,lambda v:v['captures'].pop(next(iter(v['captures']))))
    def test_no_transport_session_or_cache_capability(self):
        for key in ('sessionPresent','transportPresent','cachePresent'):
            self.rejected('adopt-channel',lambda v,k=key:v['snapshots'][0]['projection'].__setitem__(k,True))
    def test_whole_inventory_and_sibling_preservation(self):
        for s in C['SCENARIOS']:
            self.rejected(s,lambda v:v['snapshots'][2]['durable']['thread'].__setitem__('clock',99))
            self.rejected(s,lambda v:v['snapshots'][-1]['durable']['channel'].__setitem__('extra',True))
    def test_false_native_lock_and_early_release(self):
        for key,value in [('readOnly',False),('admission',None)]:
            self.rejected('native-refusals-channel',lambda v,k=key,x=value:next(r for r in v['snapshots'] if r['stage']=='expiry-locked')['native'].__setitem__(k,x))
        self.rejected('native-refusals-channel',lambda v:next(r for r in v['snapshots'] if r['stage']=='expiry-restored')['native'].__setitem__('readOnly',True))
    def test_native_trace_cannot_appear_before_its_handler(self):
        self.rejected('adopt-channel',lambda v:v['snapshots'][0]['projection'].__setitem__('nativeAdmission',v['finalProjection']['nativeAdmission']))
    def test_no_clear_or_replacement_capture(self):
        self.rejected('adopt-channel',lambda v:v['finalProjection']['nativeAdmission'].append(dict(kind='clear-ack',evidence={})))
        self.rejected('native-refusals-channel',lambda v:v['finalProjection']['nativeAdmission'].append(dict(kind='capture',evidence={})))
    def test_capture_scope_visit_selection_exact(self):
        for key in ('scope','visit','direction'):
            self.rejected('adopt-channel',lambda v,k=key:v['details']['preparedBefore']['native'].__setitem__(k,'other'))
    def test_unobscured_geometry_not_weakened(self):
        self.rejected('adopt-thread',lambda v:v['snapshots'][0]['composer']['hitSamples']['field']['points'][0].__setitem__('owned',False))
        self.rejected('adopt-channel',lambda v:v['snapshots'][0]['pages'][0]['hits']['panel']['points'][0].__setitem__('owned',False))
    def test_whole_dialog_rows_required(self):
        self.rejected('adopt-channel',lambda v:v['snapshots'][1]['pages'].pop())
        self.rejected('adopt-thread',lambda v:v['snapshots'][1]['pages'][1]['surfaces']['selected'].__setitem__('height',10000))
    def test_actual_button_hit_and_copy_required(self):
        self.rejected('adopt-channel',lambda v:v['details']['consent']['proof'].__setitem__('text','Send'))
        self.rejected('cancel-boundaries-channel',lambda v:v['details']['cancelBefore']['proof']['hit']['points'][3].__setitem__('owned',False))
    def test_refused_input_is_not_claimed_routed(self):
        self.rejected('adopt-channel',lambda v:v['snapshots'][-1]['projection'].__setitem__('refused',1))
        self.rejected('late-input-channel',lambda v:v['snapshots'][-1]['projection'].__setitem__('routed',0))
    def test_pending_failed_input_and_original_command_retained(self):
        for key,value in [('pending',None),('failedInputs',[]),('serial',2),('parked',True)]:
            self.rejected('failed-managed-write-channel',lambda v,k=key,x=value:next(r for r in v['snapshots'] if r['stage']=='failed')['projection']['retained'][0].__setitem__(k,x))
    def test_exact_retry_and_rebind_without_registration(self):
        self.rejected('failed-managed-write-channel',lambda v:v['details']['retryResult'].__setitem__('serialAfter',2))
        self.rejected('failed-managed-write-channel',lambda v:v['details']['resumeResult']['receipt'].__setitem__('reboundFrom',None))
    def test_registration_and_routes_are_same_editor(self):
        self.rejected('adopt-channel',lambda v:v['snapshots'][-1]['projection']['routes']['channel'].__setitem__('writer',D['uid'](999)))
        self.rejected('owner-replacement-channel',lambda v:v['details']['recoveredResult'].__setitem__('current',True))
    def test_no_install_on_cancel_before_or_rollback_after(self):
        self.rejected('cancel-boundaries-channel',lambda v:v['details']['canceledBefore'].__setitem__('installed',{}))
        self.rejected('cancel-boundaries-channel',lambda v:v['details']['canceledAfter'].__setitem__('installed',None))
    def test_old_cloned_lane_is_actually_retired(self):
        self.rejected('adopt-channel',lambda v:v['details']['oldLaneRefusal'].__setitem__('result','Applied'))
        self.rejected('adopt-thread',lambda v:v['details']['oldLaneRefusal'].__setitem__('afterSerial',2))
    def test_expiry_not_inferred_from_release_request(self):
        self.rejected('native-refusals-channel',lambda v:v['details'].__setitem__('expiryElapsedMs',3000))
        self.rejected('native-refusals-channel',lambda v:v['details']['expiredWritable'].__setitem__('admission','2'))
    def test_actual_two_input_aba_not_same_text_event(self):
        self.rejected('native-refusals-channel',lambda v:v['details'].pop('abaChangedInput'))
        self.rejected('native-refusals-channel',lambda v:v['snapshots'][-1]['projection']['localLanes'][0].__setitem__('serial',2))
        self.rejected('native-refusals-channel',lambda v:v['details']['sameTextInput'].__setitem__('flushError',None))
    def test_direction_and_unicode_selection_preserved(self):
        self.rejected('native-refusals-channel',lambda v:v['details']['selectionChange']['native'].__setitem__('direction','backward'))
        self.rejected('adopt-thread',lambda v:v['details']['typed']['observed']['native'].__setitem__('start',25))
    def test_replacement_must_remount_exact_scope(self):
        self.rejected('owner-replacement-channel',lambda v:v['details']['replacementReady'].__setitem__('ready',False))
        self.rejected('failed-managed-write-channel',lambda v:v['details']['returnReady']['native'].__setitem__('visit',v['details']['awayReady']['native']['visit']))
    def test_final_snapshot_cannot_drift(self):
        self.rejected('adopt-channel',lambda v:v['finalInventory']['channel'].__setitem__('clock',8))
        self.rejected('cancel-boundaries-channel',lambda v:v['finalProjection'].__setitem__('registry','protected-999'))
    def test_consistent_refusal_from_different_real_attempt_is_rejected(self):
        def replace(v):
            wrong=v['details']['compositionResult']['attempt']
            original=v['details']['selectionResult']['attempt']
            def walk(x):
                if isinstance(x,dict):
                    if x.get('kind')=='install-refused' and x.get('attempt')==original:x['attempt']=wrong
                    for y in x.values():walk(y)
                elif isinstance(x,list):
                    for y in x:walk(y)
            walk(v)
        self.rejected('native-refusals-channel',replace)
    def test_input_and_interaction_cannot_claim_foreign_mount(self):
        for scenario,key,wrapped in [('adopt-channel','typed',True),('adopt-thread','typed',True),('native-refusals-channel','selectionChange',False),('native-refusals-channel','compositionStart',False),('native-refusals-channel','compositionEnd',False),('native-refusals-channel','abaChangedInput',True),('native-refusals-channel','sameTextInput',True),('failed-managed-write-channel','typedAfterRetry',True)]:
            def mutate(v):
                x=v['details'][key]['observed'] if wrapped else v['details'][key]
                x['native'].update(scope='foreign',visit='foreign')
            with self.subTest(scenario=scenario,key=key):self.rejected(scenario,mutate)
        self.rejected('adopt-channel',lambda v:v['details']['nativeInitial'].__setitem__('visit','foreign'))
    def test_actual_held_token_must_match_the_recorded_native_lock(self):
        def mutate(v):
            row=next(r for r in v['snapshots'] if r['stage']=='expiry-locked')
            row['native']['admission']='99999';row['composer']['admission']='99999'
        self.rejected('native-refusals-channel',mutate)
    def test_input_before_value_is_the_actual_preceding_selection(self):
        self.rejected('adopt-channel',lambda v:v['details']['typed']['observed']['before'].__setitem__('text','foreign'))
        self.rejected('native-refusals-channel',lambda v:v['details']['sameTextInput']['observed']['before'].__setitem__('text',C['INITIAL']))
    def test_replacement_source_is_full_retained_draft_not_compact_label(self):
        def mutate(v):
            for r in v['snapshots']:
                for a in r['projection']['fixtureActions']:
                    if a['kind']=='shell-replaced':a['before']['channelDraft']={'text':'foreign'}
            v['finalProjection']=copy.deepcopy(v['snapshots'][-1]['projection'])
        self.rejected('owner-replacement-channel',mutate)
    def test_newer_input_cannot_fabricate_a_new_install_result(self):
        def mutate(v):
            v['snapshots'][-1]['projection']['result']={'kind':'installed','attempt':'protected-999'}
            v['finalProjection']=copy.deepcopy(v['snapshots'][-1]['projection'])
        self.rejected('adopt-channel',mutate)
    def test_owned_control_still_requires_complete_label_geometry(self):
        for mutation in ('missing','overflow','range'):
            def mutate(v):
                b=v['details']['consent']['proof']
                if mutation=='missing':b.pop('textGeometry')
                elif mutation=='overflow':b['textGeometry']['scrollWidth']+=20
                else:b['textGeometry']['rects'][0]['width']+=50
            with self.subTest(mutation=mutation):self.rejected('adopt-channel',mutate)
    def test_prepared_stale_open_control_is_not_accepted(self):
        def mutate(v):
            page=next(r for r in v['snapshots'] if r['stage']=='prepared')['pages'][0]
            page['buttons'].pop('protected-adoption-check')
            page['buttons']['protected-adoption-open']=page['buttons'].pop('protected-adoption-adopt')
            page['buttons']['protected-adoption-open'].update(id='protected-adoption-open',text='Review draft')
        self.rejected('adopt-channel',mutate)

if __name__=='__main__':unittest.main()
