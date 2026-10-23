"""Portable List user-title oracle regressions; never launch the native app."""
import json
import hashlib
from pathlib import Path
import runpy
import unittest
import copy
from unittest.mock import patch
import slack_list_canvas_user_titles_source as S
import profile_user_title as U
import slack_list_canvas_user_titles as V
import list_canvas_user_titles_test_data as D

ROOT=Path(__file__).resolve().parents[1]
RUN=runpy.run_path(str(ROOT/'scripts/check-list-canvas-user-titles-native.py'))


class IsolatedHarnessTests(unittest.TestCase):
    def test_baseline_owned_harness_unchanged(self):
        pins=json.loads((ROOT/'tmp/list-canvas-user-titles-20261022/oracle/baseline-pins.json').read_text())
        for row in pins['files']:
            with self.subTest(path=row['path']):
                self.assertEqual(hashlib.sha256((ROOT/row['path']).read_bytes()).hexdigest(),row['sha256'])

    def test_original_closed_pipeline_keeps_same_globals(self):
        base=RUN['harness']()
        for name in ('run_fixture','validate_log'):
            self.assertIs(base[name].__globals__,base)
        self.assertEqual(base['PREFIX'],'slack-list-comments-compose-check: ')
        self.assertEqual(base['EDGE'],'slack-list-comments-compose-stage: ')
        self.assertEqual(base['WAITING'],'slack-list-comments-compose-waiting: ')
        original=runpy.run_path(str(ROOT/'scripts/check-slack-list-comments-compose-native.py'))
        self.assertIsNot(base['case_contract'],original['case_contract'])
        self.assertEqual(base['case_contract']('compose')[0],original['STAGES'])

    def test_only_exact_opt_in_cases_reach_fixture(self):
        cls=RUN['harness']()['NativeFixture'];old=cls.__bases__[0];captured=[]
        for case in RUN['CASES']:
            checks={'SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE':case,'other':'retained'}
            with patch.object(old,'__init__',lambda self,*a,**kw:captured.append((a,kw))):
                cls('slack-list',Path('log'),Path('binary'),checks)
            self.assertNotIn(RUN['FLAG'],checks)
            self.assertEqual(captured[-1][0][-1],dict(checks,**{RUN['FLAG']:'1'}))

    def test_foreign_or_preconfigured_mode_refused_before_constructor(self):
        cls=RUN['harness']()['NativeFixture'];old=cls.__bases__[0]
        with patch.object(old,'__init__') as init:
            for checks in ({},{'SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE':'compose'},
                           {'SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE':'user-title-known',RUN['FLAG']:'0'}):
                with self.assertRaises(RuntimeError):cls('slack-list',Path('log'),Path('binary'),checks)
            init.assert_not_called()


