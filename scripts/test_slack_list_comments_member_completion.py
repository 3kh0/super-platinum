"""Portable/adversarial closed List channel completion receipts; no app or Slack IO."""
import copy
import json
import runpy
import sys
import unittest
from unittest.mock import patch
from types import SimpleNamespace
from pathlib import Path
V = runpy.run_path(str(Path(__file__).with_name('slack_list_comments_member_completion.py')))
BASE = runpy.run_path(str(Path(__file__).with_name('test_slack_list_comments_reaction_selection.py')))
DIR = Path('/private/member-completion')
LOCAL, PRIVATE, REMOTE = 'CLISTLOCAL','ULISTMEM2','ULISTMEM1'
FIRST,SECOND,RETIRED,SELF=V['FIRST'],V['SECOND'],V['RETIRED'],V['SELF']
LABELS = V['LABELS']
BOX = dict(left=10, top=10, right=210, bottom=35, width=200, height=25)


def snap(blocks, generation=20):
    return dict(blocks=copy.deepcopy(blocks), generation=str(generation), revision=10, composing=False)


def tail(original, values):
    v=copy.deepcopy(original);v[0]['elements'][0]['elements'].extend(values);return v


def query(original, term):return tail(original,[dict(type='text',text=' '+term)])


def inserted(original, ids):
    values=[]
    for id in ids:values.extend([dict(type='text',text=' '),dict(type='user',user_id=id),dict(type='text',text=' ')])
    return tail(original,values)


def source_base(model):
    return dict(enabled=True,named=True,protectedFixture=True,identityCurrent=True,httpCapability=False,networkCalls=0,
        account=model['account'],epoch=model['epoch'],team='T1',user='ULIST0001',requests=[],responses=[],releases=[],seed=V['seed_source'](),completed=0,cancelled=0,held=0)


def source_rows():return V['source_rows']()


