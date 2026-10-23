"""Focused portable adversarial tests; original process harness is never launched."""
import copy
import hashlib
import json
from pathlib import Path
import runpy
import unittest
from unittest.mock import patch
import list_own_edit_empty_canvas_test_data as D
import slack_list_own_edit_empty_canvas as V

ROOT=Path(__file__).resolve().parents[1]
RUN=runpy.run_path(str(ROOT/'scripts/check-list-own-edit-empty-canvas-native.py'))


class HarnessTests(unittest.TestCase):
    def test_preserved_baseline_bytes(self):
        for item in json.loads((ROOT/'tmp/list-own-edit-empty-canvas-20261022/oracle/baseline-pins.json').read_text()):
            self.assertEqual(hashlib.sha256((ROOT/item['path']).read_bytes()).hexdigest(),item['sha256'],item['path'])

    def test_owned_pipeline_isolated(self):
        a,b=RUN['harness'](),RUN['harness']()
        self.assertIsNot(a,b)
        for name in ('run_fixture','validate_log'):self.assertIs(a[name].__globals__,a)
        self.assertEqual(a['PREFIX'],'slack-list-comments-compose-check: ')
        self.assertEqual(a['EDGE'],'slack-list-comments-compose-stage: ')
        self.assertEqual(a['case_contract']('own-edit')[0],b['case_contract']('own-edit')[0])

    def test_exact_mode_only_added(self):
        cls=RUN['harness']()['NativeFixture'];parent=cls.__bases__[0]
        for case in RUN['CASES']:
            checks={'SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE':case,'preserved':'yes'};seen=[]
            with patch.object(parent,'__init__',lambda self,*a,**kw:seen.append((a,kw))):
                cls('slack-list',Path('log'),Path('binary'),checks)
            self.assertNotIn(RUN['FLAG'],checks)
            self.assertEqual(seen[0][0][-1],dict(checks,**{RUN['FLAG']:'1'}))

    def test_foreign_and_conflicting_mode_never_launches(self):
        cls=RUN['harness']()['NativeFixture'];parent=cls.__bases__[0]
        with patch.object(parent,'__init__') as init:
            for checks in ({},{'SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE':'own-edit'},
                {'SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE':V.LIFECYCLE,RUN['FLAG']:'0'}):
                with self.assertRaises(RuntimeError):cls('slack-list',Path('log'),Path('binary'),checks)
            init.assert_not_called()


class FullCorpusTests(unittest.TestCase):
    def test_both_cases_both_widths(self):
        for case in V.STAGES:
            for narrow in (False,True):
                with self.subTest(case=case,narrow=narrow):
                    f,r=D.sample(case,narrow);V.validator(case)(f,r,D.DIR,png=False)

    def test_currentness_is_retained_before_real_input(self):
        f,r=D.sample();self.assertFalse(r['stages'][1]['model']['edit']['active']['titleUsers']['current'])
        V.validator(r['case'])(f,r,D.DIR,png=False)

    def test_duplicate_occurrences_share_one_actual_request(self):
        f,r=D.sample();row=r['stages'][1];job=row['proof']['titleReady'][0]['job']
        self.assertEqual(len(job['tokens']),2);self.assertNotEqual(*job['tokens'])
        self.assertEqual(len(job['observed']['titles']['files']),2)
        self.assertEqual(len(row['model']['coldFiles']['requests']),1)

    def test_disabled_legacy_static_bytes_are_not_received_metadata(self):
        f,r=D.sample();self.assertEqual(r['stages'][0]['model']['titleSource']['files'],[])
        self.assertEqual(r['stages'][0]['model']['titleSource']['fileResponseBytes'],'unused static legacy fixture')
        V.validator(r['case'])(f,r,D.DIR,png=False)


class MutationTests(unittest.TestCase):pass


def negative(name,index,change,case=V.LIFECYCLE):
    def test(self):
        f,r=D.sample(case);change(r['stages'][index]);r['final']=copy.deepcopy(r['stages'][-1]['model'])
        with self.assertRaises(RuntimeError):V.validator(case)(f,r,D.DIR,png=False)
    setattr(MutationTests,'test_'+name,test)


def title(row):return row['proof']['titleReady'][0]['job']['observed']['titles']

