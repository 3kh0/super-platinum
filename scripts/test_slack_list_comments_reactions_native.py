"""Reaction receipt adversaries; pure fixtures, no native process or Slack IO."""
import copy
import json
from pathlib import Path
import runpy
import unittest
from unittest.mock import patch

V=runpy.run_path(str(Path(__file__).with_name('check-slack-list-comments-compose-native.py')))
BASE=runpy.run_path(str(Path(__file__).with_name('test_slack_list_comments_native.py')))
DIR=Path('/private/list-comments-reactions')


def sample(narrow=False):
    fixture,base=BASE['sample'](narrow)
    initial=copy.deepcopy(base['stages'][2]['model'])
    initial['account']='8624eb74-23f4-4bee-9872-e37f392485c0'
    initial['ledger'].update(requests=initial['ledger']['requests'][:4],completed=4,replies=1)
    scope=dict(workspace='T1',user='U0',list=V['READ']['LIST'],record=V['READ']['KNOWN'],
               created=None,creator=None,root=V['READ']['ROOT_TS'])
    seed_blocks=[dict(type='rich_text',elements=[dict(type='rich_text_section',elements=[
        dict(type='text',text='Existing context '),dict(type='user',user_id='U_FIXTURE_OPAQUE',opaque=dict(retain=[1,2])),
        dict(type='text',text=' '),dict(type='link',url='https://example.test/reference',text='Reference',opaque=dict(source='fixture'))])])]
    blocks=copy.deepcopy(seed_blocks)
    blocks[0]['elements'][0]['elements'].append(dict(type='text',text=V['REACTION_TEXT'].split('\n')[0]+'\n',style=dict(bold=True)))
    blocks[0]['elements'].append(dict(type='rich_text_section',elements=[dict(type='text',text=V['REACTION_TEXT'].split('\n')[1])]))
    text='Existing context @U_FIXTURE_OPAQUE Reference'+V['REACTION_TEXT'].replace('\n','\n\n')
    seed=dict(scope=scope,blocks=seed_blocks,revision=1,reopened=True)
    post=dict(method='chat.postMessage',args=dict(channel='CCOMMENT01',ts='1781525000.000001',type='message',reply_broadcast='false',
        thread_ts=scope['root'],include_channel_perm_error='true',client_msg_id='00000000-0000-4000-8000-000000000001',
        blocks=json.dumps(blocks),_x_reason='webapp_message_send'))
    message=dict(type='message',channel='CCOMMENT01',thread_ts=scope['root'],user='U0',blocks=blocks,
                 client_msg_id=post['args']['client_msg_id'],ts='1781525001.000001')
    policy=dict(request=dict(method='client.userBoot',args={'include_min_version_bump_check':'1'}),
        boot=dict(ok=True,team=dict(id='T1'),self=dict(id='U0')),channel=dict(id='CCOMMENT01'),threadReadOnly=False)
    preflight=dict(method='conversations.genericInfo',args={'updated_channels':'{"CCOMMENT01":0}'})
    box=dict(left=10,top=10,right=210,bottom=35,width=200,height=25)
    rows=[];first_ts='1781524801.000001'
    for i,name in enumerate(V['REACTION_STAGES']):
        sent=i>=5;model=copy.deepcopy(initial);gen=20 if not sent else 21
        if sent:
            for request in copy.deepcopy(initial['ledger']['requests']):
                request['serial']=len(model['ledger']['requests'])+1;model['ledger']['requests'].append(request)
            model['ledger'].update(completed=8,replies=2)
            model['comments']['visibleTs'].append(message['ts'])
        target=message['ts'] if sent else first_ts;count=(2,3,3,2,2,0,1,0)[i]
        reactions=[]
        for ts in model['comments']['visibleTs']:
            n=count if ts==target else 2
            reactions.append(dict(ts=ts,reactions=[dict(name='wave',count=n)] if n else []))
        model['comments']['reactions']=copy.deepcopy(reactions)
        content=[] if sent else copy.deepcopy(blocks)
        capture='11111111-2222-4333-8444-'+('666666666666' if sent else '555555555555')
        journal=dict(scope=copy.deepcopy(scope),revision=4 if sent else 2,capture=capture,blocks=copy.deepcopy(content),attempt=None)
        draft=dict(id='original',scope=copy.deepcopy(scope),generation=gen,blocks=copy.deepcopy(content),capture=capture,
            sequence=2 if sent else 1,savedRevision=journal['revision'],attempt=None,busy=False,saving=False,loading=False,
            captureFailed=False,terminalPending=False,error=None,native=True)
        model['draftStore']=dict(active=f"original:{model['comments']['generation']}:{gen}",drafts=[draft])
        model['composeLedger']=dict(prepared=[copy.deepcopy(post)] if sent else [],posts=[copy.deepcopy(post)] if sent else [],
            messages=[copy.deepcopy(message)] if sent else [],policies=[copy.deepcopy(policy)] if sent else [],
            preflights=[copy.deepcopy(preflight)] if sent else [],published=False,held=False,heldPost=False)
        controls=[dict(id=k,disabled=sent if k=='slack-list-comment-send' else False,visible=True,hit=True,rect=copy.deepcopy(box))
            for k in ('slack-list-comments-back','slack-list-comments-next','slack-list-comments-refresh','slack-list-comment-send')]
        geometry=dict(ok=True,noOverflow=True,dialog=True,comments=True,compose=True,viewport=[668,380] if narrow else [1280,780],
            active='slack-list-comment-draft',controls=controls,rows=copy.deepcopy(reactions),
            editor=dict(visible=True,editable=True,blocks=copy.deepcopy(content),text='' if sent else text,bold=not sent,
                        generation=str(gen),sameNode=True,rect=copy.deepcopy(box)))
        for row in geometry['rows']:row['text']='First page comment.' if row['ts']==first_ts else text
        probe=dict(ts=target,count=count,channel='CCOMMENT01',editorGeneration=str(gen),hasMore=True,
                   readRequests=8 if sent else 4,prepared=int(sent),posts=int(sent))
        rows.append(dict(stage=name,model=model,geometry=geometry,reactionProbe=probe,
            proof=dict(seed=copy.deepcopy(seed),blocks=copy.deepcopy(blocks),journal=journal),capture=str(DIR/f'{fixture}-{name}.png')))
    events=[]
    for stage,team,generation,channel,ts,kind,changed in (
        ('added','T1',7,'CCOMMENT01',first_ts,'reaction_added',True),
        ('duplicate','T1',7,'CCOMMENT01',first_ts,'reaction_added',False),
        ('removed','T1',7,'CCOMMENT01',first_ts,'reaction_removed',True),
        ('foreign','T1',7,'COTHER',first_ts,'reaction_added',False),
        ('foreign','T_OTHER',7,'CCOMMENT01',first_ts,'reaction_added',False),
        ('foreign','T1',6,'CCOMMENT01',first_ts,'reaction_added',False),
        ('foreign','T1',7,'CCOMMENT01','1999999999.000001','reaction_added',False),
        ('tail-added','T1',7,'CCOMMENT01',message['ts'],'reaction_added',True),
        ('tail-removed','T1',7,'CCOMMENT01',message['ts'],'reaction_removed',True)):
        events.append(dict(stage=stage,team=team,generation=generation,changed=changed,
            frame=dict(type=kind,user='U_RT_REACTOR',reaction='wave',item=dict(type='message',channel=channel,ts=ts))))
    return fixture,dict(ok=True,case='reactions',fixture=fixture,noLiveNetwork=True,readiness=base['readiness'],
        initialRecords=base['initialRecords'],stages=rows,reactionEvents=events,final=copy.deepcopy(rows[-1]['model']))


