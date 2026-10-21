"""Portable/adversarial closed List channel completion receipts; no app or Slack IO."""
import copy
import json
import runpy
import sys
import unittest
from pathlib import Path
V = runpy.run_path(str(Path(__file__).with_name('slack_list_comments_channel_completion.py')))
BASE = runpy.run_path(str(Path(__file__).with_name('test_slack_list_comments_reaction_selection.py')))
DIR = Path('/private/channel-completion')
LOCAL, PRIVATE, REMOTE = 'CLISTLOCAL', 'GLISTPRIVATE', 'CLISTREMOTE'
LABELS = {LOCAL:'#project-local', PRIVATE:'#project-private', REMOTE:'#project-cross-team'}
BOX = dict(left=10, top=10, right=210, bottom=35, width=200, height=25)


def snap(blocks, generation=20):
    return dict(blocks=copy.deepcopy(blocks), generation=str(generation), revision=10, composing=False)


def tail(original, values):
    v=copy.deepcopy(original);v[0]['elements'][0]['elements'].extend(values);return v


def query(original, term):return tail(original,[dict(type='text',text=' '+term)])


def inserted(original, ids):
    values=[]
    for id in ids:values.extend([dict(type='text',text=' '),dict(type='channel',channel_id=id),dict(type='text',text=' ')])
    return tail(original,values)


def source_base(model):
    user=dict(id='ULIST0001',name='fixture-self',team_id='T1',is_admin=True,profile={})
    seed=dict(accepted=True,boot=dict(ok=True,team=dict(id='T1',name='List completion'),self=user,users=[user],
        channels=[dict(id=LOCAL,name='project-local',team_id='T1',is_channel=True,is_member=True,updated=100,previous_names=[])],prefs={}),
        init=dict(ok=True,self=user,team=dict(id='T1',name='List completion'),workspaces=[dict(id='T1'),dict(id='TOTHER001')],
        onboarding=dict(user_data=dict(onboarding_complete=True)),user_date_created=123))
    return dict(enabled=True,named=True,protectedFixture=True,identityCurrent=True,httpCapability=False,networkCalls=0,
        account=model['account'],epoch=model['epoch'],team='T1',user='ULIST0001',requests=[],responses=[],releases=[],seed=seed,completed=0,cancelled=0,held=0)


def source_rows():return [
    dict(id=PRIVATE,name='project-private',team_id='T1',is_channel=True,is_private=True,is_member=True,updated=101,previous_names=[],fixture_opaque={'keep':'private'}),
    dict(id=REMOTE,name='project-cross-team',team_id='TOTHER001',is_channel=True,is_member=False,updated=101,previous_names=['project-previous'],fixture_opaque={'keep':'cross-team'})]