negative('raw_singleton_changed',0,lambda r:r['model']['coldFiles']['responses'][0]['raw']['file'].update(title='not empty'))
negative('raw_response_text_reordered',0,lambda r:r['model']['coldFiles']['responses'][0].update(responseText='{}'))
negative('original_request_owner',0,lambda r:r['model']['coldFiles'].update(account='foreign'))
negative('request_factory_page',0,lambda r:r['model']['coldFiles']['requests'][0]['args'].update(page='2'))
negative('double_fetch',1,lambda r:r['model']['coldFiles']['requests'].append(copy.deepcopy(r['model']['coldFiles']['requests'][0])))
negative('candidate_channel_order',0,lambda r:r['model']['coldFiles']['source']['rows'][0]['effective'].update(channels=['CAMAP001','CZMAP001','CTAIL001']))
negative('title_projection_borrowed_sharing',1,lambda r:title(r)['files'][0]['effective'].update(channels=['CZMAP001']))
negative('title_missing_duplicate_occurrence',1,lambda r:title(r)['files'].pop())
negative('title_multi_file_envelope_borrowed',1,lambda r:title(r)['files'][0].update(receipt={'ok':True,'files':[]}))
negative('plain_cannot_fabricate_quip',1,lambda r:title(r)['canvasFallback'][V.S.CANVAS].update(quipTitle={'fileId':V.S.CANVAS,'current':True,'value':'nullish'}))
negative('blocks_requires_observed_quip',1,lambda r:title(r)['canvasFallback'][V.S.CANVAS].update(quipTitle=None),V.RECOVERY)
negative('stale_quip',1,lambda r:title(r)['canvasFallback'][V.S.CANVAS]['quipTitle'].update(current=False),V.RECOVERY)
negative('foreign_locale_descriptor',1,lambda r:title(r)['canvasFallback'][V.S.CANVAS]['descriptor'].update(locale='fr'))
negative('fallback_not_current',1,lambda r:title(r)['canvasFallback'][V.S.CANVAS].update(current=False))
negative('no_user_requires_no_policy',1,lambda r:r['proof']['titleReady'][0]['job']['observed'].update(policy={}))
negative('member_io_refused',0,lambda r:r['model']['titleSource']['requests'].append({'method':'users/info'}))
negative('unowned_member_driver_refused',0,lambda r:r['model']['titleSource'].update(driver={'started':1,'finished':1,'active':0}))
negative('label_cannot_change_raw_token',1,lambda r:r['proof']['titleReady'][0]['native']['tokens'][0]['value'].update(text='Untitled'))
negative('duplicate_token_style',1,lambda r:r['proof']['titleReady'][0]['native']['tokens'][1]['value'].update(style={'bold':True}))
negative('preinput_receipt_stale',1,lambda r:r['proof']['titleReady'][0]['job'].update(current=False))
negative('borrowed_edit_scope',1,lambda r:r['model']['edit']['active']['titleUsers'].update(scope='list-edit:foreign'))
negative('source_context_suffix',3,lambda r:r['model']['fixture']['updates'][0]['args'].update(blocks='[]'))
negative('independent_reply_preserved',1,lambda r:r['journal']['blocks'].append({'type':'text','text':'lost'}))
negative('durable_original_seed',1,lambda r:r['editInventory']['seed']['target'].update(channel='foreign'))
negative('save_double_dispatch',3,lambda r:r['model']['fixture']['updates'].append(copy.deepcopy(r['model']['fixture']['updates'][0])))
negative('held_failed_not_native_ready',3,lambda r:r['model']['edit']['active'].update(nativeReady=True),V.RECOVERY)
negative('retry_disabled',4,lambda r:r['proof']['titleActions'][-1]['native']['proof'].update(disabled=True),V.RECOVERY)
negative('held_old_owner_still_current',9,lambda r:r['proof']['visitReplacement'].update(oldCurrent=True),V.RECOVERY)
negative('held_native_not_detached',2,lambda r:r['geometry'].update(originalRunning=True),V.RECOVERY)
negative('cancel_must_clear_durable',5,lambda r:r['editInventory'].update(blocks=r['proof']['editedBlocks']),V.RECOVERY)
negative('late_cannot_publish_source',6,lambda r:r['model']['coldFiles'].update(source=D.candidate(json.loads(V.S.E.singleton(V.S.CANVAS)))),V.RECOVERY)
negative('late_wrong_disposition',6,lambda r:r['model']['coldFiles']['releases'][0].update(delivered=True),V.RECOVERY)
negative('fresh_requires_new_serial',7,lambda r:r['proof']['freshFile'].update(serial=3),V.RECOVERY)
negative('fresh_cannot_restore_cancelled_modifications',8,lambda r:r['model']['edit']['active'].update(blocks=r['proof']['editedBlocks']),V.RECOVERY)
negative('late_cannot_change_reply_native',6,lambda r:r['geometry']['reply'].update(sameNode=False),V.RECOVERY)
negative('returned_transcript_cannot_borrow_query',3,lambda r:r['proof']['returnTranscript']['rows'][0]['titleUsers']['titles']['files'][0].update(receipt={'ok':True,'files':[]}),V.RECOVERY)
negative('actual_invalidation_history_cannot_be_recaptured',4,lambda r:r['proof']['fileInvalidation'].update(eventRevisionBefore=8,eventRevisionAfter=9),V.RECOVERY)


