"""Portable/adversarial closed List channel completion receipts; no app or Slack IO."""
import copy
import json
import runpy
import sys
import unittest
from unittest.mock import patch
from types import SimpleNamespace
from pathlib import Path
V = runpy.run_path(str(Path(__file__).with_name('slack_list_comments_group_completion.py')))
BASE = runpy.run_path(str(Path(__file__).with_name('test_slack_list_comments_reaction_selection.py')))
DIR = Path('/private/group-completion')
LOCAL,PRIVATE,REMOTE=V['LOCAL'],V['PRIVATE'],V['REMOTE']
G1,G2,G3,G4=V['G1'],V['G2'],V['G3'],V['G4']
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
    for id in ids:values.extend([dict(type='text',text=' '),(dict(type='usergroup',usergroup_id=id) if id.startswith('S') else dict(type='user',user_id=id)),dict(type='text',text=' ')])
    return tail(original,values)


def source_base(model):
    return dict(enabled=True,named=True,protectedFixture=True,identityCurrent=True,httpCapability=False,networkCalls=0,
        account=model['account'],epoch=model['epoch'],team='T1',user='ULIST0001',requests=[],responses=[],releases=[],seed=V['seed_source'](),completed=0,cancelled=0,held=0)


def source_rows():return V['source_rows']()