class OriginalSourceTests(unittest.TestCase):
    def setUp(self):
        self.model=dict(account='00000000-0000-4000-8000-000000000081',epoch=3)
        self.raw=S.BASE.read_bytes().decode().replace('"title":"Alex Canvas",',
            '"title":"Alex Canvas","title_blocks":'+json.dumps(U.title_blocks(),separators=(',',':'))+',',1)
        self.value=dict(case='user-title-recovery',fileResponseBytes=self.raw,requests=[],policy=[],pending=[],
                        sourcePresent=False,sourceObserved=True,sourceRaw=None,sourceReady=True,native=None,
                        knownDriver=dict(started=0,finished=0,active=0),driver=dict(started=0,finished=0,active=0))

    def validate(self,baseline=None):
        return S.member_ledger(self.value,self.model,'user-title-recovery',baseline if baseline is not None else {})

    def discovery(self):
        row=dict(serial=1,lane='absent',kind='discovery',method='api.getFlannelHttpUrl',
            args=dict(include_external_workspaces='true',_x_reason='client-needs-valid-flannel-url'),
            account=self.model['account'],epoch=3,team='T1',released=False)
        self.value['requests']=[row];self.value['pending']=[copy.deepcopy(row)]

    def release(self):
        self.value['requests'][0].update(released=True,response=dict(raw=dict(ok=True,
            url='https://edgeapi.slack.com/cache/T1',ttl_seconds=300),keys=None),
            disposition=dict(serial=1,receiverRetired=False,delivered=True))
        self.value['pending']=[]

    def test_exact_original_byte_insertion(self):self.validate()
    def test_original_unknown_title_fields_preserved(self):
        self.value['fileResponseBytes']=self.raw.replace('opaque_run','discarded_run',1)
        with self.assertRaises(RuntimeError):self.validate()
    def test_file_bytes_must_not_be_reserialized(self):
        self.value['fileResponseBytes']=json.dumps(json.loads(self.raw),sort_keys=True)
        with self.assertRaises(RuntimeError):self.validate()
    def test_original_held_release_is_append_only(self):
        self.discovery();b={};self.validate(b);self.release();self.validate(b)
        self.value['requests'][0]['response']['raw']['ttl_seconds']=301
        with self.assertRaises(RuntimeError):self.validate(b)
    def test_foreign_account_refused(self):
        self.discovery();self.value['requests'][0]['account']='foreign'
        with self.assertRaises(RuntimeError):self.validate()
    def test_foreign_discovery_endpoint_refused(self):
        self.discovery();self.release();self.value['requests'][0]['response']['raw']['url']='https://edgeapi.slack.com/cache/T2'
        with self.assertRaises(RuntimeError):self.validate()
    def test_wrong_discovery_args_refused(self):
        self.discovery();self.value['requests'][0]['args']['include_external_workspaces']='false'
        with self.assertRaises(RuntimeError):self.validate()
    def test_secret_field_refused(self):
        self.discovery();self.value['requests'][0]['args']['token']='synthetic-forbidden'
        with self.assertRaises(RuntimeError):self.validate()
    def test_fabricated_callback_disposition_refused(self):
        self.discovery();self.release();self.value['requests'][0]['disposition']['receiverRetired']=True
        with self.assertRaises(RuntimeError):self.validate()
    def test_missing_pending_receipt_refused(self):
        self.discovery();self.value['pending']=[]
        with self.assertRaises(RuntimeError):self.validate()
    def test_hidden_policy_not_inferred_from_default(self):
        self.value.update(case='user-title-hidden',policy=[dict(serial=1,method='users.prefs.get',
            raw=S.policy('user-title-known'),account=self.model['account'],epoch=3,team='T1',user='ULIST0001')])
        with self.assertRaises(RuntimeError):S.member_ledger(self.value,self.model,'user-title-hidden',{})
    def test_typed_row_without_current_source_is_not_a_grant(self):
        self.value['native']=S.member_raw();self.validate()
        self.assertFalse(self.value['sourcePresent'])


