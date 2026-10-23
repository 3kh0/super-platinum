"""Portable own-edit title custody tests; never launch a native process."""
import copy
import hashlib
import json
from pathlib import Path
import runpy
import unittest
from unittest.mock import patch

ROOT=Path(__file__).resolve().parents[1]
RUN=runpy.run_path(str(ROOT/'scripts/check-list-own-edit-user-titles-native.py'))


class IsolatedHarnessTests(unittest.TestCase):
    def test_baseline_bytes_unchanged(self):
        pins=json.loads((ROOT/'tmp/list-own-edit-user-titles-20261022/oracle/baseline-pins.json').read_text())
        for item in pins['files']:
            with self.subTest(path=item['path']):
                self.assertEqual(hashlib.sha256((ROOT/item['path']).read_bytes()).hexdigest(),item['sha256'])
    def test_original_owned_pipeline_globals_are_isolated(self):
        base=RUN['harness']()
        for key in ('run_fixture','validate_log'):self.assertIs(base[key].__globals__,base)
        other=RUN['harness']();self.assertIsNot(base,other)
        self.assertEqual(base['case_contract']('own-edit')[0],other['case_contract']('own-edit')[0])
        self.assertEqual(base['PREFIX'],'slack-list-comments-compose-check: ')
        self.assertEqual(base['EDGE'],'slack-list-comments-compose-stage: ')
    def test_exact_cases_add_only_named_flag(self):
        cls=RUN['harness']()['NativeFixture'];parent=cls.__bases__[0]
        for case in RUN['CASES']:
            checks={'SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE':case,'other':'unchanged'};seen=[]
            with patch.object(parent,'__init__',lambda self,*a,**kw:seen.append((a,kw))):
                cls('slack-list',Path('log'),Path('binary'),checks)
            self.assertNotIn(RUN['FLAG'],checks)
            self.assertEqual(seen[0][0][-1],dict(checks,**{RUN['FLAG']:'1'}))
    def test_foreign_preconfigured_mode_refused_before_launch(self):
        cls=RUN['harness']()['NativeFixture'];parent=cls.__bases__[0]
        with patch.object(parent,'__init__') as init:
            for value in ({},{'SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE':'own-edit'},
                          {'SUPER_PLATINUM_LIST_COMMENTS_COMPOSE_CASE':RUN['CASES'][0],RUN['FLAG']:'0'}):
                with self.assertRaises(RuntimeError):cls('slack-list',Path('log'),Path('binary'),value)
            init.assert_not_called()




import slack_list_own_edit_user_titles as V
import slack_list_own_edit_titles_source as U
BASE=runpy.run_path(str(ROOT/'scripts/test_slack_list_comments_own_edit.py'))
BASE['sample'].__globals__['V']=V.O
DIR=Path('/private/own-edit')
UUID='00000000-0000-4000-8000-000000000001'


def source(case,model):
    raw=U.T.BASE.read_bytes().decode('utf-8').replace('"title":"Alex Canvas"',
        '"title":"Alex Canvas","title_blocks":'+json.dumps(U.T.REFERENCE.title_blocks(),ensure_ascii=False,separators=(',',':')))
    f=json.loads(raw)['files'][0]
    line=next(line.strip().rstrip(',') for line in raw.splitlines() if '"id":"FLISTFILE1"' in line)
    return dict(case=case,fileResponseBytes=raw,requests=[],policy=[dict(serial=1,method='users.prefs.get',raw=U.policy(case),
        account=model['account'],epoch=model['epoch'],team='T1',user=V.SELF)],
        files=[dict(serial=1,method='files.info',id=U.CANVAS,raw=dict(ok=True,file=f),responseText='{"ok":true,"file":'+line+'}')],
        pending=[],sourceRaw=U.member_source(),sourceReady=True,sourceObserved=True,sourcePresent=True,native=U.member_raw(),
        driver=dict(started=0,finished=0,active=0),knownDriver=dict(started=0,finished=0,active=0))


