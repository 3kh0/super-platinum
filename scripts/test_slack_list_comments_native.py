"""List comments protocol/custody acceptance tests; no native process."""
import copy
import json
from pathlib import Path
import runpy
import unittest
R=runpy.run_path(str(Path(__file__).with_name('check-slack-list-comments-native.py')))
DIR=Path('/private/list-comments')


def sample(narrow=False):
    fixture='slack-list'+('-narrow' if narrow else '')
    viewport=[668,380] if narrow else [1280,780]
    box=dict(left=10,top=10,right=210,bottom=35,width=200,height=25)
    doc=dict(id=R['LIST'],mode='list',opaque={'retained':True})
    records={R['KNOWN']:dict(id=R['KNOWN'],list_id=R['LIST'],thread_ts=R['ROOT_TS'],fields=[dict(key='estimate',number=[0])]),
             R['ALIAS']:dict(id=R['ALIAS'],list_id=R['LIST'],fields=[dict(key='estimate',number=[1])])}
    records.update({f'R_LIST_{i:03}':dict(id=f'R_LIST_{i:03}',opaque={'text':'\u4e2d\U0001f642','index':i}) for i in range(2,73)})
    original=copy.deepcopy(records);records,unrelated=R['record_proof'](records)
    requests=[]
    for operation in range(10):
        record=R['KNOWN'] if operation<7 else R['ALIAS']
        args=[dict(file=R['LIST']),dict(list_id=R['LIST'],id=record,include_is_subscribed='true',_x_reason='get-record-subtasks'),
              dict(ssb_platform='windows',ssb_release_channel='prod'),dict(channel='CCOMMENT01',ts=R['ROOT_TS'] if operation<7 else R['LIST']+'-'+R['ALIAS'],
                  inclusive='true',limit='28',cached_latest_updates='{}',_x_reason='history-api/fetchReplies',include_tombstones='true')]
        if operation==1:args[-1]['cursor']=R['CURSOR']
        for method,body in zip(('files.info','lists.records.info','experiments.getByUser','conversations.replies'),args):
            requests.append(dict(serial=len(requests)+1,method=method,args=body))
    first=dict(ok=True,has_more=True,messages=[dict(ts='1781524801.000001',thread_ts=R['ROOT_TS'],text='First page comment.')],response_metadata=dict(next_cursor=R['CURSOR']))
    second=dict(ok=True,has_more=False,messages=[dict(ts='1781524802.000001',thread_ts=R['ROOT_TS'],text='Second page retained.')])
    empty=dict(ok=True,messages=[],has_more=False)
    synthetic=dict(ok=True,messages=[dict(ts='1781524901.000001',thread_ts=R['SYNTHETIC_TS'],text='Synthetic root resolved.')],has_more=False)
    for page,root,record in [(first,R['ROOT_TS'],R['KNOWN']),(synthetic,R['SYNTHETIC_TS'],R['ALIAS'])]:
        page['messages'].insert(0,dict(ts=root,subtype='list_record_comment',slack_list=dict(list_id=R['LIST'],list_record_id=record),unknown=dict(keep=True),text='Record comment root'))
    rows=[]
    for i,stage in enumerate(R['STAGES']):
        active=stage not in ('detail','back','record-replaced','account-replaced')
        held=stage in ('loading','record-held','account-held')
        count=R['COUNTS'][i]
        ledger=dict(requests=copy.deepcopy(requests[:count]),replies=count//4,completed=count-int(held),held=held,cancelled=0 if i<12 else 1 if i<14 else 2)
        page=None if i in (0,1,13,14) else first if i==2 else second if i in (3,4,5,6) else empty if i in (7,8,9) else synthetic
        error=R['GENERIC'] if stage in ('malformed','wrong-root') else R['OFFLINE'] if stage=='offline' else R['COOLDOWN'] if stage in ('cooldown','back') else None
        entry=None if i in (0,14) else dict(active=active,record=R['KNOWN'] if i<10 else R['ALIAS'],root=None if i==13 else R['ROOT_TS'] if i<10 else R['SYNTHETIC_TS'],requestTs=R['ROOT_TS'],generation=1 if i<10 else 2 if i<13 else 3,
             loading=held or i==12,error=error,retryAt=999 if i==8 else 0,retryAfter=2 if i==8 else 0,page=copy.deepcopy(page),hasMore=i==2,
             visibleTs=[m['ts'] for m in (page or {}).get('messages',[]) if m.get('subtype')!='list_record_comment'])
        current=copy.deepcopy(records)
        if i>=12:current[R['ALIAS']]['fields'][0]['number']=[19]
        model=dict(account=None if i<14 else 'list-comments-successor',epoch=0 if i<14 else 1,noSession=True,noTransport=True,writes=0,networkCalls=0,
            ledger=ledger,initialDocument=copy.deepcopy(doc),initialRecords=copy.deepcopy(records),initialUnrelated=copy.deepcopy(unrelated),unrelated=copy.deepcopy(unrelated) if i<14 else None,document=copy.deepcopy(doc) if i<14 else None,
            records=current if i<14 else None,detail=dict(record=R['KNOWN'] if i<10 else R['ALIAS'],open=1 if i<10 else 2) if i<14 else None,comments=entry)
        ids=['slack-list-comments-back','slack-list-comments-refresh'] if active else ['slack-list-record-back','slack-list-record-comments'] if i<14 else []
        if active and entry['hasMore']:ids.append('slack-list-comments-next')
        controls=[dict(id=id,text=id,disabled=(held or stage=='cooldown') and id!='slack-list-comments-back',visible=True,hit=True,rect=copy.deepcopy(box))for id in ids]
        for c in controls:
            if c['id']=='slack-list-comments-back':c.update(display='flex',fits=True,iconCount=1)
        notices=[]
        if active and error:notices.append(dict(text=error,visible=True,rect=copy.deepcopy(box)))
        if held:notices.append(dict(text='Loading comments...',visible=True,rect=copy.deepcopy(box)))
        if stage=='empty':notices.append(dict(text='No comments yet',visible=True,rect=copy.deepcopy(box)))
        native_rows=[dict(ts=m['ts'],text=m['text'],visible=True,rect=copy.deepcopy(box))for m in (page or {}).get('messages',[]) if m.get('subtype')!='list_record_comment'] if active else []
        g=dict(ok=True,noOverflow=True,editable=0,comments=active,detail=True if i<14 else False,dialog=i<14,viewport=viewport,generation=str(entry['generation']) if active else None,
               controls=controls,notices=notices,rows=native_rows,active='slack-list-record-comments' if stage=='back' else '')
        if stage=='first':g['readContent']=dict(bold=True,secondParagraph=True,reactionCount='2',reactionGlyph='\U0001f44b',reactionTag='SPAN',reactionVisible=True,messageTools=0,reactionButtons=0)
        proof=None
        if stage=='first':proof=dict(release=dict(delivered=True,cancelledBeforeReply=False))
        if stage=='cooldown':proof=dict(before=copy.deepcopy(ledger),after=copy.deepcopy(ledger),native=dict(ok=True,disabled=True))
        if stage in ('record-replaced','account-replaced'):proof=dict(before=copy.deepcopy(rows[-1]['model']),release=dict(delivered=False,cancelledBeforeReply=True))
        rows.append(dict(stage=stage,model=model,geometry=g,proof=proof,capture=str(DIR/f'{fixture}-{stage}.png')))
    return fixture,dict(ok=True,fixture=fixture,initialRecords=original,noLiveNetwork=True,readiness=R['ready_tuple'](fixture,'0'*32,123),stages=rows,final=copy.deepcopy(rows[-1]['model']))


class CommentsTests(unittest.TestCase):
    def check(self,f,r):return R['validate'](f,r,DIR,platform='win32',png=False)
    def reject(self,change,stage='next'):
        f,r=sample();change(next(s for s in r['stages'] if s['stage']==stage))
        with self.assertRaises(RuntimeError):self.check(f,r)
    def test_checksum_utf8_sorted_nested_objects_preserves_array_order(self):
        values={'z':{'z':True,'a':'\u4e2d\U0001f642\n'},'a':[2,1]}
        _,summary=R['record_proof'](values)
        self.assertEqual(summary,dict(count=2,byteLength=42,fnv1a64='b80e91e571c0ce7d'))
        self.assertEqual(summary,R['record_proof']({'a':[2,1],'z':{'a':'\u4e2d\U0001f642\n','z':True}})[1])
        self.assertNotEqual(summary,R['record_proof']({'a':[1,2],'z':values['z']})[1])
    def test_unrelated_source_checksum_and_inventory_are_exact(self):
        for key,value in [('count',70),('byteLength',1),('fnv1a64','0'*16)]:
            self.reject(lambda s:s['model']['unrelated'].update({key:value}))
            self.reject(lambda s:s['model']['initialUnrelated'].update({key:value}))
        f,r=sample();r['initialRecords']['R_LIST_002']['opaque']['index']=999
        with self.assertRaises(RuntimeError):self.check(f,r)
        f,r=sample();del r['initialRecords']['R_LIST_072']
        with self.assertRaises(RuntimeError):self.check(f,r)
    def test_account_replacement_cannot_retain_unrelated_source(self):
        self.reject(lambda s:s['model'].update(unrelated=s['model']['initialUnrelated']),stage='account-replaced')
    def test_back_content_fits_native_flex(self):
        for key,value in [('display','grid'),('fits',False),('iconCount',0)]:
            self.reject(lambda s:s['geometry']['controls'][0].update({key:value}),stage='first')
    def test_rich_comment_is_bold_multiline_and_inert(self):
        for key,value in [('bold',False),('secondParagraph',False),('reactionCount','1'),('reactionGlyph','x'),('reactionTag','BUTTON'),('reactionVisible',False),('messageTools',1),('reactionButtons',1)]:
            self.reject(lambda s:s['geometry']['readContent'].update({key:value}),stage='first')
    def test_both_widths(self):
        for narrow in (False,True):self.check(*sample(narrow))
    def test_gate_fixture_identity(self):
        f,r=sample();r['fixture']='other'
        with self.assertRaises(RuntimeError):self.check(f,r)
    def test_request_endpoint_and_options(self):
        for key,value in [('limit','100'),('include_tombstones','false'),('channel','C_OTHER'),('ts','foreign'),('token','secret')]:
            self.reject(lambda s:s['model']['ledger']['requests'][3]['args'].update({key:value}))
    def test_acquisition_order(self):
        self.reject(lambda s:s['model']['ledger']['requests'][0].update(method='conversations.replies'))
    def test_cursor_exact(self):
        self.reject(lambda s:s['model']['ledger']['requests'][7]['args'].update(cursor='other'))
    def test_no_writes_or_transport(self):
        for key,value in [('writes',1),('networkCalls',1),('noSession',False),('noTransport',False)]:
            self.reject(lambda s:s['model'].update({key:value}))
    def test_wrong_root_never_published(self):
        self.reject(lambda s:s['model']['comments'].update(root='foreign'),stage='wrong-root')
    def test_failure_preserves_exact_page(self):
        for stage in ('malformed','wrong-root','offline'):
            self.reject(lambda s:s['model']['comments'].update(page=None),stage)
    def test_empty_page_required(self):
        self.reject(lambda s:s['model']['comments'].update(page=dict(ok=True,messages=False)),stage='empty')
    def test_cooldown_blocks_dispatch(self):
        self.reject(lambda s:s['proof']['after'].update(replies=99),stage='cooldown')
        self.reject(lambda s:s['model']['comments'].update(retryAfter=0),stage='cooldown')
    def test_back_restores_focus(self):
        self.reject(lambda s:s['geometry'].update(active='body'),stage='back')
    def test_synthetic_uses_alias_then_acquired_root(self):
        self.reject(lambda s:s['model']['ledger']['requests'][31]['args'].update(ts=R['SYNTHETIC_TS']),stage='synthetic')
        self.reject(lambda s:s['model']['comments'].update(root=None),stage='synthetic')
    def test_old_held_request_dropped(self):
        for stage in ('record-replaced','account-replaced'):
            self.reject(lambda s:s['proof']['release'].update(delivered=True),stage)
            self.reject(lambda s:s['model']['ledger'].update(cancelled=0),stage)
    def test_predecessor_custody_exact(self):
        self.reject(lambda s:s['proof'].update(before={}),stage='record-replaced')
    def test_only_record_number_changes(self):
        self.reject(lambda s:s['model']['records'][R['KNOWN']].update(opaque='changed'),stage='record-replaced')
        self.reject(lambda s:s['model']['records'][R['ALIAS']]['fields'][0].update(number=[20]),stage='record-replaced')
    def test_original_source_not_reseeded(self):
        self.reject(lambda s:s['model']['initialDocument'].update(title='replacement'))
    def test_account_replacement_clears_comments(self):
        self.reject(lambda s:s['model'].update(comments={}),stage='account-replaced')
        self.reject(lambda s:s['model'].update(epoch=0),stage='account-replaced')
    def test_native_rows_match_source(self):
        self.reject(lambda s:s['geometry']['rows'][0].update(ts='foreign'))
    def test_root_metadata_and_hidden_stub_projection(self):
        self.reject(lambda s:s['model']['comments']['page']['messages'][0]['slack_list'].update(list_record_id='foreign'),stage='synthetic')
        self.reject(lambda s:s['model']['comments']['visibleTs'].insert(0,R['ROOT_TS']),stage='first')
    def test_rendered_comment_text_exact(self):
        self.reject(lambda s:s['geometry']['rows'][0].update(text='Other comment'))
    def test_loading_feedback(self):
        self.reject(lambda s:s['geometry'].update(notices=[]),stage='loading')
    def test_failures_visible(self):
        self.reject(lambda s:s['geometry'].update(notices=[]),stage='offline')
    def test_controls_not_clipped_or_occluded(self):
        self.reject(lambda s:s['geometry']['controls'][0].update(hit=False))
        self.reject(lambda s:s['geometry']['controls'][0]['rect'].update(right=9999))
    def test_no_comment_editor(self):
        self.reject(lambda s:s['geometry'].update(editable=1))
    def test_read_busy_disables_actions(self):
        self.reject(lambda s:s['geometry']['controls'][1].update(disabled=False),stage='loading')
    def test_capture_path_and_stages(self):
        self.reject(lambda s:s.update(capture='/foreign.png'))
        f,r=sample();r['stages'].pop()
        with self.assertRaises(RuntimeError):self.check(f,r)
    def test_readiness_exact_types(self):
        for args in [('other','0'*32,123),('slack-list','bad',123),('slack-list','0'*32,True)]:
            with self.assertRaises(RuntimeError):R['ready_tuple'](*args)
    def test_native_failure_is_not_masked_by_missing_success_readiness(self):
        f,r=sample(); expected=r['readiness']
        class Log:
            def receipts(self,prefix):
                return [dict(ok=False,error='List DOM did not settle: inert detail controls')] if prefix==R['PREFIX'] else [expected] if prefix==R['WAITING'] else []
        with self.assertRaisesRegex(RuntimeError,'Native comments failed: List DOM did not settle: inert detail controls'):
            R['validate_log'](Log(),f,DIR,expected,None)
    def test_success_still_requires_exact_terminal_owner(self):
        f,r=sample();expected=copy.deepcopy(r['readiness']);r['readiness']['pid']+=1
        class Log:
            def receipts(self,prefix):
                return [r] if prefix==R['PREFIX'] else [expected] if prefix==R['WAITING'] else []
        with self.assertRaisesRegex(RuntimeError,'Wrong exact readiness owner'):
            R['validate_log'](Log(),f,DIR,expected,None)
    def test_duplicate_terminal_or_edges(self):
        f,r=sample()
        class Log:
            def receipts(self,prefix):
                return [r,r] if prefix==R['PREFIX'] else [r['readiness']]
        with self.assertRaises(RuntimeError):R['validate_log'](Log(),f,DIR,r['readiness'],None)

if __name__=='__main__':unittest.main()
