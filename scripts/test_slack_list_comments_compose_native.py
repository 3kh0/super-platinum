"""Adversarial compose receipt acceptance tests; no native process or Slack I/O."""
import copy
import json
from pathlib import Path
import runpy
import unittest
R=runpy.run_path(str(Path(__file__).with_name('check-slack-list-comments-compose-native.py')))
BASE=runpy.run_path(str(Path(__file__).with_name('test_slack_list_comments_native.py')))
DIR=Path('/private/list-comments-compose')


def sample(narrow=False):
    f,base=BASE['sample'](narrow)
    initial=base['stages'][0]['model']
    # Exercise actual field-key lookup: the changed Estimate is not the first field.
    related=[r for r in base['initialRecords'].values() if r['id']==R['READ']['ALIAS']]
    related += [initial[k][R['READ']['ALIAS']] for k in ('records','initialRecords')]
    for record in related:
        if not any(field['key']=='NAME' for field in record['fields']):
            record['fields'].insert(0,dict(key='NAME',text='Unchanged item name'))
    box=dict(left=10,top=10,right=210,bottom=35,width=200,height=25)
    def blocks(text):return [dict(type='rich_text',elements=[dict(type='rich_text_section',elements=[dict(type='text',text=text,style=dict(bold=True))])])]
    first=blocks('Final review 中🙂\nSecond line <literal> & *punctuation*.')
    tokens=[dict(type='user',user_id='U_FIXTURE_OPAQUE',opaque=dict(retain=[1,2])),dict(type='link',url='https://example.test/reference',text='Reference',opaque=dict(source='fixture'))]
    first[0]['elements'][0]['elements'].extend(copy.deepcopy(tokens))
    recovery=blocks('Recoverable reply 日🦊\nKeep this exact draft.')
    outgoing=[]
    for i,b in enumerate((first,recovery,recovery,recovery)):
        outgoing.append(dict(method='chat.postMessage',args=dict(channel='CCOMMENT01',ts=f'178152500{i}.000001',type='message',reply_broadcast='false',thread_ts=R['READ']['ROOT_TS'],include_channel_perm_error='true',client_msg_id=f'00000000-0000-4000-8000-{i:012}',blocks=json.dumps(b,ensure_ascii=False),_x_reason='webapp_message_send')))
    counts=[(0,0)]*7+[(1,1),(1,1),(2,1),(3,2)]+[(4,3)]*8
    preflight=dict(method='conversations.genericInfo',args={'updated_channels':'{"CCOMMENT01":0}'})
    policy=dict(request=dict(method='client.userBoot',args={'include_min_version_bump_check':'1'}),boot=dict(ok=True),channel=dict(id='CCOMMENT01'),threadReadOnly=False)
    stages=[]
    for i,name in enumerate(R['STAGES']):
        model=copy.deepcopy(initial)
        model['ledger']['requests']=copy.deepcopy(base['stages'][1]['model']['ledger']['requests'][:4])
        p,n=counts[i];posts=[outgoing[k] for k in (0,2,3)][:n]
        messages=[]
        for k in (0,2):
            if k<len(posts):
                a=posts[k]['args'];messages.append(dict(channel=a['channel'],thread_ts=a['thread_ts'],client_msg_id=a['client_msg_id'],blocks=json.loads(a['blocks']),user='U1',ts=f'178152590{k}.000001',text='First accepted comment' if k==0 else 'Recovered accepted comment'))
        ledger=dict(prepared=copy.deepcopy(outgoing[:p]),posts=copy.deepcopy(posts),preflights=[copy.deepcopy(preflight)]*(p+int(i>=14)+int(i>=15)+int(i>=17)),policies=[copy.deepcopy(policy)]*p,messages=messages,published=i>=13,held=name in ('record-held','account-held'),heldPost=name=='sending')
        model['composeLedger']=ledger
        model['comments']=copy.deepcopy(base['stages'][2]['model']['comments'])
        displayed=copy.deepcopy(base['stages'][2]['geometry']['rows'])
        if name in ('sent','not-sent','rejected','unconfirmed'):
            model['comments']['visibleTs'].append(messages[0]['ts'])
            displayed.append(copy.deepcopy(messages[0]))
        elif name=='confirmed':
            model['comments']['visibleTs'].extend(message['ts'] for message in messages)
            model['comments']['page']['messages'].extend(copy.deepcopy(messages))
            displayed.extend(copy.deepcopy(messages))
        if name in ('record-replaced','account-held'):
            next(f for f in model['records'][R['READ']['ALIAS']]['fields'] if f['key']=='estimate')['number']=[19]
        if name=='closed':model.update(unrelated=None,document=None,records=None,comments=dict(active=False,loading=False))
        if name=='account-replaced':model.update(unrelated=None,document=None,comments=None,account='successor',epoch=1)
        absent=name in ('back','closed','record-replaced','account-replaced','sent','confirmed')
        pending=name in ('sending','unconfirmed','locked-reopen','record-held','account-held')
        content=first if i<9 else recovery
        e=None if absent else dict(visible=True,rect=copy.deepcopy(box),blocks=copy.deepcopy(content),text='Native\nmultiline',bold=True,editable=not pending)
        controls=[] if absent else [dict(id='slack-list-comment-send',visible=True,hit=True,rect=copy.deepcopy(box),disabled=pending)]
        g=dict(ok=True,noOverflow=True,viewport=[668,380] if narrow else [1280,780],editor=e,controls=controls,dialog=name not in ('closed','account-replaced'),active='slack-list-record-comments' if name=='back' else '',notices=['Unconfirmed. Refresh comments.'] if name in ('unconfirmed','locked-reopen') else ['Recoverable error'] if name in ('not-sent','rejected','denied') else [],rows=displayed)
        if name=='sent':
            transcript=dict(box,height=100,bottom=110)
            g['reveal']=dict(serial='1',ts=messages[0]['ts'],scrollTop=40,scrollRect=copy.deepcopy(transcript))
            g['rows'][-1].update(rect=copy.deepcopy(transcript),visibleHeight=100)
            g['controls'].append(dict(id='slack-list-comments-next',visible=True,hit=True,rect=copy.deepcopy(box),disabled=False))
        proof=dict(blocks=copy.deepcopy(first)) if i==0 else dict(release=dict(delivered=True,cancelledBeforeReply=False)) if name in ('sent','unconfirmed') else dict(release=dict(delivered=False,cancelledBeforeReply=True)) if name in ('record-replaced','account-replaced') else None
        if i==0:proof['seed']=dict(reopened=True,revision=1,blocks=copy.deepcopy(first))
        if name in ('close-reopen','locked-reopen'):proof=dict(runtimeForgotten=True)
        draft=dict(id='draft-'+str(0 if i<4 else 1 if i<12 else 2),scope=dict(record='R_LIST_000',root=R['READ']['ROOT_TS']),blocks=copy.deepcopy(content),capture='capture1' if i<9 else 'capture2',savedRevision=1,captureFailed=False,terminalPending=False,attempt=None)
        if name in ('sent','confirmed'):draft['blocks']=[]
        if name in ('sending','unconfirmed','locked-reopen'):
            a=posts[0 if name=='sending' else -1]['args'];draft['attempt']=dict(record=dict(client_msg_id=a['client_msg_id'],blocks=json.loads(a['blocks'])))
        model['draftStore']=dict(drafts=[draft])
        stages.append(dict(stage=name,model=model,geometry=g,proof=proof,capture=str(DIR/f'{f}-{name}.png')))
    readback=dict(model=copy.deepcopy(stages[8]['model']),geometry=copy.deepcopy(stages[8]['geometry']))
    readback['model']['composeLedger']['published']=True
    readback['model']['comments']['page']['messages'].append(copy.deepcopy(readback['model']['composeLedger']['messages'][0]))
    reads=readback['model']['ledger']['requests']
    for request in copy.deepcopy(reads):
        request['serial']=len(reads)+1;reads.append(request)
    next_reads=copy.deepcopy(reads)
    for request in copy.deepcopy(reads[-4:]):
        request['serial']=len(next_reads)+1;next_reads.append(request)
    for row in stages[9:]:row['model']['ledger']['requests']=copy.deepcopy(next_reads)
    return f,dict(ok=True,fixture=f,noLiveNetwork=True,initialRecords=base['initialRecords'],readiness=base['readiness'],stages=stages,
                  acceptedReadback=readback,final=copy.deepcopy(stages[-1]['model']))