def observed(case,s,tokens,generation):
    f=json.loads(s['fileResponseBytes'])['files'][0]
    return dict(current=True,titles=dict(current=True,tokens=copy.deepcopy(tokens),required=[U.MEMBER],
        files=[dict(id=U.CANVAS,source=f,effective=U.T.title_projection(f),receipt=dict(ok=True,file=f)) for _ in tokens],
        users=dict(current=True,generation=generation,members=[dict(source=U.member_source(),effective=U.member_raw())]),
        labels={json.dumps(t,ensure_ascii=False,sort_keys=True):U.label(case) for t in tokens}),
        policy=dict(user=U.policy(case),enterprise=None,revision=0))


def admission(case,s,scope='list-edit:100:77',recovered=False,blocks=None):
    blocks=copy.deepcopy(blocks or V.received()['blocks'][:1]);tokens=U.tokens(blocks)
    job=dict(scope=scope,generation=int(scope.split(':')[-1]),ts=V.TS,revision=0,blocks=blocks,tokens=tokens,current=True,
        loading=False,status='ready',retryAt=0,error=None,
        observed=observed(case,s,tokens,'list-own-edit-title-'+scope+('' if recovered else '-file-title-users')))
    return dict(job=job,native=dict(snapshot=dict(blocks=copy.deepcopy(blocks)),tokens=[dict(value=t,label=U.label(case)) for t in tokens]))


def sample(case='own-edit-title-known',narrow=False):
    fixture,r=BASE['sample'](narrow);r['case']=case
    initial=r['stages'][0]['model']['comments'];s=source(case,initial)
    tokens=U.tokens(V.received()['blocks'][:1]);rect=BASE['B']['rect']()
    trans=dict(generation=1,running=False,rows=[dict(id=U.CANVAS,serial=i+1,ordinal=i,current=True,retryable=False,
        titleUsers=observed(case,s,[t],'list-transcript-title-'+str(i+1)+'-file-title-users')) for i,t in enumerate(tokens)])
    native=dict(ok=True,ts=V.TS,viewport=[480,800] if narrow else [1100,800],tokens=[dict(id=U.CANVAS,kind='canvas',text=U.label(case),
        renderedText=U.label(case),visible=True,hit=True,rect=rect,textRects=[dict(rect=rect,hit=True)]) for _ in tokens])
    entry=admission(case,s);entry2=admission(case,s,'list-edit:100:78');ds=V.durable_seed(initial)
    for i,row in enumerate(r['stages']):
        row['model'].update(titleSource=copy.deepcopy(s),transcriptTitles=copy.deepcopy(trans))
        row['proof'].update(initialTitleSource=copy.deepcopy(s),initialTranscript=copy.deepcopy(trans),initialTranscriptNative=copy.deepcopy(native),
            titleReady=copy.deepcopy([] if i==0 else [entry] if i<3 else [entry,entry2]),titleActions=[],releases=[])
        a=row['model']['edit']['active']
        if a:
            e=entry if i==1 else entry2
            a.update(scope=e['job']['scope'],generation=e['job']['generation'],nativeReady=True,titleUsers=copy.deepcopy(e['job']))
            a['titleUsers']['current']=False
        blocks=row['proof'].get('cancelledBlocks') if i==1 else row['proof'].get('editedBlocks') if i>=3 else []
        inv=dict(scope=row['journal']['scope'],ts=V.TS,revision=0 if i in (0,2) else 1,capture='' if i in (0,2) else UUID,
            seed=None if i in (0,2) else ds,blocks=copy.deepcopy(blocks),attempt=None,retry_at=0)
        if i>=3:
            inv['attempt']=dict(id=UUID,record=dict(version=1,seed=ds,blocks=blocks+V.received()['blocks'][1:],text=V.G['display'](blocks)),
                capture=UUID,phase='Pending' if i==3 else 'Confirmed',raw=None if i==3 else row['model']['edit']['attempts'][0]['receipt'],readback=None,retry_at=0)
        if i>=4:inv.update(seed=None,blocks=[],attempt=None)
        row['editInventory']=inv
        body=V.received()['blocks'][:1] if i<4 else row['proof']['editedBlocks']
        row['geometry']['row']['text']='\n'.join(''.join(U.label(case) if v.get('type')=='canvas' else v.get('text','') for v in section['elements']) for b in body for section in b['elements'])+' Preserved original context'
    r['final']=copy.deepcopy(r['stages'][-1]['model'])
    return fixture,r