def sample(case='member-completion',narrow=False):
    fixture,base=BASE['sample'](narrow=narrow);first=copy.deepcopy(base['stages'][0]);seed=first['proof']['seed'];a=first['proof']['originalA'];seed['scope']['user']='ULIST0001';first['model']['draftStore']['drafts'][0]['scope']['user']='ULIST0001'
    tokens=[dict(type='channel',channel_id=LOCAL,style={'bold':True},opaque={'occurrence':1}),dict(type='channel',channel_id=LOCAL,style={'italic':True},opaque={'occurrence':2})]
    for b in (seed['blocks'],a):b[0]['elements'][0]['elements'][4:4]=copy.deepcopy(tokens)
    assert a[0]['elements'][0]['elements'][-1]==dict(type='text',text='\n')
    a[0]['elements'][0]['elements'].pop()
    success=case=='member-completion';scope=seed['scope'];canonical=first['model']['comments']['page'];canonical['messages'][1].pop('selection_fixture_opaque',None);canonical['messages'][1]['reactions']=[dict(name='wave',count=2,users=['U1','U2'])];canonical['messages'][0]['reply_users']=[SECOND]
    final=inserted(a,[PRIVATE,REMOTE]);source=source_base(first['model']);rows=[]
    post=dict(method='chat.postMessage',args=dict(channel='CCOMMENT01',thread_ts=scope['root'],ts='1781525000.000001',type='message',reply_broadcast='false',include_channel_perm_error='true',client_msg_id='00000000-0000-4000-8000-000000000001',blocks=json.dumps(final),_x_reason='webapp_message_send'))
    msg=dict(type='message',channel='CCOMMENT01',thread_ts=scope['root'],ts='1781525001.000001',client_msg_id=post['args']['client_msg_id'],blocks=final,user='ULIST0001')
    input_plan=[('local-held','query','@alex'),('keyboard-inserted','choose',PRIVATE),('mouse-inserted','query','@alex'),('mouse-inserted','choose',REMOTE)] if success else [('held','query','@alex'),('query-retired','query','@nomatch'),('malformed','query','@malformed'),('failure','query','@offline'),('empty','query','@empty'),('cancelled-reopened','query','@cancelled')]
    inputs=[]
    for j,(stage,kind,value) in enumerate(input_plan):
        before=query(inserted(a,[PRIVATE]) if success and j>=2 else a, '@alex' if success and j>=2 else '@alex' if kind=='choose' else value)
        v=dict(ok=True,query=value,native=snap(before),sequence=0,revision=0) if kind=='query' else dict(ok=True,label=LABELS[value],keyboard=j==1,index=0 if j==1 else 1,sequence=10 if j==1 else 20,revision=2,before=snap(before))
        item=dict(stage=stage,kind=kind,value=v)
        if kind=='choose':item['user']=value;item['values']=[dict(value=dict(type='user',user_id=id),label=LABELS[id]) for id in [SECOND,FIRST,RETIRED,SELF]]
        inputs.append(item)
    all_queries=['alex','alex'] if success else ['alex','nomatch','malformed','offline','empty','cancelled']
    for i,stage in enumerate(V['STAGES'][case]):
        row=copy.deepcopy(first);row.update(stage=stage,capture=str(DIR/f'{fixture}-{stage}.png'));m,g=row['model'],row['geometry'];c=m['comments']
        for k in ('selectionSource','selectionProbe','actionProbe'):m.pop(k,None)
        m.update(noSession=False,noTransport=False)
        reopen=i>= (5 if success else 7);sent=success and i>=7;sending=success and i==6
        gen=40 if sent else 30 if reopen else 20;commentsgen=2 if reopen else 1
        blocks=([] if sent else final if success and i>=4 else inserted(a,[PRIVATE]) if success and i==3 else query(a,'@alex') if success and i in (1,2) else a) if success else (a if i==0 else query(a,['','@alex','@nomatch','@nomatch','@malformed','@offline','@empty','@cancelled'][i]))
        count=([0,1,1,1,2,2,2,2,2] if success else [0,1,2,2,3,4,5,6])[i]
        s=copy.deepcopy(source)
        if count:
            for method in ['experiments.getByUser','users.prefs.get','client.gantryBoot','api.getFlannelHttpUrl','channels/info','users/list','channels/membership']:
                args,raw=V['observations']()[method];serial=len(s['requests'])+1
                s['requests'].append(dict(serial=serial,method=method,args=copy.deepcopy(args)));s['responses'].append(dict(serial=serial,disposition='response',raw=copy.deepcopy(raw)))
            for id,version in ((FIRST,101),(SECOND,0)):
                args=dict(updated_ids={id:version},check_interaction=True,include_profile_only_users=True);serial=len(s['requests'])+1
                s['requests'].append(dict(serial=serial,method='users/info',args=args));s['responses'].append(dict(serial=serial,disposition='response',raw=V['interaction_observation'](args)))
        for j,term in enumerate(all_queries[:count]):
            serial=len(s['requests'])+1;s['requests'].append(dict(serial=serial,method='users/search',args=dict(query=term,count=25,fuzz=1,enable_workspace_ranking=True,top_users=[],default_workspace='T1')))
            disposition='held' if j==0 or not success and j==5 else 'offline' if not success and j==3 else 'response'
            raw=None if disposition=='offline' else dict(ok=True,results=False if not success and j==2 else [] if not success and j==4 else source_rows())
            s['responses'].append(dict(serial=serial,disposition=disposition,raw=raw))
        s.update(completed=([0,0,1,1,2,2,2,2,2] if success else [0,0,2,2,3,4,5,6])[i],cancelled=0 if success else [0,0,1,1,1,1,1,2][i],held=([0,1,0,0,0,0,0,0,0] if success else [0,1,1,0,0,0,0,0])[i])
        if i>= (2 if success else 3):s['releases'].append(dict(serial=10,delivered=success,cancelledBeforeReply=not success))
        if not success and i==7:s['releases'].append(dict(serial=15,delivered=False,cancelledBeforeReply=True))
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
        if not success and i>=3:p['staleChoice']=dict(ok=True,accepted=False,before=snap(query(a,'@nomatch')),after=snap(query(a,'@nomatch')))
        row['proof']=p
        isopen=i in ({1,2} if success else {1,2,3,4,5,6});ids=[SECOND,FIRST,SELF] if i==1 else [SECOND,FIRST,RETIRED,SELF] if success and i==2 else [];labels=[LABELS[id] for id in ids]
        status=None if labels or not isopen else V['FAILURE'] if i in (4,5) else 'No matching mentions'
        sequence=10 if i==1 or success and i==2 else 20 if i in (2,3) else i*10
        m['completion']=dict(sequence=sequence,revision=2,labels=labels,current=True,started=True,consumed=False,friendly=False,values=[dict(value=dict(type='user',user_id=id),label=LABELS[id]) for id in ids]) if isopen else None
        g['completion']=dict(sequence=sequence if isopen else 0,revision=2 if isopen else 0,open=isopen,
            rows=[dict(label=l,id=f'slack-list-channel-choice-{gen}-{sequence}-{j}',selected=str(j==0).lower(),type='button',role='option',visible=True,hit=True,fullyVisible=True,rect=BOX) for j,l in enumerate(labels)],status=dict(text=status,visible=True,hit=True,rect=BOX) if status else None,popup=dict(visible=True,hit=True,rect=BOX) if isopen else None)
        opaque=[v for b in blocks for sec in b['elements'] for v in sec['elements'] if v['type']!='text']
        g['completion']['scroll']=dict(top=0,height=25,total=25) if isopen else None
        g['completion']['captureMode']='sending' if sending else 'interactive'
        g['action']['readOnlyText']=None
        if sending:
            text=g['action']['editorText']
            for line in text['lines']:
                line['hit']=False
                for fragment in line['fragments']:fragment['hit']=False
            text.update(ok=False,visibleLines=0)
            g['action']['readOnlyText']=dict(fullVisibleLines=sum(line['fullVisible'] for line in text['lines']),ok=True)
        g['native']=dict(snapshot=snap(blocks,gen),tokens=[dict(value=copy.deepcopy(v),label=LABELS[v['channel_id']] if v['type']=='channel' else LABELS.get(v['user_id'],'@'+v['user_id']) if v['type']=='user' else v['text']) for v in opaque])
        text='\n'.join(''.join(LABELS[v['channel_id']] if v['type']=='channel' else LABELS.get(v['user_id'],'@'+v['user_id']) if v['type']=='user' else v.get('text','') for v in sec['elements']) for b in blocks for sec in b['elements'])
        g['editor'].update(blocks=copy.deepcopy(blocks),generation=str(gen),text=text,bold=not sent,editable=not sending,sameNode=not reopen)
        for control in g['controls']:control['disabled']=(sent or sending) if control['id']=='slack-list-comment-send' else False
        g['rows']=[dict(ts=ts,text='comment',rect=BOX) for ts in c['visibleTs']]
        if success and i==2:p['bottomWindow']=dict(model=copy.deepcopy(m),geometry=copy.deepcopy(g),capture=str(DIR/(fixture+'-refreshed-bottom.png')))
        rows.append(row)
    return fixture,dict(ok=True,case=case,fixture=fixture,noLiveNetwork=True,readiness=base['readiness'],initialRecords=base['initialRecords'],stages=rows,final=copy.deepcopy(rows[-1]['model']))