def sample(case='group-completion',narrow=False):
    fixture,base=BASE['sample'](narrow=narrow);first=copy.deepcopy(base['stages'][0]);seed=first['proof']['seed'];a=first['proof']['originalA'];seed['scope']['user']='ULIST0001';first['model']['draftStore']['drafts'][0]['scope']['user']='ULIST0001'
    tokens=[dict(type='channel',channel_id=LOCAL,style={'bold':True},opaque={'occurrence':1}),dict(type='channel',channel_id=LOCAL,style={'italic':True},opaque={'occurrence':2})]
    for b in (seed['blocks'],a):b[0]['elements'][0]['elements'][4:4]=copy.deepcopy(tokens)
    assert a[0]['elements'][0]['elements'][-1]==dict(type='text',text='\n')
    a[0]['elements'][0]['elements'].pop()
    success=case=='group-completion';scope=seed['scope'];canonical=first['model']['comments']['page'];canonical['messages'][1].pop('selection_fixture_opaque',None);canonical['messages'][1]['reactions']=[dict(name='wave',count=2,users=['U1','U2'])];canonical['messages'][0]['reply_users']=[SECOND]
    final=inserted(a,[PRIVATE,REMOTE]);source=source_base(first['model']);rows=[]
    post=dict(method='chat.postMessage',args=dict(channel='CCOMMENT01',thread_ts=scope['root'],ts='1781525000.000001',type='message',reply_broadcast='false',include_channel_perm_error='true',client_msg_id='00000000-0000-4000-8000-000000000001',blocks=json.dumps(final),_x_reason='webapp_message_send'))
    msg=dict(type='message',channel='CCOMMENT01',thread_ts=scope['root'],ts='1781525001.000001',client_msg_id=post['args']['client_msg_id'],blocks=final,user='ULIST0001')
    input_plan=[('local-held','query','@alex'),('keyboard-inserted','choose',PRIVATE),('mouse-inserted','query','@alex'),('mouse-inserted','choose',REMOTE)] if success else [('held','query','@alex'),('query-retired','query','@nomatch'),('malformed','query','@malformed'),('failure','query','@offline'),('empty','query','@empty'),('cancelled-reopened','query','@cancelled')]
    inputs=[]
    for j,(stage,kind,value) in enumerate(input_plan):
        before=query(inserted(a,[PRIVATE]) if success and j>=2 else a, '@alex' if success and j>=2 else '@alex' if kind=='choose' else value)
        v=dict(ok=True,query=value,native=snap(before),sequence=0,revision=0) if kind=='query' else dict(ok=True,label=LABELS[value],keyboard=j==1,index=5 if j==1 else 4,sequence=10 if j==1 else 20,revision=2,before=snap(before))
        item=dict(stage=stage,kind=kind,value=v)
        if kind=='choose':item['selected']=value;item['values']=[V['choice'](id) for id in [SECOND,FIRST,RETIRED,SELF,G1,G2,G3]]
        if kind=='choose':item['publication']=dict(sequence=v['sequence'],revision=v['revision'],labels=[x['label'] for x in item['values']],values=copy.deepcopy(item['values']),current=True,batchCurrent=True,settledBatch=True,started=True,consumed=False,friendly=False)
        inputs.append(item)
    all_queries=['alex','alex'] if success else ['alex','nomatch','malformed','offline','empty','cancelled']
    settled={}
    for i,stage in enumerate(V['STAGES'][case][:9 if success else 8]):
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
            serial=len(s['requests'])+1
            s['requests'].append(dict(serial=serial,method='users/search',args=dict(query=term,count=25,fuzz=1,enable_workspace_ranking=True,top_users=[],default_workspace='T1')))
            s['responses'].append(dict(serial=serial,disposition='response',raw=dict(ok=True,results=source_rows())))
            serial+=1
            s['requests'].append(dict(serial=serial,method='usergroups/search',args=dict(query=term,count=25,org_wide=True)))
            disposition='held' if j==0 or not success and j==5 else 'offline' if not success and j==3 else 'response'
            raw=None if disposition=='offline' else dict(ok=True,results=False if not success and j==2 else [] if not success and j==4 else V['group_rows']())
            s['responses'].append(dict(serial=serial,disposition=disposition,raw=raw))
        s.update(completed=([0,0,1,1,2,2,2,2,2] if success else [0,0,2,2,3,4,5,6])[i],cancelled=0 if success else [0,0,1,1,1,1,1,2][i],held=([0,1,0,0,0,0,0,0,0] if success else [0,1,1,0,0,0,0,0])[i])
        if i>= (2 if success else 3):s['releases'].append(dict(serial=11,delivered=success,cancelledBeforeReply=not success))
        if not success and i==7:s['releases'].append(dict(serial=21,delivered=False,cancelledBeforeReply=True))
        s['groups']=V['group_state'](V['group_rows']()[:1] if i<2 else V['group_rows']())
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
        isopen=i in ({1,2} if success else {1,2,3,4,5,6});ids=[SECOND,FIRST,RETIRED,SELF,G1] if i==1 else [SECOND,FIRST,RETIRED,SELF,G1,G2,G3] if success and i==2 else [];labels=[LABELS[id] for id in ids]
        status=None if labels or not isopen else V['FAILURE'] if i in (4,5) else 'No matching mentions'
        sequence=10 if i==1 or success and i==2 else 20 if i in (2,3) else i*10
        m['completion']=dict(sequence=sequence,revision=2,labels=labels,current=True,batchCurrent=True,settledBatch=True,started=True,consumed=False,friendly=False,values=[dict(value=(dict(type='usergroup',usergroup_id=id) if id.startswith('S') else dict(type='user',user_id=id)),label=LABELS[id]) for id in ids]) if isopen else None
        if isopen and status==V['FAILURE']:m['completion'].update(batchCurrent=False,settledBatch=False)
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
        g['native']=dict(snapshot=snap(blocks,gen),tokens=[dict(value=copy.deepcopy(v),label=LABELS[v['channel_id']] if v['type']=='channel' else LABELS.get(v['user_id'],'@'+v['user_id']) if v['type']=='user' else LABELS[v['usergroup_id']] if v['type']=='usergroup' else v['text']) for v in opaque])
        text='\n'.join(''.join(LABELS[v['channel_id']] if v['type']=='channel' else LABELS.get(v['user_id'],'@'+v['user_id']) if v['type']=='user' else LABELS[v['usergroup_id']] if v['type']=='usergroup' else v.get('text','') for v in sec['elements']) for b in blocks for sec in b['elements'])
        g['editor'].update(blocks=copy.deepcopy(blocks),generation=str(gen),text=text,bold=not sent,editable=not sending,sameNode=not reopen)
        for control in g['controls']:control['disabled']=(sent or sending) if control['id']=='slack-list-comment-send' else False
        g['rows']=[dict(ts=ts,text='comment',rect=BOX) for ts in c['visibleTs']]
        if stage in ('local-held','refreshed','held'):p['bottomWindow']=dict(model=copy.deepcopy(m),geometry=copy.deepcopy(g),capture=str(DIR/(fixture+'-'+stage+'-bottom.png')))
        if stage in ('query-retired','stale-refused','malformed','failure','empty'):
            q=m['completion'];settled[stage]=dict(completion=copy.deepcopy(q),native=dict(ok=True,ready=True,sequence=q['sequence'],revision=q['revision'],labels=q['labels'],status=status))
        if settled:p['settled']=copy.deepcopy(settled)
        rows.append(row)
    if not success:append_drift(fixture,rows,a)
    return fixture,dict(ok=True,case=case,fixture=fixture,noLiveNetwork=True,readiness=base['readiness'],initialRecords=base['initialRecords'],stages=rows,final=copy.deepcopy(rows[-1]['model']))