class ReactionsTests(unittest.TestCase):
    def check(self,f,r,**kwargs):return V['validate_reactions'](f,r,DIR,png=False,**kwargs)
    def reject(self,change,index=1):
        f,r=sample();change(r['stages'][index])
        with self.assertRaises((RuntimeError,KeyError,ValueError,TypeError)):self.check(f,r)
    def test_both_widths(self):
        for narrow in (False,True):self.check(*sample(narrow))
    def test_case_dispatch(self):
        self.assertEqual(V['case_contract']('reactions'),(V['REACTION_STAGES'],V['validate_reactions']))
    def test_independent_unicode_literal(self):
        self.assertEqual([ord(c) for c in V['REACTION_TEXT'] if ord(c)>127],[0x4e2d,0x1f642])
    def test_stages_case_and_ready(self):
        for mutate in (lambda r:r.update(case='compose'),lambda r:r['stages'].pop(),lambda r:r['readiness'].update(pid=False)):
            f,r=sample();mutate(r)
            with self.assertRaises(RuntimeError):self.check(f,r)
    def test_exact_events_and_rejections(self):
        for i in range(9):
            f,r=sample();r['reactionEvents'][i]['changed']=not r['reactionEvents'][i]['changed']
            with self.assertRaises(RuntimeError):self.check(f,r)
        for key,value in [('generation',8),('team','T_OTHER')]:
            f,r=sample();r['reactionEvents'][0][key]=value
            with self.assertRaises(RuntimeError):self.check(f,r)
    def test_frame_identity(self):
        for key,value in [('reaction','eyes'),('user','OTHER'),('type','reaction_removed')]:
            f,r=sample();r['reactionEvents'][0]['frame'][key]=value
            with self.assertRaises(RuntimeError):self.check(f,r)
    def test_model_and_dom_count_independent(self):
        self.reject(lambda s:s['geometry']['rows'][0]['reactions'][0].update(count=99))
        self.reject(lambda s:s['model']['comments']['reactions'][0]['reactions'][0].update(count=99))
        self.reject(lambda s:s['reactionProbe'].update(count=99))
    def test_counts_duplicate_tail_and_foreign(self):
        for i in range(8):self.reject(lambda s:s['reactionProbe'].update(count=-1),i)
    def test_missing_duplicate_extra_pills(self):
        self.reject(lambda s:s['geometry']['rows'].clear())
        self.reject(lambda s:s['geometry']['rows'][0]['reactions'].append(dict(name='wave',count=1)))
        self.reject(lambda s:s['geometry']['rows'][0]['reactions'].append(dict(name='eyes',count=1)))
    def test_transcript_text_is_present_but_includes_changing_counts(self):
        self.reject(lambda s:s['geometry']['rows'][0].update(text=''))
        self.reject(lambda s:s['geometry']['rows'][-1].update(text=None),6)
        f,r=sample();r['stages'][1]['geometry']['rows'][0]['text']+=' wave3';self.check(f,r)
    def test_scalar_booleans_are_not_counts_or_event_changes(self):
        self.reject(lambda s:s['reactionProbe'].update(count=True),6)
        f,r=sample();r['reactionEvents'][0]['changed']=1
        with self.assertRaises(RuntimeError):self.check(f,r)
    def test_no_live_owner_mutation(self):
        for key,value in [('noSession',False),('noTransport',False),('writes',1),('networkCalls',1),('account','foreign'),('epoch',2)]:
            self.reject(lambda s:s['model'].update({key:value}))
    def test_no_requests_from_events(self):
        for i in (1,2,3,4,6,7):self.reject(lambda s:s['model']['ledger']['requests'].append(copy.deepcopy(s['model']['ledger']['requests'][0])),i)
        self.reject(lambda s:s['model']['composeLedger']['posts'].append({}),4)
    def test_original_page_and_cursor(self):
        self.reject(lambda s:s['model']['comments']['page'].update(has_more=False))
        self.reject(lambda s:s['model']['comments'].update(generation=999))
        self.reject(lambda s:s['model'].update(records={}))
    def test_native_node_focus_generation(self):
        self.reject(lambda s:s['geometry']['editor'].update(sameNode=False))
        self.reject(lambda s:s['geometry'].update(active='slack-list-comments-next'))
        self.reject(lambda s:s['geometry']['editor'].update(generation='999'))
        f,r=sample()
        for row in r['stages'][5:]:row['geometry']['editor']['sameNode']=False
        self.check(f,r)
    def test_exact_rich_journal_model_and_native(self):
        self.reject(lambda s:s['proof']['journal'].update(blocks=[]))
        self.reject(lambda s:s['geometry']['editor'].update(blocks=[]))
        self.reject(lambda s:s['model']['draftStore']['drafts'][0].update(blocks=[]))
        self.reject(lambda s:s['proof']['journal'].update(revision=999))
    def test_proof_not_self_attested(self):
        f,r=sample()
        for row in r['stages']:row['proof']['blocks'][0]['elements'][1]['elements'][0]['text']='Wrong text'
        with self.assertRaises(RuntimeError):self.check(f,r)
    def test_pending_and_errors_refused(self):
        for key in ('busy','saving','loading','captureFailed','terminalPending'):
            self.reject(lambda s:s['model']['draftStore']['drafts'][0].update({key:True}))
    def test_send_exactly_once_and_empty(self):
        self.reject(lambda s:s['model']['composeLedger']['posts'][0]['args'].update(blocks='[]'),5)
        self.reject(lambda s:s['model']['composeLedger'].update(published=True),6)
        self.reject(lambda s:s['geometry']['editor'].update(text='not cleared'),7)
        self.reject(lambda s:s['model']['composeLedger']['messages'][0].update(user='foreign'),5)
    def test_empty_native_paragraph_scaffold_is_not_draft_content(self):
        f,r=sample()
        for row in r['stages'][5:]:row['geometry']['editor']['text']='\n'
        self.check(f,r)
        for text in (' ', '\n\n', '\t', '\u200b', 'lost draft'):
            self.reject(lambda s:s['geometry']['editor'].update(text=text),5)
        self.reject(lambda s:s['geometry']['editor'].update(text='\n'),1)
        self.reject(lambda s:s['geometry']['editor'].update(blocks=s['proof']['blocks'],text='\n'),5)
    def test_preserved_native_reactions_a_receipt(self):
        path=Path(__file__).resolve().parents[1]/'tmp/list-comments-reactions-20261018/reactions-a/slack-list/slack-list.json'
        if not path.is_file():self.skipTest('Preserved offline native receipt is local evidence')
        outer=json.loads(path.read_text(encoding='utf-8'));receipt=outer['terminalReceipts'][0]
        self.assertFalse(outer['ok']);self.assertIn('Native text/style changed',outer['error'])
        self.assertEqual([s['geometry']['editor']['text'] for s in receipt['stages'][5:]],['\n']*3)
        V['validate_reactions']('slack-list',receipt,Path(outer['readyDirectory']),png=False)
    def test_geometry_controls_and_capture(self):
        self.reject(lambda s:s['geometry']['controls'][0].update(hit=False))
        self.reject(lambda s:s['geometry']['controls'][-1].update(disabled=True))
        self.reject(lambda s:s['geometry'].update(viewport=[float('nan'),380]))
        self.reject(lambda s:s.update(capture='/foreign.png'))
    def test_pngs_and_terminal(self):
        f,r=sample();seen=[]
        with patch.dict(V['H'],{'png_signature':lambda p,d:seen.append((p,d))}):
            V['validate_reactions'](f,r,DIR,png=True,dimensions=[1280,780])
        self.assertEqual(seen,[(DIR/f'{f}-{s}.png',[1280,780]) for s in V['REACTION_STAGES']])
        r['final']={}
        with self.assertRaises(RuntimeError):self.check(f,r)


if __name__=='__main__':unittest.main()
