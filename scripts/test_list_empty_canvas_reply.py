"""Focused portable mutations; these synthetic receipts do not claim native success."""
import copy
import hashlib
import json
from pathlib import Path
import runpy
import unittest
from unittest.mock import patch
import slack_list_empty_canvas_reply as O
import list_empty_canvas_reply_test_data as D

ROOT=Path(__file__).resolve().parents[1]
RUN=runpy.run_path(str(ROOT/'scripts/check-list-empty-canvas-reply-native.py'))

class HarnessTests(unittest.TestCase):
    def test_baseline_bytes_unchanged(self):
        for p in json.loads((ROOT/'tmp/list-empty-canvas-reply-20261022/oracle/baseline-pins.json').read_text()):
            self.assertEqual(hashlib.sha256((ROOT/p['path']).read_bytes()).hexdigest(),p['sha256'])
    def test_original_owned_pipeline_reused(self):
        h=RUN['harness']()
        for name in ('run_fixture','validate_log'):self.assertIs(h[name].__globals__,h)
        self.assertEqual(h['PREFIX'],'slack-list-comments-compose-check: ')
        self.assertEqual(h['EDGE'],'slack-list-comments-compose-stage: ')
        for case in O.STAGES:self.assertEqual(h['case_contract'](case)[0],O.STAGES[case])
        base=runpy.run_path(str(ROOT/'scripts/check-slack-list-comments-compose-native.py'))
        self.assertEqual(h['case_contract']('compose')[0],base['STAGES'])
    def test_only_exact_gate_added(self):
        cls=RUN['harness']()['NativeFixture'];parent=cls.__bases__[0];seen=[]
        for case in O.STAGES:
            original={'SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE':case,'unrelated':'preserved'}
            with patch.object(parent,'__init__',lambda self,*a,**kw:seen.append(a[-1])):
                cls('slack-list',Path('log'),Path('binary'),original)
            self.assertEqual(seen[-1],dict(original,**{RUN['FLAG']:'1'}));self.assertNotIn(RUN['FLAG'],original)
    def test_foreign_and_preconfigured_gates_refused(self):
        cls=RUN['harness']()['NativeFixture']
        with patch.object(cls.__bases__[0],'__init__') as init:
            for v in ({},{'SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE':'compose'},
                      {'SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE':O.LIFECYCLE,RUN['FLAG']:'0'}):
                with self.assertRaises(RuntimeError):cls('slack-list',Path('log'),Path('binary'),v)
            init.assert_not_called()