class OriginalTitleTests(unittest.TestCase):
    def test_all_standard_widths(self):
        for case in V.CASES[:2]:
            for narrow in (False,True):
                with self.subTest(case=case,narrow=narrow):
                    f,r=sample(case,narrow);V.validator(case)(f,r,DIR,png=False)
    def test_unmodified_baseline_still_accepts(self):
        original=runpy.run_path(str(ROOT/'scripts/test_slack_list_comments_own_edit.py'))
        f,r=original['sample']();original['V']['validator']('own-edit')(f,r,DIR,png=False)
    def test_raw_own_file_occurrences_are_distinct(self):
        a,b=U.tokens(V.received()['blocks']);self.assertNotEqual(a,b)
        self.assertEqual(a['opaque_title_occurrence'],{'index':0})
        self.assertEqual(b['style'],{'italic':True})
    def test_ready_label_cannot_rewrite_wire(self):
        f,r=sample();entry=r['stages'][1]['proof']['titleReady'][0]
        entry['native']['tokens'][0]['value']['style']={}
        with self.assertRaises(RuntimeError):V.validator(r['case'])(f,r,DIR,png=False)
    def test_pre_input_current_required_but_after_input_false_allowed(self):
        f,r=sample();self.assertFalse(r['stages'][1]['model']['edit']['active']['titleUsers']['current'])
        V.validator(r['case'])(f,r,DIR,png=False)
        r['stages'][1]['proof']['titleReady'][0]['job']['current']=False
        with self.assertRaises(RuntimeError):V.validator(r['case'])(f,r,DIR,png=False)




def action(id):
    return dict(id=id,native=dict(ok=True,proof=dict(id=id,text=id,disabled=False,visible=True,hit=True,rect=BASE['B']['rect']())))


def member_requests(model,count,released,*,failed=False):
    rows=[]
    for serial in range(1,count+1):
        discovery=serial%2==1
        q=dict(serial=serial,lane='known',kind='discovery' if discovery else 'member',
            method='api.getFlannelHttpUrl' if discovery else 'users/info',
            args=dict(include_external_workspaces='true',_x_reason='client-needs-valid-flannel-url') if discovery else
                dict(ids=[U.MEMBER],check_interaction=True,include_profile_only_users=True),
            account=model['account'],epoch=model['epoch'],team='T1',released=serial<=released)
        if q['released']:
            q['response']=dict(error='offline') if failed and serial==2 else dict(raw=dict(ok=True,url='https://edgeapi.slack.com/cache/T1',ttl_seconds=300) if discovery else U.member_response(),keys=None)
            q['disposition']=dict(serial=serial,receiverRetired=False,delivered=True)
        rows.append(q)
    return rows