def native_blocks(row,blocks):
    m,g,p=row['model'],row['geometry'],row['proof']
    d=m['draftStore']['drafts'][0]
    for target in (d,p['journal']):target['blocks']=copy.deepcopy(blocks)
    p['expectedBlocks']=copy.deepcopy(blocks)
    g['editor'].update(blocks=copy.deepcopy(blocks),text=V['display'](blocks))
    g['native']['snapshot']['blocks']=copy.deepcopy(blocks)
    tokens=[v for b in blocks for sec in b['elements'] for v in sec['elements'] if v['type']!='text']
    def label(v):
        if v['type']=='channel':return LABELS[v['channel_id']]
        if v['type']=='usergroup':return LABELS[v['usergroup_id']]
        if v['type']=='user':return LABELS.get(v['user_id'],'@'+v['user_id'])
        return v['text']
    g['native']['tokens']=[dict(value=copy.deepcopy(v),label=label(v)) for v in tokens]


def popup(row,ids,sequence):
    m,g=row['model'],row['geometry'];gen=g['editor']['generation']
    values=[V['choice'](id) for id in ids];labels=[v['label'] for v in values]
    m['completion']=dict(sequence=sequence,revision=3,labels=labels,current=True,batchCurrent=True,settledBatch=True,started=True,consumed=False,friendly=False,values=values)
    g['completion']=dict(sequence=sequence,revision=3,scroll=dict(top=0,height=25,total=25),captureMode='interactive',open=True,
        rows=[dict(label=l,id=f'slack-list-channel-choice-{gen}-{sequence}-{j}',selected=str(j==0).lower(),type='button',role='option',visible=True,hit=True,fullyVisible=True,rect=BOX) for j,l in enumerate(labels)],
        status=None,popup=dict(visible=True,hit=True,rect=BOX))


def append_drift(fixture,rows,a):
    changes=[]
    for offset,stage in enumerate(('source-renamed','source-deleted')):
        row=copy.deepcopy(rows[-1]);row.update(stage=stage,capture=str(DIR/f'{fixture}-{stage}.png'))
        m,p,g=row['model'],row['proof'],row['geometry'];p.pop('bottomWindow',None)
        blocks=query(query(a,'@cancelled'),'@alex');native_blocks(row,blocks)
        source=m['completionSource']
        for method,args,raw in [('users/search',dict(query='alex',count=25,fuzz=1,enable_workspace_ranking=True,top_users=[],default_workspace='T1'),dict(ok=True,results=source_rows())),
                                ('usergroups/search',dict(query='alex',count=25,org_wide=True),dict(ok=True,results=[]))]:
            serial=len(source['requests'])+1
            source['requests'].append(dict(serial=serial,method=method,args=args));source['responses'].append(dict(serial=serial,disposition='response',raw=raw))
        source['completed']=7+offset
        popup(row,[SECOND,FIRST,RETIRED,SELF,G1,G2,G3],80+offset)
        if offset:
            m['completion']['labels'][4]='@alex-renamed';m['completion']['values'][4]['label']='@alex-renamed';g['completion']['rows'][4]['label']='@alex-renamed'
        before=copy.deepcopy(m);original=copy.deepcopy(g)
        raw=V['group_rows']();raw[0].update(handle='alex-renamed',date_update=102+offset,date_delete=123 if offset else 0)
        source['groups']=V['group_state'](raw)
        operation=dict(incoming=[raw[0]],rows=copy.deepcopy(source['groups']['rows']),membership=None)
        m['completion']=None
        g['completion']=dict(sequence=0,revision=0,scroll=None,captureMode='interactive',open=False,rows=[],status=None,popup=None)
        retired=copy.deepcopy(m)
        stale=dict(ok=True,accepted=False,before=copy.deepcopy(original['native']['snapshot']),after=copy.deepcopy(original['native']['snapshot']))
        changes.append(dict(stage=stage,id=G1,deleted=bool(offset),before=before,original=original,index=4,operation=operation,retired=retired,after=copy.deepcopy(retired),staleChoice=stale))
        p['sourceChanges']=copy.deepcopy(changes)
        p['inputs'].append(dict(stage=stage,kind='query',value=dict(ok=True,query='@alex',native=snap(blocks,30),sequence=70+offset,revision=3)))
        rows.append(row)