class ComposeTests(unittest.TestCase):
    def test_case_routing_preserves_default_and_refuses_unknown(self):
        stages,validator=R['case_contract']('compose')
        self.assertEqual(stages,R['STAGES'])
        self.assertIs(validator,R['validate'])
        with self.assertRaisesRegex(RuntimeError,'Unknown compose acceptance case'):
            R['case_contract']('unrecognized')
    def check(self,f,r):return R['validate'](f,r,DIR,png=False)
    def reject(self,change,stage='sending'):
        f,r=sample();change(next(s for s in r['stages'] if s['stage']==stage))
        with self.assertRaises((RuntimeError,ValueError,KeyError)):self.check(f,r)
    def test_both_widths(self):
        for narrow in (False,True):self.check(*sample(narrow))
    def test_lost_rich_draft(self):
        for stage in ('back-reopen','close-reopen','escape-reopen','scrim-reopen','sending','rejected','locked-reopen'):
            self.reject(lambda s:s['geometry']['editor'].update(blocks=[]),stage)
    def test_original_opaque_and_journal_custody(self):
        self.reject(lambda s:s['proof']['seed'].update(reopened=False),'draft')
        self.reject(lambda s:s['model'].update(draftStore=dict(drafts=[])))
        self.reject(lambda s:s['model']['draftStore']['drafts'][0]['attempt']['record'].update(client_msg_id='foreign'))
        for stage in ('close-reopen','locked-reopen'):
            self.reject(lambda s:s['proof'].update(runtimeForgotten=False),stage)
            self.reject(lambda s:s['model']['draftStore']['drafts'][0].update(capture='foreign'),stage)
        self.reject(lambda s:s['model']['draftStore']['drafts'][0]['scope'].update(record='R_OTHER'),'account-replaced')
    def test_failure_guidance_and_editability(self):
        for stage in ('not-sent','rejected','denied'):
            self.reject(lambda s:s['geometry'].update(notices=[]),stage)
            self.reject(lambda s:s['geometry']['editor'].update(editable=False),stage)
    def test_uncertainty_cannot_replay_or_edit(self):
        for stage in ('unconfirmed','locked-reopen','sending','record-held','account-held'):
            if stage not in ('unconfirmed','locked-reopen'):self.reject(lambda s:s['geometry']['editor'].update(editable=True),stage)
            self.reject(lambda s:s['geometry']['controls'][0].update(disabled=False),stage)
    def test_exact_factory_fields(self):
        for key,value in [('text','unexpected'),('thread_ts','foreign'),('reply_broadcast','true'),('client_msg_id','invalid'),('ts','1')]:
            self.reject(lambda s:s['model']['composeLedger']['prepared'][0]['args'].update({key:value}))
    def test_duplicate_or_known_unsent_dispatch(self):
        self.reject(lambda s:s['model']['composeLedger']['posts'].append(s['model']['composeLedger']['prepared'][1]),'not-sent')
        self.reject(lambda s:s['model']['composeLedger']['prepared'].append(s['model']['composeLedger']['prepared'][0]))
    def test_no_live_state_or_extra_write(self):
        for k,v in [('noSession',False),('noTransport',False),('networkCalls',1),('writes',1)]:
            self.reject(lambda s:s['model'].update({k:v}))
    def test_original_source_preservation(self):
        self.reject(lambda s:s['model']['records'][R['READ']['KNOWN']].update(changed=True))
        self.reject(lambda s:s['model'].update(unrelated={}))
        self.reject(lambda s:s['model'].update(document={}))
    def test_preflight_authority_required(self):
        self.reject(lambda s:s['model']['composeLedger'].update(policies=[]))
        self.reject(lambda s:s['model']['composeLedger']['preflights'][0]['args'].update(token='secret'))
    def test_positive_confirmation_exact(self):
        self.reject(lambda s:s['model']['composeLedger']['messages'][0].update(client_msg_id='other'))
        self.reject(lambda s:s['geometry'].update(rows=[]),'confirmed')
        self.reject(lambda s:s['model']['composeLedger'].update(published=False),'confirmed')
    def test_sent_requires_immediate_model_and_native_projection(self):
        self.reject(lambda s:s['model']['comments'].update(visibleTs=[]),'sent')
        self.reject(lambda s:s['geometry'].update(rows=[]),'sent')
        f,r=sample();sent=r['stages'][8]
        sent['model']['comments']['visibleTs']=copy.deepcopy(r['stages'][7]['model']['comments']['visibleTs'])
        sent['geometry']['rows']=copy.deepcopy(r['stages'][7]['geometry']['rows'])
        with self.assertRaisesRegex(RuntimeError,'immediately projected'):self.check(f,r)
    def test_sent_cannot_wait_for_an_extra_refresh(self):
        f,r=sample();sent=r['stages'][8]
        request=copy.deepcopy(sent['model']['ledger']['requests'][-1])
        request['serial']=len(sent['model']['ledger']['requests'])+1
        sent['model']['ledger']['requests'].append(request)
        with self.assertRaisesRegex(RuntimeError,'extra read'):self.check(f,r)
    def test_sent_overlay_does_not_replace_original_page_or_cursor(self):
        self.reject(lambda s:s['model']['comments']['page']['response_metadata'].update(next_cursor='invented'),'sent')
        self.reject(lambda s:s['model']['comments'].update(hasMore=False),'sent')
    def test_positive_rows_cannot_duplicate_consistently(self):
        for name in ('sent','confirmed'):
            f,r=sample();row=next(s for s in r['stages'] if s['stage']==name)
            message=row['model']['composeLedger']['messages'][0]
            row['model']['comments']['visibleTs'].append(message['ts'])
            row['geometry']['rows'].append(copy.deepcopy(next(m for m in row['geometry']['rows'] if m['ts']==message['ts'])))
            with self.assertRaisesRegex(RuntimeError,'uniqueness'):self.check(f,r)
    def test_held_send_is_not_optimistically_projected(self):
        f,r=sample();row=r['stages'][7];message=copy.deepcopy(row['model']['composeLedger']['messages'][0])
        row['model']['comments']['visibleTs'].append(message['ts']);row['geometry']['rows'].append(message)
        with self.assertRaisesRegex(RuntimeError,'before positive acceptance'):self.check(f,r)
    def test_failed_or_uncertain_send_cannot_add_any_visible_row(self):
        for name in ('not-sent','rejected','unconfirmed','locked-reopen'):
            f,r=sample();row=next(s for s in r['stages'] if s['stage']==name)
            row['model']['comments']['visibleTs'].append('1781525999.000001')
            row['geometry']['rows'].append(dict(ts='1781525999.000001',text='Failed draft optimistic projection'))
            with self.assertRaisesRegex(RuntimeError,'projected'):self.check(f,r)
    def test_failure_keeps_prior_accepted_content_until_reopen(self):
        for name in ('not-sent','rejected','unconfirmed'):
            f,r=sample();row=next(s for s in r['stages'] if s['stage']==name)
            row['model']['comments']['visibleTs'].pop();row['geometry']['rows'].pop()
            with self.assertRaisesRegex(RuntimeError,'removed an accepted'):self.check(f,r)
            self.reject(lambda s:s['geometry']['rows'][-1].update(text='Recoverable reply shown instead'),name)
    def test_confirmed_requires_exact_readback_once(self):
        self.reject(lambda s:s['model']['comments']['page'].update(messages=[]),'confirmed')
        self.reject(lambda s:s['model']['comments']['page']['messages'].append(copy.deepcopy(s['model']['comments']['page']['messages'][-1])),'confirmed')
        self.reject(lambda s:s['model']['comments']['page']['messages'][-1].update(blocks=[]),'confirmed')
        self.reject(lambda s:s['model']['comments']['page']['messages'][-1].update(client_msg_id='other'),'confirmed')
    def test_sent_reveal_is_original_and_measurably_visible(self):
        for key,value in (('serial',''),('serial','0'),('ts','other'),('scrollTop',float('nan'))):
            self.reject(lambda s:s['geometry']['reveal'].update({key:value}),'sent')
        self.reject(lambda s:s['geometry']['rows'][-1].update(visibleHeight=0),'sent')
        self.reject(lambda s:s['geometry']['rows'][-1].update(visibleHeight=999),'sent')
        self.reject(lambda s:s['geometry']['rows'][-1]['rect'].update(bottom=float('nan')),'sent')
    def test_sent_transcript_rejects_zero_tiny_and_subminimum_height(self):
        for height in (0,2.7,4,79.9):
            f,r=sample(True);g=r['stages'][8]['geometry']
            g['reveal']['scrollRect'].update(height=height,bottom=10+height)
            g['rows'][-1]['visibleHeight']=height
            with self.assertRaisesRegex(RuntimeError,'reveal geometry'):self.check(f,r)
    def test_sent_body_rejects_partial_clip_despite_tall_transcript(self):
        for height in (0,2.7,25,78.9):
            f,r=sample(True);g=r['stages'][8]['geometry']
            g['rows'][-1]['rect'].update(top=110-height,bottom=210-height)
            g['rows'][-1]['visibleHeight']=height
            with self.assertRaisesRegex(RuntimeError,'readably revealed'):self.check(f,r)
    def test_sent_body_allows_readable_partial_or_short_full_row(self):
        for row_height,visible_height in ((100,80),(100,79),(35,35),(35,34)):
            f,r=sample(True);g=r['stages'][8]['geometry']
            g['reveal']['scrollRect'].update(height=80,bottom=90)
            g['rows'][-1]['rect'].update(top=90-visible_height,bottom=90-visible_height+row_height,height=row_height)
            g['rows'][-1]['visibleHeight']=visible_height
            self.check(f,r)
    def test_sent_readable_transcript_keeps_next_page_reachable(self):
        self.reject(lambda s:s['geometry'].update(controls=[]),'sent')
        self.reject(lambda s:s['geometry']['controls'][0].update(visible=False),'sent')
        self.reject(lambda s:s['geometry']['controls'][0].update(hit=False),'sent')
    def test_same_panel_readback_requires_original_owner_and_single_refresh(self):
        for key,value in (('account','foreign'),('document',{}),('records',{})):
            f,r=sample();r['acceptedReadback']['model'][key]=value
            with self.assertRaisesRegex(RuntimeError,'source/owner'):self.check(f,r)
        f,r=sample();r['acceptedReadback']['model']['ledger']['requests'].pop()
        with self.assertRaisesRegex(RuntimeError,'explicit refresh'):self.check(f,r)
        f,r=sample();r['acceptedReadback']['model']['comments']['generation']+=1
        with self.assertRaisesRegex(RuntimeError,'original comments visit'):self.check(f,r)
    def test_same_panel_readback_dedup_and_original_page_identity(self):
        for location in ('visible','dom','page','uuid'):
            f,r=sample();proof=r['acceptedReadback'];model=proof['model'];message=model['composeLedger']['messages'][0]
            if location=='visible':model['comments']['visibleTs'].append(message['ts'])
            elif location=='dom':proof['geometry']['rows'].append(copy.deepcopy(proof['geometry']['rows'][-1]))
            elif location=='page':model['comments']['page']['messages'].append(copy.deepcopy(message))
            else:model['comments']['page']['messages'][-1]['client_msg_id']='foreign'
            with self.assertRaisesRegex(RuntimeError,'readback|Same-panel'):self.check(f,r)
    def test_same_panel_readback_keeps_cursor_and_empty_original_draft(self):
        for location in ('cursor','has_more','blocks','draft','post'):
            f,r=sample();model=r['acceptedReadback']['model']
            if location=='cursor':model['comments']['page']['response_metadata']['next_cursor']='foreign'
            elif location=='has_more':model['comments']['page']['has_more']=False
            elif location=='blocks':model['comments']['page']['messages'][-1]['blocks']=[]
            elif location=='draft':model['draftStore']['drafts'][0]['blocks']=r['stages'][0]['proof']['blocks']
            else:model['composeLedger']['posts'].append(copy.deepcopy(model['composeLedger']['posts'][0]))
            with self.assertRaises(RuntimeError):self.check(f,r)
    def test_next_send_requires_distinct_exact_scoped_preflight(self):
        for change in ('missing','extra','foreign','order','prefix'):
            f,r=sample();reads=r['stages'][9]['model']['ledger']['requests']
            if change=='missing':del reads[-4:]
            elif change=='extra':reads.extend(copy.deepcopy(reads[-4:]))
            elif change=='foreign':reads[-3]['args']['id']='R_LIST_001'
            elif change=='order':reads[-4],reads[-3]=reads[-3],reads[-4]
            else:reads[0]['args']['file']='FOTHER'
            with self.assertRaises(RuntimeError):self.check(f,r)
    def test_old_normal_readable_geometry_needs_new_next_control_evidence(self):
        path=Path(__file__).resolve().parent.parent/'tmp/list-comments-immediate-display-20261018/compose-a/slack-list/slack-list.json'
        if not path.is_file():self.skipTest('Retained compose-a evidence is not installed')
        outer=json.loads(path.read_text(encoding='utf-8'));receipt=outer['terminalReceipts'][0]
        sent=receipt['stages'][8]
        accepted=sent['model']['composeLedger']['messages'][0]['ts']
        row=next(row for row in sent['geometry']['rows'] if row['ts']==accepted)
        self.assertGreaterEqual(sent['geometry']['reveal']['scrollRect']['height'],80)
        self.assertGreaterEqual(row['visibleHeight'],min(row['rect']['height'],80)-1)
        with self.assertRaisesRegex(RuntimeError,'Next page control'):
            R['validate'](receipt['fixture'],receipt,Path(outer['readyDirectory']),dimensions=outer['expectedPngDimensions'],png=False)
    def test_old_narrow_2px_transcript_is_not_readable_acceptance(self):
        path=Path(__file__).resolve().parent.parent/'tmp/list-comments-immediate-display-20261018/compose-b/slack-list-narrow/slack-list-narrow.json'
        if not path.is_file():self.skipTest('Retained compact transcript evidence is not installed')
        outer=json.loads(path.read_text(encoding='utf-8'));receipt=outer['native']
        self.assertLess(receipt['stages'][8]['geometry']['reveal']['scrollRect']['height'],3)
        with self.assertRaisesRegex(RuntimeError,'reveal geometry'):
            R['validate'](receipt['fixture'],receipt,Path(outer['readyDirectory']),dimensions=outer['expectedPngDimensions'],png=False)
    def test_retired_original_owner_cannot_receive(self):
        for stage in ('record-replaced','account-replaced'):
            self.reject(lambda s:s['proof']['release'].update(delivered=True),stage)
    def test_clipping_and_capture_custody(self):
        self.reject(lambda s:s['geometry']['controls'][0].update(hit=False))
        self.reject(lambda s:s['geometry']['editor']['rect'].update(right=9999))
        self.reject(lambda s:s.update(capture='/other.png'))
    def test_early_failure_keeps_real_reason(self):
        f,r=sample()
        class Log:
            def receipts(self,prefix):return [r['readiness']] if prefix==R['WAITING'] else [dict(ok=False,error='native editor refused capture')]
        with self.assertRaisesRegex(RuntimeError,'native editor refused capture'):
            R['validate_log'](Log(),f,DIR,r['readiness'],None)


if __name__=='__main__':unittest.main()