def recovery_sample(narrow=False):
    f,base=sample(narrow=narrow);case=V.CASES[2];rows=[];initial=base['stages'][0]['model']['comments']
    content=V.received()['blocks'][:1];content[0]['elements'][-1]['elements'][-1]['text']+=V.RECOVERED
    ack=dict(ok=True,channel=V.CHANNEL,ts=V.TS,message=V.update_row(content));readset=initial['ledger']['requests']
    ds=V.durable_seed(initial)
    for i,name in enumerate(V.STAGES[case]):
        template=0 if i==0 else 1 if i<=6 else i-4
        row=copy.deepcopy(base['stages'][template]);row['stage']=name;row['capture']=str(DIR/f'{f}-{name}.png')
        m=row['model']['comments'];e=row['model']['edit'];p=row['proof'];fs=row['model']['fixture'];g=row['geometry']
        count=V.EXT_READS[case][i]
        m['ledger']=dict(requests=[dict(copy.deepcopy(q),serial=n+1) for n,q in enumerate(readset*(count//4))],completed=count,cancelled=0,held=False,replies=count//4)
        s=source(case,m);qs=member_requests(m,V.COUNTS[case][i],V.RELEASED[case][i],failed=True)
        s.update(requests=qs,pending=[q for q in qs if not q['released']],sourceReady=i==0 or i>=6,
            sourceRaw=U.member_source(i>=6),knownDriver=dict(started=0 if i==0 else 1 if i<4 else 2,
                finished=0 if i<3 else 1 if i<6 else 2,active=int(i in (1,2,4,5))))
        row['model']['titleSource']=s
        p.update(initialTitleSource=copy.deepcopy(rows[0]['model']['titleSource']) if rows else copy.deepcopy(s),
            titleReady=[admission(case,s,recovered=True)] if i>=6 else [],releases=[],titleActions=[])
        p['releases']=[q['disposition'] for q in qs if q['released']]
        p['titleActions']=[action(id) for id in ((['slack-list-comment-edit-title-retry'] if i>=4 else [])+(['slack-list-comment-edit-save'] if i>=7 else [])+(['slack-list-comments-refresh'] if i>=9 else []))]
        p.pop('cancelledBlocks',None);p.pop('editedBlocks',None);p.pop('doubleSave',None);p.pop('release',None)
        if i>=6:p['editedBlocks']=copy.deepcopy(content)
        if i>=7:p['doubleSave']=dict(ok=True,disabled=True)
        if i>=8:p['release']=V.RELEASE
        fs.update(policy=[dict(request=q,response=r) for _ in range(V.EXT_POLICY[case][i]) for q,r in V.O['policy_cycle']()],
            prepared=[V.O['wire'](content)] if i>=7 else [],updates=[V.O['wire'](content)] if i>=7 else [],
            responses=[dict(disposition='accepted',raw=ack)] if i>=7 else [],futureRows=[ack['message']] if i>=7 else [],
            held=i==7,releases=[V.RELEASE] if i>=8 else [],published=i==9,publishedRow=ack['message'] if i==9 else None)
        if i>=7:e['attempts']=[dict(id=123,ts=V.TS,blocks=content+V.received()['blocks'][1:],phase='sent' if i==7 else 'confirmed',receipt=None if i==7 else ack,retryAt=0)]
        if 1<=i<=7:
            a=e['active'];a.update(scope='list-edit:100:77',generation=77,blocks=copy.deepcopy(content if i>=6 else V.received()['blocks'][:1]),
                revision=1 if i>=6 else 0,nativeReady=i>=6,busy=i==7)
            job=admission(case,s,recovered=True)['job'];job['current']=i<6
            if i<6:job.update(current=True,status='failed' if i==3 else 'loading',loading=i!=3,error='Title details could not load. Your saved edit and reply are retained.' if i==3 else None,observed=None)
            a['titleUsers']=job
        if i in range(1,6):
            ids=['slack-list-comment-edit-cancel','slack-list-comment-edit-save']+(['slack-list-comment-edit-title-retry'] if i==3 else [])
            g=dict(ok=True,editing=False,editorAbsent=True,scope='list-edit:100:77',generation='77',viewport=g['viewport'],
                reply=dict(blocks=p['replyBlocks'],sameNode=True,hidden=True,inert=True),
                controls=[dict(id=id,visible=True,hit=True,disabled=id.endswith('-save'),rect=BASE['B']['rect']()) for id in ids],
                status=dict(id='slack-list-comment-edit-title-status',visible=True,text='Title details could not load. Your saved edit and reply are retained.' if i==3 else 'Loading title details...',rect=BASE['B']['rect']()))
            row['geometry']=g
        elif i>=6:
            if g.get('edit'):g['edit']['blocks']=content
            g['row']['text']='\n'.join(''.join(U.label(case) if v.get('type')=='canvas' else v.get('text','') for v in sec['elements']) for b in (content if i>=8 else V.received()['blocks'][:1]) for sec in b['elements'])+' Preserved original context'
        if i>=8:
            m['comments']['pageMessages']=[ack['message'] if x.get('ts')==V.TS else x for x in m['comments']['pageMessages']]
        if i==9:m['comments']['page']['messages']=[ack['message'] if x.get('ts')==V.TS else x for x in m['comments']['page']['messages']]
        blocks=[] if i==0 else content if i>=6 else V.received()['blocks'][:1]
        inv=dict(scope=row['journal']['scope'],ts=V.TS,revision=0 if i==0 else 1,capture='' if i==0 else UUID,
            seed=None if i==0 else ds,blocks=copy.deepcopy(blocks),attempt=None,retry_at=0)
        if i>=7:inv['attempt']=dict(id=UUID,record=dict(version=1,seed=ds,blocks=content+V.received()['blocks'][1:],text=V.G['display'](content)),capture=UUID,phase='Pending' if i==7 else 'Confirmed',raw=None if i==7 else ack,readback=None,retry_at=0)
        if i>=8:inv.update(seed=None,blocks=[],attempt=None)
        row['editInventory']=inv;rows.append(row)
    base.update(case=case,stages=rows,final=copy.deepcopy(rows[-1]['model']))
    return f,base


class RecoveryTitleTests(unittest.TestCase):
    def test_complete_recovery_both_widths(self):
        for narrow in (False,True):
            f,r=recovery_sample(narrow);V.validator(r['case'])(f,r,DIR,png=False)
    def test_known_factory_never_uses_absent_updated_ids(self):
        f,r=recovery_sample();r['stages'][2]['model']['titleSource']['requests'][1]['args']={'updated_ids':{U.MEMBER:0},'check_interaction':True,'include_profile_only_users':True}
        with self.assertRaisesRegex(RuntimeError,'factory'):V.validator(r['case'])(f,r,DIR,png=False)
    def test_failed_premount_never_grants_native_ready(self):
        f,r=recovery_sample();r['stages'][3]['model']['edit']['active']['nativeReady']=True
        with self.assertRaises(RuntimeError):V.validator(r['case'])(f,r,DIR,png=False)




def held_sample(narrow=False):
    f,base=sample(narrow=narrow);case=V.CASES[3];rows=[];initial=base['stages'][0]['model']['comments'];readset=initial['ledger']['requests']
    content=V.received()['blocks'][:1];content[0]['elements'][-1]['elements'][-1]['text']+=V.DURABLE
    ds=V.durable_seed(initial);initialsource=source(case,initial);entry=admission(case,initialsource)
    fresh=admission(case,initialsource,'list-edit:200:99',recovered=True)
    invalid=copy.deepcopy(initialsource);invalid['sourceReady']=False
    detached=dict(ok=True,editorAbsent=True,originalDetached=True,originalRunning=False,commentsPresent=False,recordPresent=True)
    replacement=None;durable=None;held=None
    for i,name in enumerate(V.STAGES[case]):
        row=copy.deepcopy(base['stages'][1 if i in (1,3,4,7,8,9) else 0]);row.update(stage=name,capture=str(DIR/f'{f}-{name}.png'))
        m=row['model']['comments'];e=row['model']['edit'];p=row['proof'];fs=row['model']['fixture'];g=row['geometry']
        for k in ('cancelledBlocks','editedBlocks','doubleSave','release','runtimeForgotten'):p.pop(k,None)
        if i>=1:p['editedBlocks']=copy.deepcopy(content)
        if i>=3:m['comments']['generation']=200
        count=V.EXT_READS[case][i];m['ledger']=dict(requests=[dict(copy.deepcopy(q),serial=n+1) for n,q in enumerate(readset*(count//4))],completed=count,cancelled=0,held=False,replies=count//4)
        fs.update(policy=[dict(request=q,response=r) for _ in range(V.EXT_POLICY[case][i]) for q,r in V.O['policy_cycle']()],
            prepared=[],updates=[],responses=[],futureRows=[],held=False,releases=[],published=False,publishedRow=None)
        sourceval=copy.deepcopy(initialsource);qs=member_requests(m,V.COUNTS[case][i],V.RELEASED[case][i])
        sourceval.update(requests=qs,pending=[q for q in qs if not q['released']],sourceReady=i<3 or i>=9,
            sourceRaw=U.member_source(i>=9),knownDriver=dict(started=0 if i<3 else 1 if i<7 else 2,
                finished=0 if i<5 else 1 if i<9 else 2,active=int(i in (3,4,7,8))))
        row['model']['titleSource']=sourceval
        p.update(initialTitleSource=copy.deepcopy(initialsource),titleReady=copy.deepcopy([] if i==0 else [entry] if i<9 else [entry,fresh]),titleActions=[],releases=[])
        p['releases']=[q['disposition'] for q in qs if q['released']]
        p['titleActions']=[action(id) for id in ((['slack-list-record-comments'] if i>=3 else [])+(['slack-list-comment-edit-cancel'] if i>=5 else [])+(['slack-list-comment-edit-cancel'] if i>=10 else []))]
        if i>=3:p['invalidated']=copy.deepcopy(invalid)
        active=i in (1,3,4,7,8,9)
        if active:
            scope='list-edit:100:77' if i==1 else 'list-edit:200:88' if i<7 else 'list-edit:200:99'
            blocks=content if i in (1,3,4) else V.received()['blocks'][:1]
            a=e['active'];a.update(scope=scope,generation=int(scope.split(':')[-1]),blocks=copy.deepcopy(blocks),revision=1 if i in (1,3,4) else 0,
                nativeReady=i in (1,9),busy=False,attempt=None)
            job=copy.deepcopy(entry['job'] if i==1 else fresh['job'] if i==9 else admission(case,sourceval,scope,blocks=blocks)['job'])
            if i==1:job['current']=False
            elif i!=9:job.update(status='loading',loading=True,observed=None,revision=a['revision'])
            a['titleUsers']=job
            if i==4:held=copy.deepcopy(a)
        else:e['active']=None
        e['attempts']=[]
        if i in (3,4,7,8):
            a=e['active'];g=dict(ok=True,editing=False,editorAbsent=True,scope=a['scope'],generation=str(a['generation']),viewport=g['viewport'],
                reply=dict(blocks=p['replyBlocks'],sameNode=True,hidden=True,inert=True),
                controls=[dict(id='slack-list-comment-edit-'+id,visible=True,hit=True,disabled=id=='save',rect=BASE['B']['rect']()) for id in ('cancel','save')],
                status=dict(id='slack-list-comment-edit-title-status',visible=True,text='Loading title details...',rect=BASE['B']['rect']()))
        elif i==2:g=detached
        else:
            if i in (1,9):g['edit']['blocks']=copy.deepcopy(e['active']['blocks'])
            g['row']['text']='\n'.join(''.join(U.label(case) if v.get('type')=='canvas' else v.get('text','') for v in sec['elements']) for b in V.received()['blocks'][:1] for sec in b['elements'])+' Preserved original context'
        row['geometry']=g
        blocks=content if i in (1,2,3,4) else V.received()['blocks'][:1] if i in (7,8,9) else []
        inv=dict(scope=row['journal']['scope'],ts=V.TS,revision=1 if blocks else 0,capture=UUID if blocks else '',seed=ds if blocks else None,blocks=copy.deepcopy(blocks),attempt=None,retry_at=0)
        row['editInventory']=inv
        if i==1:durable=copy.deepcopy(inv)
        if i>=1:p['durableBeforeRetirement']=copy.deepcopy(durable)
        if i==2:
            replacement=dict(kind='synthetic-list-visit-replacement',beforeVisit=1,afterVisit=2,account=m['account'],epoch=m['epoch'],team='T1',user=V.SELF,
                before=copy.deepcopy(rows[1]['model']['edit']),after=copy.deepcopy(e),oldCurrent=False,oldVisible=False,oldRetained=True)
        if i>=2:p.update(visitReplacement=copy.deepcopy(replacement),detachedFinished=dict(current=False,visible=False,retained=False,geometry=detached))
        if i>=4:p['heldOriginal']=copy.deepcopy(held)
        if i>=9:p['freshSeed']=copy.deepcopy(e['active'] if i==9 else rows[9]['model']['edit']['active'])
        rows.append(row)
    base.update(case=case,stages=rows,final=copy.deepcopy(rows[-1]['model']))
    return f,base


class HeldTitleTests(unittest.TestCase):
    def test_complete_held_custody_both_widths(self):
        for narrow in (False,True):
            f,r=held_sample(narrow);V.validator(r['case'])(f,r,DIR,png=False)
    def test_cancel_cannot_repopulate_modified_durable_edit(self):
        f,r=held_sample();r['stages'][6]['editInventory']['blocks']=r['stages'][1]['proof']['editedBlocks']
        with self.assertRaises(RuntimeError):V.validator(r['case'])(f,r,DIR,png=False)
    def test_retirement_cannot_claim_old_owner_current(self):
        f,r=held_sample();r['stages'][-1]['proof']['detachedFinished']['current']=True
        with self.assertRaises(RuntimeError):V.validator(r['case'])(f,r,DIR,png=False)
    def test_fresh_edit_cannot_restore_cancelled_modified_blocks(self):
        f,r=held_sample();r['stages'][9]['model']['edit']['active']['blocks']=r['stages'][1]['proof']['editedBlocks']
        with self.assertRaises(RuntimeError):V.validator(r['case'])(f,r,DIR,png=False)




class AdversarialCustodyTests(unittest.TestCase):
    pass

def negative(name,factory,index,mutate,match=None):
    def test(self):
        f,r=factory();mutate(r['stages'][index]);r['final']=copy.deepcopy(r['stages'][-1]['model'])
        with self.assertRaisesRegex(RuntimeError,match or '.'):
            V.validator(r['case'])(f,r,DIR,png=False)
    setattr(AdversarialCustodyTests,'test_'+name,test)

negative('source_wire_file_text',sample,1,lambda r:r['proof']['cancelledBlocks'][0]['elements'][0]['elements'][2].update(text='Borrowed label'))
negative('source_wire_context',sample,3,lambda r:r['model']['fixture']['prepared'][0]['args'].update(blocks='[]'))
negative('native_label_wrong',sample,1,lambda r:r['proof']['titleReady'][0]['native']['tokens'][0].update(label='Alex Canvas'))
negative('native_token_opaque_missing',sample,1,lambda r:r['proof']['titleReady'][0]['native']['tokens'][0]['value'].pop('opaque_title_occurrence'))
negative('native_token_duplicate',sample,1,lambda r:r['proof']['titleReady'][0]['native']['tokens'].append(copy.deepcopy(r['proof']['titleReady'][0]['native']['tokens'][0])))
negative('query_generation_borrowed',sample,1,lambda r:r['proof']['titleReady'][0]['job']['observed']['titles']['users'].update(generation='list-comment-member-query-file-title-users'))
negative('wrong_independent_policy',sample,1,lambda r:r['proof']['titleReady'][0]['job']['observed']['policy']['user']['prefs'].update(hidden_users='UMISSING01'))
negative('title_member_id',sample,1,lambda r:r['proof']['titleReady'][0]['job']['observed']['titles']['users']['members'][0]['source'].update(id='UFOREIGN'))
negative('title_file_query_receipt',sample,1,lambda r:r['proof']['titleReady'][0]['job']['observed']['titles']['files'][0].update(receipt=json.loads(r['model']['titleSource']['fileResponseBytes'])))
negative('title_file_share_authority',sample,1,lambda r:r['proof']['titleReady'][0]['job']['observed']['titles']['files'][0]['effective'].update(channels=['CFOREIGN']))
negative('metadata_raw_rewrite',sample,0,lambda r:r['model']['titleSource']['files'][0]['raw']['file'].update(title='Changed'))
negative('metadata_duplicate_fetch',sample,1,lambda r:r['model']['titleSource']['files'].append(copy.deepcopy(r['model']['titleSource']['files'][0])))
negative('metadata_owner_policy',sample,0,lambda r:r['model']['titleSource']['policy'][0].update(epoch=999))
negative('ready_display_raw_substitution',sample,0,lambda r:r['model']['titleSource'].update(sourceRaw=U.member_raw()))
negative('reply_sqlite_changed',sample,1,lambda r:r['journal']['blocks'].append({'type':'text','text':'lost'}))
negative('edit_sqlite_scope',sample,1,lambda r:r['editInventory']['scope'].update(record='R_FOREIGN'))
negative('edit_sqlite_seed',sample,1,lambda r:r['editInventory']['seed']['target'].update(channel='CFOREIGN'))
negative('edit_sqlite_attempt_early',sample,1,lambda r:r['editInventory'].update(attempt={}))
negative('edit_sqlite_pending_fake_confirmed',sample,3,lambda r:r['editInventory']['attempt'].update(phase='Confirmed'))
negative('save_wrong_return_ack',sample,4,lambda r:r['model']['edit']['attempts'][0]['receipt'].update(ts='1781524801.999999'))
negative('save_held_double_dispatch',sample,3,lambda r:r['model']['fixture']['updates'].append(copy.deepcopy(r['model']['fixture']['updates'][0])))
negative('recovery_early_member_adoption',recovery_sample,2,lambda r:r['model']['titleSource'].update(sourceReady=True))
negative('recovery_absent_lane',recovery_sample,2,lambda r:r['model']['titleSource']['requests'][1].update(lane='absent'))
negative('recovery_wrong_owner',recovery_sample,2,lambda r:r['model']['titleSource']['requests'][1].update(account='foreign'))
negative('recovery_fabricated_response',recovery_sample,3,lambda r:r['model']['titleSource']['requests'][1].update(response={'raw':U.member_response(),'keys':None}))
negative('recovery_disabled_retry',recovery_sample,4,lambda r:r['proof']['titleActions'][0]['native']['proof'].update(disabled=True))
negative('recovery_hidden_input',recovery_sample,2,lambda r:r['geometry'].update(editorAbsent=False))
negative('recovery_new_file_read',recovery_sample,4,lambda r:r['model']['titleSource']['files'].append(copy.deepcopy(r['model']['titleSource']['files'][0])))
negative('recovery_lost_original_raw',recovery_sample,3,lambda r:r['model']['titleSource'].update(sourceRaw=None))
negative('recovery_wrong_factory_ids',recovery_sample,2,lambda r:r['model']['titleSource']['requests'][1]['args'].update(ids=['UOTHER']))
negative('held_wrong_visit_owner',held_sample,-1,lambda r:r['proof']['visitReplacement'].update(account='foreign'))
negative('held_same_visit',held_sample,-1,lambda r:r['proof']['visitReplacement'].update(afterVisit=r['proof']['visitReplacement']['beforeVisit']))
negative('held_detached_native_running',held_sample,2,lambda r:r['geometry'].update(originalRunning=True))
negative('held_retained_capture_unfinished',held_sample,-1,lambda r:r['proof']['detachedFinished'].update(retained=True))
negative('held_late_ready',held_sample,6,lambda r:r['model']['titleSource'].update(sourceReady=True))
negative('held_late_raw_mutation',held_sample,6,lambda r:r['model']['titleSource']['sourceRaw'].update(is_stranger=False))
negative('held_fresh_scope_reused',held_sample,9,lambda r:r['model']['edit']['active'].update(scope=r['proof']['heldOriginal']['scope']))
negative('held_reply_consumed',held_sample,5,lambda r:r['model']['comments']['draftStore']['drafts'][0].update(blocks=[]))
negative('held_cancel_not_original',held_sample,5,lambda r:r['proof']['titleActions'][-1]['native']['proof'].update(id='slack-list-comment-send'))
negative('held_release_fabricated',held_sample,6,lambda r:r['proof']['releases'][-1].update(delivered=False))

negative('canvas_token_cannot_be_generic_file',sample,1,lambda r:r['proof']['cancelledBlocks'][0]['elements'][0]['elements'][2].update(type='file'))
negative('native_canvas_kind_cannot_be_generic_file',sample,1,lambda r:r['proof']['titleReady'][0]['native']['tokens'][0]['value'].update(type='file'))
negative('title_canvas_dependency_cannot_be_generic_file',sample,1,lambda r:r['proof']['titleReady'][0]['job']['tokens'][0].update(type='file'))


class UnicodeAppendTests(unittest.TestCase):
    def test_exact_source_codepoints(self):
        self.assertEqual(V.DURABLE, ' Durable title edit B 日🦊')
        self.assertEqual(V.RECOVERED, ' Recovered edit B 日🦊')
        self.assertEqual([ord(c) for c in V.DURABLE[-2:]], [0x65e5, 0x1f98a])
    def test_questionmark_substitution_is_refused(self):
        f,r=held_sample()
        r['stages'][1]['proof']['editedBlocks'][0]['elements'][-1]['elements'][-1]['text']='Second editable line. Durable title edit B ???'
        with self.assertRaisesRegex(RuntimeError,'Original text/style'):
            V.validator(r['case'])(f,r,DIR,png=False)
    def test_original_held_receipt_exact_semantics_when_retained(self):
        p=ROOT/'tmp/list-own-edit-user-titles-20261022/integration/held-normal-b/slack-list.json'
        if not p.is_file():self.skipTest('Original closed diagnostic is not present')
        original=json.loads(p.read_text());self.assertIs(original['ok'],False)
        t=original['terminalReceipts'][0]
        V.validator(t['case'])(t['fixture'],t,Path(t['stages'][0]['capture']).parent,png=False)

if __name__=='__main__':unittest.main()