def restricted_sample(narrow=False):
    fixture,base=sample(narrow=narrow);original=base['stages'][0]['proof']['originalA'];rows=[]
    source=copy.deepcopy(base['stages'][1]['model']['completionSource'])
    pairs=[(q,r) for q,r in zip(source['requests'],source['responses']) if q['method']!='usergroups/search']
    source['requests']=[];source['responses']=[];source.update(completed=0,cancelled=0,held=0,releases=[])
    for serial,(q,r) in enumerate(pairs,1):
        q['serial']=r['serial']=serial;source['requests'].append(q);source['responses'].append(r)
    source=V['restricted_data'](source);source['groups']['selfRestricted']=True
    for i,stage in enumerate(('draft','restricted-results','member-inserted')):
        row=copy.deepcopy(base['stages'][[0,1,3][i]]);row.update(stage=stage,capture=str(DIR/f'{fixture}-{stage}.png'))
        m,p,g=row['model'],row['proof'],row['geometry'];p.pop('bottomWindow',None)
        m['completionSource']=copy.deepcopy(source if i else V['restricted_data'](base['stages'][0]['model']['completionSource']))
        m['completionSource']['groups']['selfRestricted']=True
        blocks=original if i==0 else query(original,'@alex') if i==1 else inserted(original,[SECOND])
        native_blocks(row,blocks)
        inputs=[dict(stage='restricted-results',kind='query',value=dict(ok=True,query='@alex',native=snap(query(original,'@alex')),sequence=0,revision=0))]
        if i==1:
            popup(row,[SECOND,FIRST,RETIRED,SELF],10)
            p['bottomWindow']=dict(model=copy.deepcopy(m),geometry=copy.deepcopy(g),capture=str(DIR/f'{fixture}-{stage}-bottom.png'))
        if i==2:
            inputs.append(dict(stage=stage,kind='choose',selected=SECOND,values=copy.deepcopy(rows[1]['model']['completion']['values']),value=dict(ok=True,label=LABELS[SECOND],keyboard=True,index=0,sequence=10,revision=3,before=snap(query(original,'@alex')))))
        if i==2:inputs[-1]['publication']=copy.deepcopy(rows[1]['model']['completion'])
        if i==1:
            q=m['completion'];p['settled']={'restricted-results':dict(completion=copy.deepcopy(q),native=dict(ok=True,ready=True,sequence=q['sequence'],revision=q['revision'],labels=q['labels'],status=''))}
        elif i==2:p['settled']=copy.deepcopy(rows[1]['proof']['settled'])
        p.update(inputs=inputs[:i],releases=[])
        rows.append(row)
    base.update(case='group-completion-restricted',stages=rows,final=copy.deepcopy(rows[-1]['model']))
    return fixture,base