class MemberCompletionTests(unittest.TestCase):
    def test_mixed_backend_empty_group_request_keeps_member_fixture_closed(self):
        _, receipt = sample('member-completion')
        model = receipt['stages'][-1]['model']
        source = model['completionSource']
        serial = len(source['requests']) + 1
        source['requests'].append(dict(serial=serial,method='usergroups/search',
            args=dict(query='alex',count=25,org_wide=True)))
        source['responses'].append(dict(serial=serial,disposition='response',raw=dict(ok=True,results=[])))
        V['closed_source'](source,model)
        V['source_receipts'](source)
        source['requests'][-1]['args']['channel'] = 'CCOMMENT01'
        with self.assertRaises(RuntimeError): V['closed_source'](source,model)
        del source['requests'][-1]['args']['channel']
        source['responses'][-1]['raw']['results'] = [dict(id='SLISTGRP1')]
        with self.assertRaises(RuntimeError): V['source_receipts'](source)
    def check(self,f,r):return V['validate'](f,r,DIR,png=False)
    def reject(self,change,stage=2,case='member-completion'):
        f,r=sample(case);change(r['stages'][stage]);r['final']=copy.deepcopy(r['stages'][-1]['model'])
        with self.assertRaises((RuntimeError,KeyError,ValueError,TypeError,IndexError)):self.check(f,r)
    def test_both_cases_both_widths(self):
        for case in V['STAGES']:
            for narrow in (False,True):self.check(*sample(case,narrow))
    def test_duplicate_label_does_not_authorize_wrong_id(self):self.reject(lambda r:r['model']['completion']['values'][0]['value'].update(user_id=FIRST))
    def test_root_reply_users_not_inferred(self):self.reject(lambda r:r['model']['comments']['page']['messages'][0].update(reply_users=[FIRST]))
    def test_actual_membership_false_preserved(self):
        def change(r):
            x=next(v for v in r['model']['completionSource']['responses'] if v['raw'] and 'non_members' in v['raw']);x['raw']['non_members']=[]
        self.reject(change)
    def test_interaction_cannot_be_inferred_from_membership(self):
        def change(r):
            request=next(v for v in r['model']['completionSource']['requests'] if v['method']=='users/info');request['args']['check_interaction']=False
        self.reject(change)
    def test_interaction_response_false_refused(self):
        def change(r):
            response=next(v for v in r['model']['completionSource']['responses'] if v['raw'] and 'can_interact' in v['raw']);response['raw']['can_interact'][FIRST]=False
        self.reject(change)
    def test_deleted_bot_cannot_appear(self):self.reject(lambda r:r['model']['completion']['values'][2]['value'].update(user_id='ULISTMEM4'))
    def test_member_opaque_raw_preserved(self):self.reject(lambda r:r['model']['completionSource']['responses'][-1]['raw']['results'][1].pop('fixture_opaque'))
    def test_no_http(self):self.reject(lambda r:r['model']['completionSource'].update(httpCapability=True))
    def test_invalid_wire_count(self):self.reject(lambda r:r['model']['completionSource']['requests'][-1]['args'].update(count=20))
    def test_wrong_directory_endpoint(self):self.reject(lambda r:r['model']['completionSource']['requests'][-1].update(method='channels/search'))
    def test_original_seed_required(self):self.reject(lambda r:r['model']['completionSource']['seed']['boot'].update(users=[]))
    def test_unseen_row_refused(self):
        def change(r):
            for g in (r['geometry'],r['proof']['bottomWindow']['geometry']):
                o=g['completion']['rows'][3];o['rect']=dict(left=10,top=40,right=210,bottom=65,width=200,height=25);o.update(visible=False,hit=False,fullyVisible=False)
        self.reject(change)
    def test_fabricated_visibility_refused(self):self.reject(lambda r:r['geometry']['completion']['rows'][0].update(fullyVisible=False))
    def test_bottom_original_model_required(self):self.reject(lambda r:r['proof']['bottomWindow']['model']['completion'].update(sequence=999))
    def test_styles_and_opaque_preserved(self):self.reject(lambda r:r['proof']['expectedBlocks'][0]['elements'][0]['elements'][4].update(style={}))
    def test_user_label_must_be_resolved(self):self.reject(lambda r:r['geometry']['native']['tokens'][-1].update(label='@wrong'),4)
    def test_user_id_not_label_alias(self):self.reject(lambda r:r['geometry']['native']['tokens'][-1]['value'].update(user_id=SECOND),4)
    def test_journal_exact(self):self.reject(lambda r:r['proof']['journal'].update(blocks=[]),4)
    def test_second_post_forbidden(self):self.reject(lambda r:r['model']['composeLedger']['posts'].append(copy.deepcopy(r['model']['composeLedger']['posts'][0])),6)
    def test_query_retired_stays_cancelled(self):self.reject(lambda r:r['model']['completionSource']['releases'][0].update(delivered=True,cancelledBeforeReply=False),3,'member-completion-custody')
    def test_stale_choice_refused(self):self.reject(lambda r:r['proof']['staleChoice'].update(accepted=True),3,'member-completion-custody')
    def test_raw_offline_not_success(self):self.reject(lambda r:r['model']['completionSource']['responses'][-1].update(disposition='response',raw=dict(ok=True,results=[])),5,'member-completion-custody')
    def test_source_request_receipt_prefix(self):self.reject(lambda r:r['model']['completionSource']['requests'][0].update(serial=8))
    def test_no_extra_blank_line(self):self.reject(lambda r:r['geometry']['editor'].update(text=r['geometry']['editor']['text']+'\n'),4)
    def test_actual_unicode(self):
        text=V['display'](V['original_blocks']());self.assertIn(chr(0x4e2d)+chr(0x1f642),text)
    def test_dispatch(self):
        for case in V['STAGES']:self.assertEqual(V['C']['case_contract'](case)[0],V['STAGES'][case])

    def lifecycle(self, case, edges):
        f,r=sample(case)
        c=V['C']; ready=r['readiness']
        packets={c['WAITING']:[ready],c['PREFIX']:[r],c['EDGE']:edges}
        native=SimpleNamespace(receipts=lambda prefix:packets[prefix])
        # Isolate the runner's lifecycle check from the separately tested body.
        contract=lambda _: (V['STAGES'][case],lambda *args,**kwargs:r)
        with patch.dict(c['validate_log'].__globals__,case_contract=contract):
            return c['validate_log'](native,f,DIR,ready,None,case)

    def test_exact_member_capture_lifecycle(self):
        captures=('draft','local-held','refreshed','refreshed-bottom','keyboard-inserted',
                  'mouse-inserted','reopened','sending','sent','readback')
        edges=[dict(stage=s,edge=e) for s in captures for e in ('before','after')]
        self.lifecycle('member-completion',edges)
        malformed=[edges[:6]+edges[8:],edges[:6]+edges[7:8]+edges[6:7]+edges[8:],
                   edges[:8]+edges[6:8]+edges[8:],edges+[dict(stage='other',edge='before')]]
        for bad in malformed:
            with self.subTest(edges=bad),self.assertRaisesRegex(RuntimeError,'Capture lifecycle edges changed'):
                self.lifecycle('member-completion',bad)

    def test_custody_does_not_admit_extra_bottom_capture(self):
        edges=[dict(stage=s,edge=e) for s in V['STAGES']['member-completion-custody'] for e in ('before','after')]
        self.lifecycle('member-completion-custody',edges)
        with self.assertRaisesRegex(RuntimeError,'Capture lifecycle edges changed'):
            self.lifecycle('member-completion-custody',edges+[dict(stage='refreshed-bottom',edge=e) for e in ('before','after')])

    def test_retained_native_normal_a_log_and_original_pngs(self):
        path=Path(__file__).resolve().parents[1]/'tmp/list-comments-member-completion-20261019/member-normal-a/slack-list/slack-list.json'
        if not path.is_file():self.skipTest('Retained local native evidence is not installed')
        evidence=json.loads(path.read_text(encoding='utf-8'));c=V['C']
        log=SimpleNamespace(log=path.with_suffix('.log'))
        native=SimpleNamespace(receipts=lambda prefix:c['NativeFixture'].receipts(log,prefix))
        result=c['validate_log'](native,evidence['fixture'],Path(evidence['readyDirectory']),
            evidence['readiness'],evidence['expectedPngDimensions'],evidence['case'])
        self.assertEqual(result,evidence['terminalReceipts'][0])
        self.assertFalse(evidence['ok'])
        self.assertEqual(evidence['error'],'Capture lifecycle edges changed')

if __name__=='__main__':unittest.main()