class CompleteReceiptTests(unittest.TestCase):
    def setUp(self):self.fixture,self.receipt=D.sample()
    def validate(self):V.validate(self.fixture,self.receipt,D.DIRECTORY,png=False)
    def bad(self,change):
        change(self.receipt)
        with self.assertRaises(RuntimeError):self.validate()
    def test_complete_known_both_widths(self):
        for narrow in (False,True):
            f,r=D.sample(narrow=narrow);V.validate(f,r,D.DIRECTORY,png=False)
    def test_complete_hidden_both_widths(self):
        for narrow in (False,True):
            f,r=D.sample('user-title-hidden',narrow);V.validate(f,r,D.DIRECTORY,png=False)
    def test_missing_stage(self):self.bad(lambda r:r['stages'].pop(4))
    def test_reordered_stage(self):self.bad(lambda r:r['stages'].reverse())
    def test_foreign_model_owner(self):self.bad(lambda r:r['stages'][4]['model'].__setitem__('epoch',99))
    def test_lost_opaque_wire(self):self.bad(lambda r:r['stages'][4]['proof']['expectedBlocks'][0]['elements'][0]['elements'][1].pop('opaque'))
    def test_ordinary_link_is_editable_not_atomic_label(self):
        for row in self.receipt['stages']:
            self.assertFalse(any(t['value']['type']=='link' for t in row['geometry']['native']['tokens']))
        self.validate()
    def test_ordinary_link_wire_stays_exact(self):
        self.bad(lambda r:r['stages'][0]['proof']['expectedBlocks'][0]['elements'][0]['elements'][3].__setitem__('url','https://foreign.invalid'))
    def test_fabricated_atomic_link_refused(self):
        self.bad(lambda r:r['stages'][0]['geometry']['native']['tokens'].insert(1,dict(value=copy.deepcopy(r['stages'][0]['proof']['expectedBlocks'][0]['elements'][0]['elements'][3]),label='Reference')))
    def test_derived_label_in_wire(self):self.bad(lambda r:r['stages'][4]['proof']['expectedBlocks'][0]['elements'][0]['elements'][-2].__setitem__('text',S.label(r['case'])))
    def test_query_user_borrowed_by_list(self):self.bad(lambda r:r['stages'][1]['model']['completion']['titleUsers'].__setitem__(S.LIST,copy.deepcopy(r['stages'][1]['model']['completion']['titleUsers'][S.CANVAS])))
    def test_stale_query_member(self):self.bad(lambda r:r['stages'][1]['model']['completion']['titleUsers'][S.CANVAS].__setitem__('current',False))
    def test_wrong_query_sequence(self):self.bad(lambda r:r['stages'][1]['model']['completion']['titleUsers'][S.CANVAS].__setitem__('generation','list-comment-member-1-mount-999-file-title-users'))
    def test_wrong_query_policy(self):self.bad(lambda r:r['stages'][1]['model']['completion']['titleUsers'][S.CANVAS]['policy'].__setitem__('user',{}))
    def test_wrong_query_gantry(self):self.bad(lambda r:r['stages'][1]['model']['completion']['titleUsers'][S.CANVAS]['policy'].__setitem__('gantry',{}))
    def test_wrong_original_choice(self):self.bad(lambda r:r['stages'][2]['proof']['inputs'][1].__setitem__('selected',S.LIST))
    def test_consumed_publication_cannot_insert(self):self.bad(lambda r:r['stages'][2]['proof']['inputs'][1]['publication'].__setitem__('current',False))
    def test_missing_click(self):self.bad(lambda r:r['stages'][5]['proof']['inputs'].pop())
    def test_foreign_click_receipt(self):self.bad(lambda r:r['stages'][5]['proof']['inputs'][-1]['value'].__setitem__('id','foreign'))
    def test_independent_draft_requires_own_generation(self):self.bad(lambda r:r['stages'][4]['model']['titleUsers']['observed']['titles']['users'].__setitem__('generation','list-comment-member-1-mount-10-file-title-users'))
    def test_known_subset_fast_path_requires_exact_suffix(self):self.bad(lambda r:r['stages'][4]['model']['titleUsers']['observed']['titles']['users'].__setitem__('generation','list-draft-title-independent-list-binding'))
    def test_independent_draft_requires_current_policy(self):self.bad(lambda r:r['stages'][4]['model']['titleUsers']['observed'].__setitem__('current',False))
    def test_independent_draft_requires_original_token(self):self.bad(lambda r:r['stages'][4]['model']['titleUsers']['observed']['titles']['tokens'][0].__setitem__('url','https://foreign.invalid'))
    def test_independent_draft_requires_raw_file(self):self.bad(lambda r:r['stages'][4]['model']['titleUsers']['observed']['titles']['files'][0]['source'].__setitem__('title_blocks',[]))
    def test_independent_draft_requires_raw_receipt(self):self.bad(lambda r:r['stages'][4]['model']['titleUsers']['observed']['titles']['files'][0].__setitem__('receipt',{}))
    def test_title_observation_cannot_borrow_candidate_channels(self):self.bad(lambda r:r['stages'][4]['model']['titleUsers']['observed']['titles']['files'][0]['effective'].__setitem__('channels',['CZMAP001','CAMAP001','CTAIL001']))
    def test_candidate_still_requires_original_channel_order(self):self.bad(lambda r:r['stages'][1]['proof']['sourceFiles']['rows'][0]['effective'].__setitem__('channels',['CAMAP001','CZMAP001','CTAIL001']))
    def test_title_status_projection_cannot_drop_guard_field(self):self.bad(lambda r:r['stages'][4]['model']['titleUsers']['observed']['titles']['files'][0]['effective'].pop('isUnknown'))
    def test_duplicate_user_dependency_refused(self):self.bad(lambda r:r['stages'][4]['model']['titleUsers']['observed']['titles']['required'].append(S.MEMBER))
    def test_draft_cannot_borrow_query_policy_without_read(self):self.bad(lambda r:r['stages'][4]['model']['completionSource']['titleUsers'].__setitem__('policy',[]))
    def test_reopen_requires_fresh_native_owner(self):self.bad(lambda r:r['stages'][4]['geometry']['editor'].__setitem__('sameNode',True))
    def test_earlier_remount_refused(self):self.bad(lambda r:r['stages'][2]['geometry']['editor'].__setitem__('sameNode',False))
    def test_journal_capture_custody(self):self.bad(lambda r:r['stages'][4]['proof']['journal'].__setitem__('capture','00000000-0000-4000-8000-000000000000'))
    def test_held_send_remains_readonly(self):self.bad(lambda r:r['stages'][5]['geometry']['editor'].__setitem__('editable',True))
    def test_transcript_requires_independent_receipt(self):self.bad(lambda r:r['stages'][6]['model']['titleAcquisition'].__setitem__('rows',[]))
    def test_transcript_cannot_borrow_draft_generation(self):self.bad(lambda r:r['stages'][6]['model']['titleAcquisition']['rows'][0]['titleUsers']['titles']['users'].__setitem__('generation','list-draft-title-independent-list-binding'))
    def test_transcript_original_currentness(self):self.bad(lambda r:r['stages'][6]['model']['titleAcquisition']['rows'][0].__setitem__('current',False))
    def test_painted_title_original_occurrence(self):self.bad(lambda r:r['stages'][6]['proof']['renderedFiles']['before']['tokens'][0].__setitem__('id',S.LIST))
    def test_painted_title_not_occluded(self):self.bad(lambda r:r['stages'][6]['proof']['renderedFiles']['before']['tokens'][0].__setitem__('hit',False))
    def test_no_second_send(self):self.bad(lambda r:r['stages'][7]['model']['composeLedger']['posts'].append(copy.deepcopy(r['stages'][7]['model']['composeLedger']['posts'][0])))
    def test_original_ack_channel(self):self.bad(lambda r:r['stages'][6]['model']['composeLedger']['messages'][0].__setitem__('channel','FOREIGN'))
    def test_original_png_path(self):self.bad(lambda r:r['stages'][0].__setitem__('capture','/foreign.png'))
    def test_final_model_exact(self):self.bad(lambda r:r['final'].__setitem__('epoch',999))