def sample(case='channel-completion',narrow=False):
    fixture,base=BASE['sample'](narrow=narrow);first=copy.deepcopy(base['stages'][0]);seed=first['proof']['seed'];a=first['proof']['originalA'];seed['scope']['user']='ULIST0001';first['model']['draftStore']['drafts'][0]['scope']['user']='ULIST0001'
    tokens=[dict(type='channel',channel_id=LOCAL,style={'bold':True},opaque={'occurrence':1}),dict(type='channel',channel_id=LOCAL,style={'italic':True},opaque={'occurrence':2})]
    for b in (seed['blocks'],a):b[0]['elements'][0]['elements'][4:4]=copy.deepcopy(tokens)
    assert a[0]['elements'][0]['elements'][-1]==dict(type='text',text='\n')
    a[0]['elements'][0]['elements'].pop()
    success=case=='channel-completion';scope=seed['scope'];canonical=first['model']['comments']['page'];canonical['messages'][1].pop('selection_fixture_opaque',None);canonical['messages'][1]['reactions']=[dict(name='wave',count=2,users=['U1','U2'])]
    final=inserted(a,[PRIVATE,REMOTE]);source=source_base(first['model']);rows=[]
    post=dict(method='chat.postMessage',args=dict(channel='CCOMMENT01',thread_ts=scope['root'],ts='1781525000.000001',type='message',reply_broadcast='false',include_channel_perm_error='true',client_msg_id='00000000-0000-4000-8000-000000000001',blocks=json.dumps(final),_x_reason='webapp_message_send'))
    msg=dict(type='message',channel='CCOMMENT01',thread_ts=scope['root'],ts='1781525001.000001',client_msg_id=post['args']['client_msg_id'],blocks=final,user='ULIST0001')
    input_plan=[('local-held','query','#project'),('keyboard-inserted','choose',PRIVATE),('mouse-inserted','query','#project-previous'),('mouse-inserted','choose',REMOTE)] if success else [('held','query','#project'),('query-retired','query','#project-new'),('malformed','query','#malformed'),('failure','query','#offline'),('empty','query','#empty'),('cancelled-reopened','query','#cancelled')]
    inputs=[]
    for j,(stage,kind,value) in enumerate(input_plan):
        before=query(inserted(a,[PRIVATE]) if success and j>=2 else a, '#project-previous' if success and j>=2 else '#project' if kind=='choose' else value)
        v=dict(ok=True,query=value,native=snap(before),sequence=0,revision=0) if kind=='query' else dict(ok=True,label=LABELS[value],keyboard=j==1,index=1 if j==1 else 0,sequence=10 if j==1 else 20,revision=2,before=snap(before))
        item=dict(stage=stage,kind=kind,value=v)
        if kind=='choose':item['channel']=value
        inputs.append(item)
    all_queries=['project','project-previous'] if success else ['project','project-new','malformed','offline','empty','cancelled']
    for i,stage in enumerate(V['STAGES'][case]):
        row=copy.deepcopy(first);row.update(stage=stage,capture=str(DIR/f'{fixture}-{stage}.png'));m,g=row['model'],row['geometry'];c=m['comments']
        for k in ('selectionSource','selectionProbe','actionProbe'):m.pop(k,None)
        m.update(noSession=False,noTransport=False)
        reopen=i>= (5 if success else 7);sent=success and i>=7;sending=success and i==6
        gen=40 if sent else 30 if reopen else 20;commentsgen=2 if reopen else 1
        blocks=([] if sent else final if success and i>=4 else inserted(a,[PRIVATE]) if success and i==3 else query(a,'#project') if success and i in (1,2) else a) if success else (a if i==0 else query(a,['','#project','#project-new','#project-new','#malformed','#offline','#empty','#cancelled'][i]))
        count=([0,1,1,1,2,2,2,2,2] if success else [0,1,2,2,3,4,5,6])[i]
        s=copy.deepcopy(source)
        if count:
            s['requests']=[dict(serial=1,method='experiments.getByUser',args=dict(ssb_platform={'win32':'windows','darwin':'darwin'}.get(sys.platform,sys.platform),ssb_release_channel='prod'))]
            s['responses']=[dict(serial=1,disposition='response',raw=dict(ok=True,assignments={'friendly_channel_names':{'group':'off'}}))]
            for method,args,raw in [
                ('users.prefs.get',{},dict(ok=True,prefs={'frecency':'{}'})),
                ('client.gantryBoot',{'include_channels':'false','_x_reason':'fetch-client-data'},dict(ok=True,self=dict(id='ULIST0001',team_id='T1',profile={}),team=dict(id='T1'),workspaces=[dict(id='T1',prefs={'display_real_names':False}),dict(id='TOTHER001',prefs={'display_real_names':False})],default_workspace='T1')),
                ('api.getFlannelHttpUrl',{'include_external_workspaces':'true','_x_reason':'client-needs-valid-flannel-url'},dict(ok=True,url='https://edgeapi.slack.com/cache/T1',ttl_seconds=300))]:
                serial=len(s['requests'])+1;s['requests'].append(dict(serial=serial,method=method,args=args));s['responses'].append(dict(serial=serial,disposition='response',raw=raw))
        for j,term in enumerate(all_queries[:count]):
            serial=j+5;s['requests'].append(dict(serial=serial,method='channels/search',args=dict(query=term,count=25,fuzz=1,filter='xws',include_record_channels=True,check_membership=True,top_channels=[],default_workspace='T1')))
            disposition='held' if j==0 or not success and j==5 else 'offline' if not success and j==3 else 'response'
            raw=None if disposition=='offline' else dict(ok=True,results=False if not success and j==2 else [] if not success and j==4 else source_rows())
            s['responses'].append(dict(serial=serial,disposition=disposition,raw=raw))
        s.update(completed=([0,0,1,1,2,2,2,2,2] if success else [0,0,2,2,3,4,5,6])[i],cancelled=0 if success else [0,0,1,1,1,1,1,2][i],held=([0,1,0,0,0,0,0,0,0] if success else [0,1,1,0,0,0,0,0])[i])
        if i>= (2 if success else 3):s['releases'].append(dict(serial=5,delivered=success,cancelledBeforeReply=not success))
        if not success and i==7:s['releases'].append(dict(serial=10,delivered=False,cancelledBeforeReply=True))
        m['completionSource']=s
        read_count=([4,4,4,4,4,8,12,12,16] if success else [4,4,4,4,4,4,4,8])[i]
        template=first['model']['ledger']['requests'][:4];m['ledger'].update(requests=[dict(copy.deepcopy(q),serial=b*4+j+1) for b in range(read_count//4) for j,q in enumerate(template)],completed=read_count,cancelled=0,held=False)
        c.update(generation=commentsgen,page=copy.deepcopy(canonical),loading=False,error=None,visibleTs=['1781524801.000001']+([msg['ts']] if sent else []))
        if success and i==8:c['page']['messages'].append(copy.deepcopy(msg))
        compose=dict(prepared=[post] if sending or sent else [],posts=[post] if sending or sent else [],messages=[msg] if sending or sent else [],published=success and i==8,held=False,heldPost=sending,preflights=[],policies=[])
        if sending or sent:
            compose['preflights']=[dict(method='conversations.genericInfo',args={'updated_channels':'{"CCOMMENT01":0}'})]
            compose['policies']=[dict(request=dict(method='client.userBoot',args={'include_min_version_bump_check':'1'}),boot=dict(ok=True,team=dict(id='T1'),self=dict(id='ULIST0001')),channel=dict(id='CCOMMENT01'),threadReadOnly=False)]
        m['composeLedger']=copy.deepcopy(compose)
        capture=f'11111111-2222-4333-8444-{(i if i!=5 or not success else 4):012}'
        revision=(i+2 if i!=5 or not success else 6);pending=None
        if sending:pending=dict(capture=capture,raw=None,record=dict(client_msg_id=post['args']['client_msg_id'],client_ts=post['args']['ts'],blocks=final,workspace='T1',user='ULIST0001',target=dict(document=m['initialDocument'],record=m['records']['R_LIST_000'],channel='CCOMMENT01',root=scope['root'])))
        d=copy.deepcopy(first['model']['draftStore']['drafts'][0]);d.update(id='reopened' if reopen else 'original',generation=gen,blocks=copy.deepcopy(blocks),capture=capture,savedRevision=revision,attempt=pending,busy=sending)
        m['draftStore']=dict(active=f"{d['id']}:{commentsgen}:{gen}",drafts=[d])
        journal=dict(scope=scope,blocks=copy.deepcopy(blocks),capture=capture,revision=revision,attempt=copy.deepcopy(pending))
        p=dict(seed=copy.deepcopy(seed),originalA=copy.deepcopy(a),expectedBlocks=copy.deepcopy(blocks),journal=journal,runtimeForgotten=reopen,
            inputs=copy.deepcopy(inputs[:([0,1,1,2,4,4,4,4,4] if success else [0,1,2,2,3,4,5,6])[i]]),
            releases=[dict(stage='refreshed' if success else 'stale-refused' if n==0 else 'cancelled-reopened',value=copy.deepcopy(x)) for n,x in enumerate(s['releases'])])
        if sent:p.update(postRelease=dict(delivered=True,cancelledBeforeReply=False),sentBlocks=copy.deepcopy(final))
        if not success and i>=3:p['staleChoice']=dict(ok=True,accepted=False,before=snap(query(a,'#project-new')),after=snap(query(a,'#project-new')))
        row['proof']=p
        isopen=i in ({1,2} if success else {1,2,3,4,5,6});labels=[LABELS[LOCAL]] if i==1 else list(LABELS.values()) if success and i==2 else []
        status=None if labels or not isopen else V['FAILURE'] if i in (4,5) else 'No matching channels'
        sequence=10 if i==1 or success and i==2 else 20 if i in (2,3) else i*10
        m['completion']=dict(sequence=sequence,revision=2,labels=labels,current=True,started=True,consumed=False,friendly=False) if isopen else None
        g['completion']=dict(sequence=sequence if isopen else 0,revision=2 if isopen else 0,open=isopen,
            rows=[dict(label=l,id=f'slack-list-channel-choice-{gen}-{sequence}-{j}',selected=str(j==0).lower(),type='button',role='option',visible=True,hit=True,rect=BOX) for j,l in enumerate(labels)],status=dict(text=status,visible=True,hit=True,rect=BOX) if status else None,popup=dict(visible=True,hit=True,rect=BOX) if isopen else None)
        opaque=[v for b in blocks for sec in b['elements'] for v in sec['elements'] if v['type']!='text']
        g['completion']['captureMode']='sending' if sending else 'interactive'
        g['action']['readOnlyText']=None
        if sending:
            text=g['action']['editorText']
            for line in text['lines']:
                line['hit']=False
                for fragment in line['fragments']:fragment['hit']=False
            text.update(ok=False,visibleLines=0)
            g['action']['readOnlyText']=dict(fullVisibleLines=sum(line['fullVisible'] for line in text['lines']),ok=True)
        g['native']=dict(snapshot=snap(blocks,gen),tokens=[dict(value=copy.deepcopy(v),label=LABELS[v['channel_id']] if v['type']=='channel' else '@'+v['user_id'] if v['type']=='user' else v['text']) for v in opaque])
        text='\n'.join(''.join(LABELS[v['channel_id']] if v['type']=='channel' else '@'+v['user_id'] if v['type']=='user' else v.get('text','') for v in sec['elements']) for b in blocks for sec in b['elements'])
        g['editor'].update(blocks=copy.deepcopy(blocks),generation=str(gen),text=text,bold=not sent,editable=not sending,sameNode=not reopen)
        for control in g['controls']:control['disabled']=(sent or sending) if control['id']=='slack-list-comment-send' else False
        g['rows']=[dict(ts=ts,text='comment',rect=BOX) for ts in c['visibleTs']]
        rows.append(row)
    return fixture,dict(ok=True,case=case,fixture=fixture,noLiveNetwork=True,readiness=base['readiness'],initialRecords=base['initialRecords'],stages=rows,final=copy.deepcopy(rows[-1]['model']))


class ChannelCompletionTests(unittest.TestCase):
    def check(self,f,r):return V['validate'](f,r,DIR,png=False)
    def reject(self,change,stage=2,case='channel-completion'):
        f,r=sample(case);change(r['stages'][stage]);r['final']=copy.deepcopy(r['stages'][-1]['model'])
        with self.assertRaises((RuntimeError,KeyError,ValueError,TypeError,IndexError)):self.check(f,r)
    def test_both_cases_both_widths(self):
        for case in V['STAGES']:
            for narrow in (False,True):self.check(*sample(case,narrow))
    def test_legacy_short_self_is_refused(self):self.reject(lambda r:r['model']['completionSource'].update(user='U0'))
    def test_sent_author_is_original_valid_self(self):self.reject(lambda r:r['model']['composeLedger']['messages'][0].update(user='U0'),7)
    def test_http_capability_rejected(self):self.reject(lambda r:r['model']['completionSource'].update(httpCapability=True))
    def test_missing_protected_identity(self):self.reject(lambda r:r['model']['completionSource'].update(identityCurrent=False))
    def test_wrong_session_assumption(self):self.reject(lambda r:r['model'].update(noSession=True))
    def test_cross_team_rewritten(self):self.reject(lambda r:r['model']['completionSource']['responses'][-1]['raw']['results'][1].update(team_id='T1'))
    def test_private_source_rewritten(self):self.reject(lambda r:r['model']['completionSource']['responses'][-1]['raw']['results'][0].update(is_private=False))
    def test_previous_name_dropped(self):self.reject(lambda r:r['model']['completionSource']['responses'][-1]['raw']['results'][1].update(previous_names=[]))
    def test_prefix_sent_to_wire(self):self.reject(lambda r:r['model']['completionSource']['requests'][-1]['args'].update(query='#project'))
    def test_extra_wire_field(self):self.reject(lambda r:r['model']['completionSource']['requests'][-1]['args'].update(marker='extra'))
    def test_required_source_observation(self):self.reject(lambda r:r['model']['completionSource']['requests'][1].update(method='search.autocomplete.model',args={'_x_reason':'AutocompleteModelCache'}))
    def test_unknown_endpoint(self):self.reject(lambda r:r['model']['completionSource']['requests'][-1].update(method='chat.delete'))
    def test_raw_failure_fabricated_as_success(self):self.reject(lambda r:r['model']['completionSource']['responses'][-1].update(disposition='response',raw={'ok':True,'results':[]}),5,'channel-completion-custody')
    def test_late_reply_not_delivered(self):self.reject(lambda r:r['model']['completionSource']['releases'][0].update(delivered=True,cancelledBeforeReply=False),3,'channel-completion-custody')
    def test_stale_choice_must_refuse(self):self.reject(lambda r:r['proof']['staleChoice'].update(accepted=True),3,'channel-completion-custody')
    def test_query_text_durable(self):self.reject(lambda r:r['proof']['journal'].update(blocks=r['proof']['originalA']),4,'channel-completion-custody')
    def test_opaque_duplicate_preserved(self):self.reject(lambda r:r['proof']['originalA'][0]['elements'][0]['elements'][4]['opaque'].update(occurrence=2))
    def test_inserted_canonical_label(self):self.reject(lambda r:r['geometry']['native']['tokens'][-1].update(label='#project-previous'),4)
    def test_style_preserved(self):self.reject(lambda r:r['geometry']['editor'].update(bold=False))
    def test_same_native_node(self):self.reject(lambda r:r['geometry']['editor'].update(sameNode=False))
    def test_reopen_new_runtime(self):self.reject(lambda r:r['model']['draftStore']['drafts'][0].update(id='original'),5)
    def test_journal_saved_revision(self):self.reject(lambda r:r['proof']['journal'].update(revision=999))
    def test_popup_option_clipped(self):self.reject(lambda r:r['geometry']['completion']['rows'][0].update(visible=False))
    def test_closed_editor_text_clipped(self):self.reject(lambda r:r['geometry']['action']['editorText'].update(ok=False,visibleLines=0),4)
    def test_retained_query_cannot_be_forged_in_all_views(self):
        f,r=sample('channel-completion-custody');row=r['stages'][4]
        blocks=copy.deepcopy(row['proof']['originalA'])
        row['proof']['expectedBlocks']=blocks;row['proof']['journal']['blocks']=blocks
        row['model']['draftStore']['drafts'][0]['blocks']=blocks;row['geometry']['editor']['blocks']=blocks;row['geometry']['native']['snapshot']['blocks']=blocks
        with self.assertRaises(RuntimeError):self.check(f,r)
    def test_failure_never_posts(self):self.reject(lambda r:r['model']['composeLedger']['posts'].append({'method':'chat.postMessage','args':{}}),5,'channel-completion-custody')
    def test_held_editor_must_be_readonly(self):self.reject(lambda r:r['geometry']['editor'].update(editable=True),6)
    def test_held_send_must_be_disabled(self):self.reject(lambda r:next(c for c in r['geometry']['controls'] if c['id']=='slack-list-comment-send').update(disabled=False),6)
    def test_held_visibility_count_exact(self):self.reject(lambda r:r['geometry']['action']['readOnlyText'].update(fullVisibleLines=999),6)
    def test_held_original_hit_evidence_preserved(self):self.reject(lambda r:r['geometry']['action']['editorText'].update(ok=True,visibleLines=1),6)
    def test_held_all_lines_clipped(self):
        def clip(row):
            t=row['geometry']['action']['editorText'];t['clip'].update(bottom=t['clip']['top']+1,height=1)
            for line in t['lines']:line['fullVisible']=False
            row['geometry']['action']['readOnlyText']=dict(fullVisibleLines=0,ok=False)
        self.reject(clip,6)
    def test_readonly_bypass_requires_real_held_post(self):self.reject(lambda r:r['model']['composeLedger'].update(heldPost=False),6)
    def test_readonly_bypass_requires_pending_journal(self):self.reject(lambda r:r['proof']['journal'].update(attempt=None),6)
    def test_interactive_hit_still_required(self):
        def lose_hit(row):
            t=row['geometry']['action']['editorText'];t.update(ok=False,visibleLines=0)
            for line in t['lines']:
                line['hit']=False
                for f in line['fragments']:f['hit']=False
        self.reject(lose_hit,4)
    def test_readonly_mode_refused_outside_send(self):self.reject(lambda r:r['geometry']['completion'].update(captureMode='sending'),4)
    def test_retained_native_inert_geometry(self):
        path=Path(__file__).resolve().parents[1]/'tmp/list-comments-channel-completion-20261019/success-normal-d/slack-list/slack-list.json'
        if not path.is_file():self.skipTest('Retained native diagnostic is local evidence')
        receipt=json.loads(path.read_text());error=receipt['terminalReceipts'][0]['error']
        self.assertTrue(error.startswith('List DOM did not settle: '));body=error.split(': ',1)[1];g,end=json.JSONDecoder().raw_decode(body)
        self.assertTrue(body[end:].startswith('; assertion: '))
        self.assertFalse(g['editor']['editable']);self.assertFalse(g['action']['editorText']['ok'])
        # Explicit adapter for historical pre-mode diagnostic; original line/hit measurements untouched.
        g['completion']['captureMode']='sending';g['action']['readOnlyText']=dict(fullVisibleLines=sum(l['fullVisible'] for l in g['action']['editorText']['lines']),ok=True)
        V['held_text'](g)
        g['editor']['editable']=True
        with self.assertRaises(RuntimeError):V['held_text'](g)
    def test_empty_seed_scaffold_is_consumed_exactly(self):
        first=V['original_blocks']()[0]['elements'][0]['elements']
        self.assertEqual(first[-1],dict(type='text',text='Final review \u4e2d\U0001f642',style={'bold':True}))
        self.assertEqual(V['seed_blocks']()[0]['elements'][0]['elements'][-1],dict(type='text',text='\n'))
    def test_retained_normal_e_reaches_strict_stale_popup_failure(self):
        path=Path(__file__).resolve().parents[1]/'tmp/list-comments-channel-completion-20261019/success-normal-e/slack-list/slack-list.json'
        if not path.is_file():self.skipTest('Retained native diagnostic is local evidence')
        wrapper=json.loads(path.read_text());receipt=wrapper['terminalReceipts'][0]
        self.assertFalse(wrapper['ok']);self.assertEqual(len(receipt['stages']),9)
        model=receipt['stages'][2]['model']['completion'];dom=receipt['stages'][2]['geometry']['completion']
        self.assertEqual(model['labels'],['#project-local','#project-private','#project-cross-team'])
        self.assertEqual([row['label'] for row in dom['rows']],['#project-local'])
        self.assertEqual((model['revision'],dom['revision']),(2,1))
        for row in receipt['stages']:V['H']['png_signature'](Path(row['capture']),wrapper['expectedPngDimensions'])
        with self.assertRaisesRegex(RuntimeError,'Canonical suggestion rows changed'):
            V['validate']('slack-list',receipt,path.parent,dimensions=wrapper['expectedPngDimensions'])
    def test_retained_other_e_captures_keep_strict_popup_contract(self):
        root=Path(__file__).resolve().parents[1]/'tmp/list-comments-channel-completion-20261019'
        cases=[('success-narrow-e','slack-list-narrow','Canonical suggestion rows changed'),('custody-normal-e','slack-list','Failure/empty feedback changed')]
        if not all((root/name/f/(f+'.json')).is_file() for name,f,_ in cases):self.skipTest('Retained native diagnostics are local evidence')
        for name,f,error in cases:
            path=root/name/f/(f+'.json');wrapper=json.loads(path.read_text());receipt=wrapper['terminalReceipts'][0]
            for row in receipt['stages']:V['H']['png_signature'](Path(row['capture']),wrapper['expectedPngDimensions'])
            with self.assertRaisesRegex(RuntimeError,error):V['validate'](f,receipt,path.parent,dimensions=wrapper['expectedPngDimensions'])
        custody=json.loads((root/'custody-normal-e/slack-list/slack-list.json').read_text())['terminalReceipts'][0]
        self.assertEqual(custody['stages'][2]['geometry']['completion']['status']['text'],'Searching channels...')
        self.assertEqual((custody['stages'][6]['model']['completion']['revision'],custody['stages'][6]['geometry']['completion']['revision']),(2,1))
    def test_unobserved_extra_blank_line_is_not_permitted(self):
        f,r=sample()
        for row in r['stages']:row['proof']['originalA'][0]['elements'][0]['elements'].append(dict(type='text',text='\n'))
        with self.assertRaisesRegex(RuntimeError,'Original styled/opaque rich content changed'):self.check(f,r)
    def test_post_count(self):self.reject(lambda r:r['model']['composeLedger']['posts'].append(copy.deepcopy(r['model']['composeLedger']['posts'][0])),6)
    def test_held_request_not_projected(self):self.reject(lambda r:r['model']['comments']['visibleTs'].append('1781525001.000001'),6)
    def test_ack_readback_dedup(self):self.reject(lambda r:r['model']['comments']['page']['messages'].append(copy.deepcopy(r['model']['comments']['page']['messages'][-1])),8)
    def test_runner_dispatch(self):
        for case in V['STAGES']:self.assertEqual(V['C']['case_contract'](case)[0],V['STAGES'][case])
    def test_legacy_stays_no_session(self):
        f,r=BASE['sample']();r['stages'][0]['model']['noSession']=False
        with self.assertRaises(RuntimeError):BASE['V']['validate'](f,r,BASE['DIR'],png=False)

if __name__=='__main__':unittest.main()