class GroupCompletionTests(unittest.TestCase):
    def check(self,f,r):return V['validate'](f,r,DIR,png=False)
    def reject(self,change,stage=2,case='group-completion'):
        f,r=restricted_sample() if case=='group-completion-restricted' else sample(case)
        change(r['stages'][stage]);r['final']=copy.deepcopy(r['stages'][-1]['model'])
        with self.assertRaises((RuntimeError,KeyError,ValueError,TypeError,IndexError)):self.check(f,r)
    def test_all_cases_and_widths(self):
        for narrow in (False,True):
            for case in ('group-completion','group-completion-custody'):self.check(*sample(case,narrow))
            self.check(*restricted_sample(narrow))
    def test_reopened_cancelled_text_required(self):
        def remove(r):
            def walk(v):
                if isinstance(v,list):
                    v[:]=[x for x in v if not (isinstance(x,dict) and x.get('type')=='text' and x.get('text')==' @cancelled')]
                    for x in v:walk(x)
                elif isinstance(v,dict):
                    for x in v.values():walk(x)
            walk(r)
        self.reject(remove,8,'group-completion-custody')
    def test_drift_query_keeps_saved_cancelled(self):
        _,r=sample('group-completion-custody')
        for row in r['stages'][8:]:self.assertIn(' @cancelled @alex',V['display'](row['proof']['expectedBlocks']))
    def test_interim_empty_refused(self):self.reject(lambda r:r['model']['completion'].update(settledBatch=False),2,'group-completion-custody')
    def test_interim_restricted_refused(self):self.reject(lambda r:r['model']['completion'].update(settledBatch=False),1,'group-completion-restricted')
    def test_native_settlement_revision(self):self.reject(lambda r:r['proof']['settled']['query-retired']['native'].update(revision=1),2,'group-completion-custody')
    def test_error_settlement_required(self):self.reject(lambda r:r['proof']['settled'].pop('failure'),5,'group-completion-custody')
    def test_stale_batch_refused(self):self.reject(lambda r:r['model']['completion'].update(batchCurrent=False))
    def test_selection_publication_required(self):self.reject(lambda r:r['proof']['inputs'][-1]['publication'].update(batchCurrent=False),4)
    def test_selection_revision_bound(self):self.reject(lambda r:r['proof']['inputs'][-1]['publication'].update(revision=999),4)
    def test_duplicate_label_wrong_id(self):self.reject(lambda r:r['model']['completion']['values'][5]['value'].update(usergroup_id=G1))
    def test_deleted_group_excluded(self):self.reject(lambda r:r['model']['completion']['values'][6]['value'].update(usergroup_id=G4))
    def test_hidden_active_group_required(self):self.reject(lambda r:r['model']['completion']['values'].pop())
    def test_original_raw_required(self):self.reject(lambda r:r['model']['completionSource']['responses'][-1]['raw']['results'][0].pop('fixture_opaque'))
    def test_exact_group_wire(self):self.reject(lambda r:r['model']['completionSource']['requests'][-1]['args'].update(fuzz=1))
    def test_group_org_wide_observed(self):self.reject(lambda r:r['model']['completionSource']['requests'][-1]['args'].update(org_wide=False))
    def test_members_preserved(self):self.reject(lambda r:r['model']['completion']['values'].pop(0))
    def test_context_ranking_preserved(self):
        def change(r):
            a=r['model']['completion']['values'];a[0],a[1]=a[1],a[0]
        self.reject(change)
    def test_native_group_value_not_label(self):self.reject(lambda r:r['geometry']['native']['tokens'][-1]['value'].update(usergroup_id=G2),4)
    def test_native_group_label(self):self.reject(lambda r:r['geometry']['native']['tokens'][-1].update(label='@wrong'),4)
    def test_styled_opaque_preserved(self):self.reject(lambda r:r['proof']['expectedBlocks'][0]['elements'][0]['elements'][4].update(style={}),4)
    def test_journal_required(self):self.reject(lambda r:r['proof']['journal'].update(blocks=[]),4)
    def test_held_is_group_only(self):self.reject(lambda r:r['model']['completionSource']['requests'][-1].update(method='users/search'),1)
    def test_cancelled_response_not_delivered(self):self.reject(lambda r:r['model']['completionSource']['releases'][0].update(delivered=True,cancelledBeforeReply=False),3,'group-completion-custody')
    def test_offline_original_failure(self):self.reject(lambda r:r['model']['completionSource']['responses'][-1].update(raw=dict(ok=True,results=[])),5,'group-completion-custody')
    def test_deleted_source_version(self):self.reject(lambda r:r['proof']['sourceChanges'][-1]['operation']['incoming'][0].update(date_update=102),9,'group-completion-custody')
    def test_source_choice_rejected(self):self.reject(lambda r:r['proof']['sourceChanges'][-1]['staleChoice'].update(accepted=True),8,'group-completion-custody')
    def test_source_no_successor_draft(self):self.reject(lambda r:r['proof']['sourceChanges'][-1]['after']['draftStore']['drafts'][0].update(blocks=[]),8,'group-completion-custody')
    def test_restricted_group_request_refused(self):self.reject(lambda r:r['model']['completionSource']['requests'][-1].update(method='usergroups/search'),1,'group-completion-restricted')
    def test_restricted_source_observed(self):self.reject(lambda r:r['model']['completionSource']['groups'].update(selfRestricted=False),1,'group-completion-restricted')
    def test_restricted_still_inserts_member(self):self.reject(lambda r:r['proof']['inputs'][-1].update(selected=FIRST),2,'group-completion-restricted')
    def test_unseen_mixed_row_refused(self):
        def change(r):
            for g in (r['geometry'],r['proof']['bottomWindow']['geometry']):
                g['completion']['rows'][6].update(visible=False,hit=False,fullyVisible=False,rect=dict(left=10,top=40,right=210,bottom=65,width=200,height=25))
        self.reject(change)
    def test_no_http(self):self.reject(lambda r:r['model']['completionSource'].update(httpCapability=True))
    def test_no_extra_write(self):self.reject(lambda r:r['model']['composeLedger']['posts'].append(copy.deepcopy(r['model']['composeLedger']['posts'][0])),6)
    def test_dispatch(self):
        for case in V['STAGES']:self.assertEqual(V['C']['case_contract'](case)[0],V['STAGES'][case])
    def test_retained_custody_d_logs_and_original_pngs(self):
        base=Path(__file__).resolve().parents[1]/'tmp/list-comments-group-completion-20261019'
        paths=list((base/'custody-normal-d').glob('slack-list/slack-list.json'))+list((base/'custody-narrow-d').glob('slack-list-narrow/slack-list-narrow.json'))
        if len(paths)!=2:self.skipTest('Both retained D receipts are not installed')
        c=V['C']
        # These immutable originals predate the category-neutral copy. Keep the
        # current oracle strict; scope only this historical replay to its copy.
        legacy=runpy.run_path(str(Path(__file__).with_name('slack_list_comments_group_completion.py')))
        legacy['validate'].__globals__.update(
            FAILURE='People and group suggestions could not load. Edit your query to try again.',
            EMPTY='No matching people or groups')
        for path in paths:
            evidence=json.loads(path.read_text(encoding='utf-8'))
            log=SimpleNamespace(log=path.with_suffix('.log'))
            native=SimpleNamespace(receipts=lambda prefix:c['NativeFixture'].receipts(log,prefix))
            with patch.dict(c['validate_log'].__globals__,case_contract=lambda case: (legacy['STAGES'][case],legacy['validator'](case))):
                result=c['validate_log'](native,evidence['fixture'],Path(evidence['readyDirectory']),evidence['readiness'],evidence['expectedPngDimensions'],evidence['case'])
            self.assertEqual(result,evidence['terminalReceipts'][0])
            self.assertFalse(evidence['ok'])
            self.assertEqual(evidence['error'],'Source retirement changed rich draft')
    def test_capture_lifecycle(self):
        c=V['C']
        for case in V['STAGES']:
            f,r=restricted_sample() if case=='group-completion-restricted' else sample(case)
            captures=[]
            for stage in V['STAGES'][case]:
                captures.append(stage)
                if stage in ('local-held','refreshed','held','restricted-results'):captures.append(stage+'-bottom')
            edges=[dict(stage=s,edge=e) for s in captures for e in ('before','after')]
            packets={c['WAITING']:[r['readiness']],c['PREFIX']:[r],c['EDGE']:edges}
            native=SimpleNamespace(receipts=lambda prefix:packets[prefix])
            with patch.dict(c['validate_log'].__globals__,case_contract=lambda _: (V['STAGES'][case],lambda *args,**kw:r)):
                c['validate_log'](native,f,DIR,r['readiness'],None,case)
                for invalid in (edges[:-1],edges+[dict(stage='extra',edge='before')],edges[:3]+list(reversed(edges[3:5]))+edges[5:]):
                    packets[c['EDGE']]=invalid
                    with self.assertRaisesRegex(RuntimeError,'Capture lifecycle edges changed'):c['validate_log'](native,f,DIR,r['readiness'],None,case)
                packets[c['EDGE']]=edges


if __name__=='__main__':unittest.main()