class IndependentRetirementTests(unittest.TestCase):
    def setUp(self):
        self.case='user-title-draft-recovery';self.job=dict(scope='old-binding',capture='capture',blocks=[{'opaque':'original'}],current=True,observed=None)
        self.journal=dict(blocks=self.job['blocks'],capture='capture',revision=9,attempt=None)
        fresh={**self.job,'scope':'fresh-binding'}
        self.rows=[]
        for name in ('retry-member','draft-replaced','draft-closed','draft-late','return-discovery','return-member','reopened'):
            job=self.job if name=='retry-member' else dict(current=False,observed=None) if name.startswith('draft-') else fresh
            proof=dict(journal=copy.deepcopy(self.journal))
            if name!='retry-member':proof['retiredDraft']=dict(job=copy.deepcopy(self.job),journal=copy.deepcopy(self.journal),memberSerial=4)
            if name.startswith('return-') or name=='reopened':proof['returnedDraft']=copy.deepcopy(fresh)
            source=dict(knownDriver=dict(active=0),sourcePresent=True,sourceReady=False,sourceRaw=S.member_source(),native=S.member_raw())
            proof['afterInvalidation']=dict(completionSource=dict(titleUsers=copy.deepcopy(source)))
            self.rows.append(dict(stage=name,proof=proof,model=dict(titleUsers=copy.deepcopy(job),completionSource=dict(titleUsers=source))))
    def validate(self):V.retired(self.rows,self.case)
    def test_exact_old_and_fresh_receipts(self):self.validate()
    def test_late_original_adoption_refused(self):
        self.rows[3]['model']['titleUsers']['current']=True
        with self.assertRaises(RuntimeError):self.validate()
    def test_late_observed_receipt_refused(self):
        self.rows[3]['model']['titleUsers']['observed']={}
        with self.assertRaises(RuntimeError):self.validate()
    def test_return_cannot_reuse_old_binding(self):
        self.rows[4]['model']['titleUsers']['scope']='old-binding'
        with self.assertRaises(RuntimeError):self.validate()
    def test_late_callback_cannot_rewrite_journal(self):
        self.rows[3]['proof']['journal']['revision']=10
        with self.assertRaises(RuntimeError):self.validate()
    def test_original_held_serial_exact(self):
        self.rows[3]['proof']['retiredDraft']['memberSerial']=2
        with self.assertRaises(RuntimeError):self.validate()
    def test_successor_receipt_not_query_scope(self):
        self.rows[-1]['model']['titleUsers']['scope']='list-comment-member-query'
        with self.assertRaises(RuntimeError):self.validate()