class TranscriptPropagationTests(unittest.TestCase):
    def check_mutation(self,mutate,match):
        f,r=D.sample(V.RECOVERY);mutate(r['stages']);r['final']=copy.deepcopy(r['stages'][-1]['model'])
        with self.assertRaisesRegex(RuntimeError,match):V.validator(V.RECOVERY)(f,r,D.DIR,png=False)

    def test_historical_receipt_without_new_proof_is_not_current_acceptance(self):
        self.check_mutation(lambda rows:rows[8]['proof'].pop('transcriptRecovery'),'proof|generation')

    def test_new_serial_is_required_even_when_label_is_identical(self):
        def mutate(rows):
            old=rows[7]['model']['transcriptTitles']['rows'][0]['serial']
            for i in (8,9):
                rows[i]['model']['transcriptTitles']['rows'][0]['serial']=old
                rows[i]['proof']['transcriptRecovery']['after']['rows'][0]['serial']=old
            rows[9]['proof']['transcriptRecoveryFinal']['after']['rows'][0]['serial']=old
        self.check_mutation(mutate,'stale job')

    def test_unavailable_failure_is_not_eligible_for_automatic_requeue(self):
        def mutate(rows):
            rows[7]['model']['transcriptTitles']['rows'][0]['phase']='unavailable'
            for i in (8,9):rows[i]['proof']['transcriptRecovery']['before']['rows'][0]['phase']='unavailable'
        self.check_mutation(mutate,'Unavailable failure')

    def test_no_new_file_request_hidden_in_recovery_proof(self):
        self.check_mutation(lambda rows:rows[8]['proof']['transcriptRecovery']['readLedgerAfter'].append(
            copy.deepcopy(rows[8]['proof']['transcriptRecovery']['readLedgerAfter'][-1])),'proof|files.info')

    def test_retired_transcript_cannot_keep_label_authority(self):
        def mutate(rows):
            old=rows[7]['model']['transcriptTitles']['rows'][0]['titleUsers']['titles']
            old['labels']=copy.deepcopy(rows[8]['model']['transcriptTitles']['rows'][0]['titleUsers']['titles']['labels'])
            for i in (8,9):rows[i]['proof']['transcriptRecovery']['before']=copy.deepcopy(rows[7]['model']['transcriptTitles'])
        self.check_mutation(mutate,'Stale retained title')

    def test_transcript_must_keep_its_single_occurrence_not_edit_pair(self):
        def mutate(rows):
            for i in (8,9):
                obs=copy.deepcopy(rows[8]['proof']['titleReady'][-1]['job']['observed'])
                rows[i]['model']['transcriptTitles']['rows'][0]['titleUsers']=obs
                rows[i]['proof']['transcriptRecovery']['after']=copy.deepcopy(rows[i]['model']['transcriptTitles'])
            rows[9]['proof']['transcriptRecoveryFinal']['after']=copy.deepcopy(rows[9]['model']['transcriptTitles'])
        self.check_mutation(mutate,'occurrences/dependencies')

    def test_recovered_native_duplicate_label_is_not_url_fallback(self):
        self.check_mutation(lambda rows:rows[8]['proof']['transcriptRecovery']['native']['tokens'][1].update(text='https://fixture.invalid/files/FLISTFILE1'),
            'proof|not exposed')

    def test_recovery_cannot_replace_edit_node(self):
        self.check_mutation(lambda rows:rows[8]['proof']['transcriptRecovery']['editorAfter']['edit'].update(sameNode=False),
            'proof|native nodes')

    def test_recovery_cannot_replace_reply_node(self):
        self.check_mutation(lambda rows:rows[8]['proof']['transcriptRecovery']['editorAfter']['reply'].update(sameNode=False),
            'proof|reply')

    def test_recovery_mount_cannot_borrow_initial_edit(self):
        self.check_mutation(lambda rows:rows[8]['proof']['transcriptRecovery']['editorMount'].update(scope=rows[1]['model']['edit']['active']['scope']),
            'proof|editor mount')

    def test_cancel_must_keep_recovered_serial(self):
        def mutate(rows):
            rows[9]['model']['transcriptTitles']['rows'][0]['serial']+=1
            rows[9]['proof']['transcriptRecoveryFinal']['after']=copy.deepcopy(rows[9]['model']['transcriptTitles'])
        self.check_mutation(mutate,'Cancel replaced')

    def test_recovery_receipt_forbidden_in_lifecycle_case(self):
        f,r=D.sample();r['stages'][-1]['proof']['transcriptRecovery']={}
        with self.assertRaisesRegex(RuntimeError,'escaped'):V.validator(V.LIFECYCLE)(f,r,D.DIR,png=False)


if __name__=='__main__':unittest.main()