class ReceiptTests(unittest.TestCase):
    def setUp(self):self.f,self.r=D.sample()
    def row(self,name='reopened'):return next(r for r in self.r['stages'] if r['stage']==name)
    def validate(self):O.validate(self.f,self.r,D.DIR,png=False)
    def recover(self):self.f,self.r=D.recovery()
    def reject(self,mutate):
        mutate()
        with self.assertRaises((RuntimeError,KeyError,ValueError,TypeError,IndexError)):self.validate()
    def test_full_both_cases_both_widths(self):
        for make in (D.sample,D.recovery):
            for narrow in (False,True):self.f,self.r=make(narrow);self.validate()
    def test_stage_omission(self):self.reject(lambda:self.r['stages'].pop(4))
    def test_original_owner(self):self.reject(lambda:self.row()['model'].__setitem__('epoch',99))
    def test_original_opaque_rich_wire(self):self.reject(lambda:self.row()['proof']['expectedBlocks'][0]['elements'][0]['elements'][1].pop('opaque'))
    def test_ordinary_link_wire(self):self.reject(lambda:self.row('draft')['proof']['expectedBlocks'][0]['elements'][0]['elements'][3].__setitem__('url','https://foreign.invalid'))
    def test_original_raw_search_bytes(self):self.reject(lambda:self.row('query-ready')['model']['completionSource']['responses'][-1].__setitem__('raw','{}'))
    def test_original_channel_map_order(self):self.reject(lambda:self.row('query-ready')['proof']['sourceFiles']['rows'][0]['effective'].__setitem__('channels',['CAMAP001','CZMAP001','CTAIL001']))
    def test_query_fallback_not_borrowed_by_list(self):self.reject(lambda:self.row('query-ready')['model']['completion']['canvasFallback'].__setitem__(O.S.LIST,D.fallback()))
    def test_missing_consumed_quip(self):self.reject(lambda:self.row('query-ready')['model']['completion']['canvasFallback'][O.S.CANVAS].__setitem__('quipTitle',None))
    def test_foreign_locale_descriptor(self):self.reject(lambda:self.row('query-ready')['model']['completion']['canvasFallback'][O.S.CANVAS]['descriptor'].__setitem__('origin','slack-catalog'))
    def test_stale_choice(self):self.reject(lambda:self.row('canvas-inserted')['proof']['inputs'][1]['publication'].__setitem__('current',False))
    def test_wrong_selected_wire(self):self.reject(lambda:self.row('canvas-inserted')['proof']['inputs'][1].__setitem__('selected',O.S.LIST))
    def test_draft_cannot_borrow_query_receipt(self):self.reject(lambda:self.row()['model']['titleUsers'].__setitem__('observed',copy.deepcopy(self.row('query-ready')['model']['completion'])))
    def test_draft_capture(self):self.reject(lambda:self.row()['model']['titleUsers'].__setitem__('capture','foreign'))
    def test_independent_title_raw_envelope(self):self.reject(lambda:self.row()['model']['titleUsers']['observed']['titles']['files'][0].__setitem__('receipt',{}))
    def test_title_projection_cannot_borrow_candidate_channels(self):self.reject(lambda:self.row()['model']['titleUsers']['observed']['titles']['files'][0]['effective'].__setitem__('channels',['CZMAP001']))
    def test_no_user_policy_read_grant(self):self.reject(lambda:self.row()['model']['titleUsers']['observed'].__setitem__('policy',{}))
    def test_no_nested_users_grant(self):self.reject(lambda:self.row()['model']['titleUsers']['observed']['titles'].__setitem__('users',{}))
    def test_reopen_native_identity(self):self.reject(lambda:self.row()['geometry']['editor'].__setitem__('sameNode',True))
    def test_held_send_readonly(self):self.reject(lambda:self.row('sending')['geometry']['editor'].__setitem__('editable',True))
    def test_no_extra_send(self):self.reject(lambda:self.row('readback')['model']['composeLedger']['posts'].append(copy.deepcopy(self.row('readback')['model']['composeLedger']['posts'][0])))
    def test_transcript_needs_own_receipt(self):self.reject(lambda:self.row('sent')['model']['titleAcquisition'].__setitem__('rows',[]))
    def test_transcript_occlusion(self):self.reject(lambda:self.row('sent')['proof']['renderedFiles']['before']['tokens'][0].__setitem__('hit',False))
    def test_png_path(self):self.reject(lambda:self.row().__setitem__('capture','/foreign.png'))
    def test_final_model(self):self.reject(lambda:self.r['final'].__setitem__('epoch',99))
    def test_failure_not_query_label_grant(self):
        self.recover();self.reject(lambda:self.row('draft-failed')['model']['titleUsers'].__setitem__('observed',D.observed()))
    def test_retry_must_be_visible(self):
        self.recover();self.reject(lambda:self.row('draft-failed')['geometry']['titleRetry'].__setitem__('hit',False))
    def test_pending_editor_must_remain_absent(self):
        self.recover();self.reject(lambda:self.row('retry-held')['geometry'].__setitem__('editorAbsent',False))
    def test_original_singleton_factory(self):
        self.recover();self.reject(lambda:self.row('retry-held')['model']['coldFiles']['requests'][1]['args'].__setitem__('count','501'))
    def test_original_singleton_raw(self):
        self.recover();self.reject(lambda:self.row()['model']['coldFiles']['responses'][2]['raw']['file'].__setitem__('title','Untitled'))
    def test_original_byte_order(self):
        self.recover();self.reject(lambda:self.row()['model']['coldFiles']['requests'][2].__setitem__('responseText',json.dumps(json.loads(O.S.singleton(O.S.CANVAS,True)),sort_keys=True)))
    def test_plain_does_not_consume_quip(self):
        self.recover();self.reject(lambda:self.row()['model']['titleUsers']['observed']['titles']['canvasFallback'][O.S.CANVAS].__setitem__('quipTitle',D.fallback()['quipTitle']))
    def test_invalidation_keeps_mounted_node(self):
        self.recover();self.reject(lambda:self.row('canvas-inserted')['proof']['mountedCustody']['after'].__setitem__('sameNode',False))
    def test_invalidation_keeps_caret(self):
        self.recover();self.reject(lambda:self.row('canvas-inserted')['proof']['mountedCustody']['after'].__setitem__('sameSelection',False))
    def test_original_comments_back_required(self):
        self.recover();self.reject(lambda:self.row('draft-replaced')['proof']['inputs'].pop(5))
    def test_closed_view_is_null(self):
        self.recover();self.reject(lambda:self.row('draft-closed')['model'].__setitem__('document',copy.deepcopy(self.row('draft')['model']['document'])))
    def test_late_callback_refused(self):
        self.recover();self.reject(lambda:self.row('draft-late')['model']['coldFiles']['releases'][0].__setitem__('delivered',True))
    def test_late_cannot_publish_successor(self):
        self.recover();self.reject(lambda:self.row('draft-late')['model']['titleUsers'].__setitem__('current',True))
    def test_late_cannot_replace_file_source(self):
        self.recover();self.reject(lambda:self.row('draft-late')['model']['coldFiles']['source'].__setitem__('unavailableIds',[]))
    def test_late_cannot_edit_original_journal(self):
        self.recover();self.reject(lambda:self.row('draft-late')['proof']['journal'].__setitem__('revision',99))
    def test_fresh_return_cannot_borrow_old_binding(self):
        self.recover();self.reject(lambda:self.row('return-held')['model']['titleUsers'].__setitem__('scope',self.row('retry-held')['model']['titleUsers']['scope']))
    def test_no_extra_read_after_return(self):
        self.recover();self.reject(lambda:self.row()['model']['coldFiles']['requests'].append(copy.deepcopy(self.row()['model']['coldFiles']['requests'][2])))

if __name__=='__main__':unittest.main()