class PendingDraftTests(unittest.TestCase):
    def setUp(self):
        self.fixture,r=D.sample();self.row=copy.deepcopy(r['stages'][4]);self.row['stage']='draft-discovery'
        self.scope=self.row['proof']['journal']['scope'];self.raw=json.loads(D.original())
        self.row['model']['titleUsers'].update(loading=True,status='loading',observed=None)
        g=self.row['geometry'];g.update(editor=None,editorAbsent=True,dialogPresent=True,record=V.READ['KNOWN'],
            titleStatus=dict(text='Loading title details…',visible=True),titleRetry=None)
    def validate(self):
        V.draft_receipt(self.row,'user-title-draft-recovery',self.raw,self.scope)
        V.custody(self.row,'user-title-draft-recovery',self.scope,None)
    def test_pending_durable_draft_does_not_mount(self):self.validate()
    def test_completed_query_cannot_authorize_pending_draft(self):
        self.row['model']['titleUsers']['observed']=D.observed('user-title-known',self.raw,'list-comment-member-query')
        with self.assertRaises(RuntimeError):self.validate()
    def test_hidden_premature_editor_refused(self):
        self.row['geometry']['editorAbsent']=False
        with self.assertRaises(RuntimeError):self.validate()
    def test_foreign_pending_record_refused(self):
        self.row['geometry']['record']='R_LIST_001'
        with self.assertRaises(RuntimeError):self.validate()
    def test_missing_current_binding_refused(self):
        self.row['model']['titleUsers']['current']=False
        with self.assertRaises(RuntimeError):self.validate()
    def test_failed_visible_retry_admitted(self):
        self.row['stage']='draft-failed';self.row['model']['titleUsers'].update(loading=False,status='failed',error='Title details could not load. Your saved draft is retained.')
        self.row['geometry']['titleRetry']=dict(id='slack-list-comment-title-retry',visible=True,hit=True,disabled=False)
        self.validate()
    def test_retry_cannot_be_occluded(self):
        self.row['stage']='draft-failed';self.row['model']['titleUsers'].update(loading=False,status='failed',error='Title details could not load. Your saved draft is retained.')
        self.row['geometry']['titleRetry']=dict(id='slack-list-comment-title-retry',visible=True,hit=False,disabled=False)
        with self.assertRaises(RuntimeError):self.validate()


class KnownRecoveryLaneTests(unittest.TestCase):
    def setUp(self):
        self.case='user-title-draft-recovery';self.model=dict(account='00000000-0000-4000-8000-000000000081',epoch=3)
        self.value=dict(case=self.case,fileResponseBytes=D.original(),requests=[],pending=[],policy=[],files=[],
            sourceObserved=True,sourcePresent=True,sourceReady=False,sourceRaw=S.member_source(),native=S.member_raw(),
            driver=dict(started=0,finished=0,active=0),knownDriver=dict(started=1,finished=0,active=1))
        self.value['requests']=[dict(serial=1,lane='known',kind='discovery',method='api.getFlannelHttpUrl',
            args=dict(include_external_workspaces='true',_x_reason='client-needs-valid-flannel-url'),
            account=self.model['account'],epoch=3,team='T1',released=False)]
        self.value['pending']=copy.deepcopy(self.value['requests'])
        self.model['completionSource']=dict(titleUsers=self.value)
        self.row=dict(stage='draft-discovery',model=self.model,proof=dict(releases=[],
            afterInvalidation=dict(completionSource=dict(titleUsers=copy.deepcopy(self.value)))))
    def validate(self,baseline=None):
        S.member_ledger(self.value,self.model,self.case,baseline if baseline is not None else {})
        V.nested_counts(self.row,self.case)
    def test_known_invalidated_raw_is_not_absent_or_ready(self):self.validate()
    def test_known_read_cannot_claim_absent_lane(self):
        self.value['requests'][0]['lane']='absent'
        with self.assertRaises(RuntimeError):self.validate()
    def test_known_read_cannot_consume_absent_driver(self):
        self.value['driver'].update(started=1,active=1)
        with self.assertRaises(RuntimeError):self.validate()
    def test_original_known_source_cannot_disappear(self):
        self.value.update(sourcePresent=False,sourceRaw=None)
        with self.assertRaises(RuntimeError):self.validate()
    def test_invalidated_raw_does_not_grant_ready(self):
        self.value['sourceReady']=True
        with self.assertRaises(RuntimeError):self.validate()
    def test_original_preimage_cannot_be_replaced(self):
        self.row['proof']['afterInvalidation']['completionSource']['titleUsers']['native']['updated']=102
        with self.assertRaises(RuntimeError):self.validate()
    def test_known_lane_counter_arithmetic_required(self):
        self.value['knownDriver']['finished']=1
        with self.assertRaises(RuntimeError):self.validate()
    def test_known_lane_attempt_cannot_be_omitted(self):
        self.value['knownDriver'].update(started=0,finished=0,active=0)
        with self.assertRaises(RuntimeError):self.validate()
    def test_known_lane_attempt_cannot_be_replayed(self):
        self.value['knownDriver'].update(started=2,finished=1,active=1)
        with self.assertRaises(RuntimeError):self.validate()
    def test_known_driver_history_cannot_regress(self):
        with self.assertRaises(RuntimeError):self.validate(dict(knownDriver=dict(started=2,finished=1,active=1)))
    def test_absent_reader_cannot_be_relabelled_known(self):
        value=copy.deepcopy(self.value);value.update(case='user-title-recovery',sourcePresent=False,sourceRaw=None,sourceReady=True)
        with self.assertRaises(RuntimeError):S.member_ledger(value,self.model,'user-title-recovery',{})
    def test_late_callback_cannot_clear_invalidation(self):
        t=IndependentRetirementTests();t.setUp();t.rows[3]['model']['completionSource']['titleUsers']['sourceReady']=True
        with self.assertRaises(RuntimeError):t.validate()
    def test_late_callback_cannot_change_raw_preimage(self):
        t=IndependentRetirementTests();t.setUp();t.rows[3]['model']['completionSource']['titleUsers']['sourceRaw']['updated']=102
        with self.assertRaises(RuntimeError):t.validate()
    def test_late_callback_cannot_change_native_preimage(self):
        t=IndependentRetirementTests();t.setUp();t.rows[3]['model']['completionSource']['titleUsers']['native']['updated']=102
        with self.assertRaises(RuntimeError):t.validate()
    def test_absent_idle_counter_cannot_stand_in_for_known_completion(self):
        t=IndependentRetirementTests();t.setUp();source=t.rows[3]['model']['completionSource']['titleUsers']
        source['driver']=dict(active=0);source['knownDriver']['active']=1
        with self.assertRaises(RuntimeError):t.validate()
    def pending_known_member(self):
        first=self.value['requests'][0]
        first.update(released=True,response=dict(raw=dict(ok=True,url='https://edgeapi.slack.com/cache/T1',ttl_seconds=300),keys=None),
                     disposition=dict(serial=1,receiverRetired=False,delivered=True))
        second=dict(serial=2,lane='known',kind='member',method='users/info',
            args=dict(ids=[S.MEMBER],check_interaction=True,include_profile_only_users=True),
            account=self.model['account'],epoch=3,team='T1',released=False)
        self.value['requests'].append(second);self.value['pending']=[copy.deepcopy(second)]
        self.row['stage']='draft-member';self.row['proof']['releases']=[copy.deepcopy(first['disposition'])]
        return second
    def test_known_unconditional_request_uses_ids(self):
        self.pending_known_member();self.validate()
    def test_known_request_rejects_absent_updated_ids(self):
        q=self.pending_known_member();q['args'].pop('ids');q['args']['updated_ids']={S.MEMBER:0}
        self.value['pending']=[copy.deepcopy(q)]
        with self.assertRaises(RuntimeError):self.validate()
    def test_known_request_rejects_combined_wire_shapes(self):
        q=self.pending_known_member();q['args']['updated_ids']={S.MEMBER:0};self.value['pending']=[copy.deepcopy(q)]
        with self.assertRaises(RuntimeError):self.validate()
    def test_known_request_rejects_duplicate_ids(self):
        q=self.pending_known_member();q['args']['ids'].append(S.MEMBER);self.value['pending']=[copy.deepcopy(q)]
        with self.assertRaises(RuntimeError):self.validate()
    def test_known_request_rejects_other_identity(self):
        q=self.pending_known_member();q['args']['ids']=['UOTHER001'];self.value['pending']=[copy.deepcopy(q)]
        with self.assertRaises(RuntimeError):self.validate()
    def test_absent_reader_rejects_known_ids_wire(self):
        q=self.pending_known_member();value=copy.deepcopy(self.value)
        value.update(case='user-title-recovery',sourcePresent=False,sourceRaw=None,sourceReady=True,
                     driver=dict(started=1,finished=0,active=1),knownDriver=dict(started=0,finished=0,active=0))
        for r in value['requests']:r['lane']='absent'
        value['pending']=[copy.deepcopy(value['requests'][-1])]
        with self.assertRaises(RuntimeError):S.member_ledger(value,self.model,'user-title-recovery',{})


class PendingQueryFeedbackTests(unittest.TestCase):
    def setUp(self):
        _,receipt=D.sample();self.row=copy.deepcopy(receipt['stages'][1])
        self.row['stage']='retry-discovery'
        self.q=self.row['model']['completion'];self.g=self.row['geometry']['completion']
        self.q.update(revision=0,labels=[],values=[],settledBatch=False)
        self.g.update(revision=0,rows=[],status=dict(text='Searching mentions...',
            visible=True,hit=True,rect=copy.deepcopy(self.g['popup']['rect'])))
    def test_exact_loading_for_recovery_and_retirement(self):
        for stage in ('discovery','member-held','retry-discovery','retry-member','second-discovery','second-member'):
            self.row['stage']=stage;V.popup(self.row)
    def test_empty_status_cannot_hide_loading(self):
        self.g['status']['text']=''
        with self.assertRaises(RuntimeError):V.popup(self.row)
    def test_missing_status_refused(self):
        self.g['status']=None
        with self.assertRaises(RuntimeError):V.popup(self.row)
    def test_wrong_trigger_status_refused(self):
        self.g['status']['text']='Searching channels...'
        with self.assertRaises(RuntimeError):V.popup(self.row)
    def test_wrong_popup_revision_refused(self):
        self.g['revision']=1
        with self.assertRaises(RuntimeError):V.popup(self.row)
    def test_settled_batch_cannot_claim_loading(self):
        self.q['settledBatch']=True
        with self.assertRaises(RuntimeError):V.popup(self.row)
    def test_wrong_owner_refused(self):
        self.q['current']=False
        with self.assertRaises(RuntimeError):V.popup(self.row)
    def test_failure_feedback_precedes_loading(self):
        self.row['stage']='failure';self.g['status']['text']=V.F['FAILURE'];V.popup(self.row)
        self.g['status']['text']='Searching mentions...'
        with self.assertRaises(RuntimeError):V.popup(self.row)
    def test_settled_empty_feedback_precedes_loading(self):
        for stage in ('query-replaced','late-query'):
            self.row['stage']=stage;self.g['status']['text']='No matching mentions';V.popup(self.row)
            self.g['status']['text']='Searching mentions...'
            with self.assertRaises(RuntimeError):V.popup(self.row)
    def test_nonempty_interim_catalog_not_called_empty_loading(self):
        _,receipt=D.sample();row=receipt['stages'][1];row['stage']='member-held';V.popup(row)


class ClosedViewOwnershipTests(unittest.TestCase):
    def setUp(self):
        _,self.receipt=D.sample();self.first=self.receipt['stages'][0]
        self.closed=copy.deepcopy(self.first);self.closed['stage']='closed'
        self.closed['model'].update(document=None,records=None,unrelated=None,detail=None)
        self.closed['model']['comments']['active']=False
    def validate(self):V.owner(self.receipt,[self.first,self.closed])
    def test_exact_closed_view_retains_original_inventory_and_seed(self):
        for name in V.CLOSED:self.closed['stage']=name;self.validate()
    def test_earlier_mounted_view_cannot_disappear(self):
        self.closed['stage']='member-held'
        with self.assertRaises(RuntimeError):self.validate()
    def test_closed_records_cannot_be_current(self):
        self.closed['model']['records']=copy.deepcopy(self.first['model']['records'])
        with self.assertRaises(RuntimeError):self.validate()
    def test_closed_missing_field_is_not_observed_null(self):
        del self.closed['model']['records']
        with self.assertRaises(RuntimeError):self.validate()
    def test_closed_comments_cannot_remain_active(self):
        self.closed['model']['comments']['active']=True
        with self.assertRaises(RuntimeError):self.validate()
    def test_original_retained_inventory_cannot_change(self):
        self.closed['model']['initialRecords']={}
        with self.assertRaises(RuntimeError):self.validate()
    def test_original_rich_seed_cannot_change(self):
        self.closed['proof']['originalA']=[]
        with self.assertRaises(RuntimeError):self.validate()
    def test_closed_owner_cannot_change(self):
        self.closed['model']['epoch']+=1
        with self.assertRaises(RuntimeError):self.validate()


class CanonicalMemberProjectionTests(unittest.TestCase):
    def setUp(self):
        self.t=KnownRecoveryLaneTests();self.t.setUp()
    def test_wire_row_cannot_replace_canonical_source(self):
        self.t.value['sourceRaw']=S.member_raw()
        with self.assertRaises(RuntimeError):self.t.validate()
    def test_canonical_normalized_name_required(self):
        self.t.value['sourceRaw']['profile']['display_name_normalized']='wrong'
        with self.assertRaises(RuntimeError):self.t.validate()
    def test_canonical_extra_field_not_silently_accepted(self):
        self.t.value['sourceRaw']['fabricated']=True
        with self.assertRaises(RuntimeError):self.t.validate()
    def test_boot_source_cannot_claim_member_info_interaction(self):
        self.t.value['sourceRaw']['is_stranger']=False
        with self.assertRaises(RuntimeError):self.t.validate()
    def test_recovered_source_has_exact_interaction_projection(self):
        value=copy.deepcopy(self.t.value);value.update(case='user-title-recovery',sourceReady=True,
            sourceRaw=S.member_source(True),driver=dict(started=1,finished=0,active=1),
            knownDriver=dict(started=0,finished=0,active=0))
        value['requests'][0]['lane']='absent';value['pending']=copy.deepcopy(value['requests'])
        S.member_ledger(value,self.t.model,'user-title-recovery',{})
        value['sourceRaw'].pop('is_stranger')
        with self.assertRaises(RuntimeError):S.member_ledger(value,self.t.model,'user-title-recovery',{})
    def test_response_wire_does_not_include_derived_fields(self):
        t=OriginalSourceTests();t.setUp();t.discovery();t.release()
        row=copy.deepcopy(t.value['requests'][0]);row.update(serial=2,kind='member',method='users/info',
            args=dict(updated_ids={S.MEMBER:0},check_interaction=True,include_profile_only_users=True),
            response=dict(raw=S.member_response(),keys=None),disposition=dict(serial=2,receiverRetired=False,delivered=True))
        row['response']['raw']['results'][0]=S.member_source(True);t.value['requests'].append(row)
        with self.assertRaises(RuntimeError):t.validate()
    def test_actual_two_back_controls_required_for_draft_departure(self):
        plan,counts=V.action_plan('user-title-draft-recovery')
        self.assertEqual([x[2] for x in plan[5:7]],['slack-list-comments-back','slack-list-record-back'])
        self.assertEqual(counts[8:],[7,8,8,9,9,9])
    def test_actual_two_back_controls_required_for_query_departure(self):
        plan,counts=V.action_plan('user-title-retirement')
        self.assertEqual([x[2] for x in plan[3:5]],['slack-list-comments-back','slack-list-record-back'])
        self.assertEqual(counts[7:],[5,6,6])


if __name__=='__main__':unittest.main()
