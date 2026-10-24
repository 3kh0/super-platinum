"""Original pre-middle packets, custody refusals and actual controller evidence."""
import base64
import copy
import json
from pathlib import Path
import subprocess
import unittest

import canvas_premiddle_wire as W
import canvas_premiddle_oracle as O
import canvas_premiddle_recovery_oracle as R
import canvas_split_input as I
import test_canvas_middle_typing_fixtures as M
import test_canvas_split_oracle as F

ROOT=Path(__file__).resolve().parent.parent
REF=ROOT/'tmp/canvas-preenter-middle-20261023/reference'
VECTORS=json.loads((REF/'bindings.json').read_text(encoding='utf-8'))['vectors']
SEEDS=json.loads((REF/'source-seeds.json').read_text(encoding='utf-8'))['seeds']
SESSION='0123456789ab'

def config(v):return W.config(v['corpus'],('restore' if v['childCadence']=='exact-revert' else v['childCadence']),'retained' if v['mode']=='retained-positive' else v['mode'],{'exact-revert':'revert','erase-retype':'retype'}.get(v['parentCadence'],v['parentCadence']))

def fixture(v):
    e=config(v);rows=F.source_rows(e.corpus);rows[1]=F.replace(rows[1],12,F.body(e.full_wire))
    seed=next(s for s in SEEDS if (s['mode'],s['corpus'])==(v['mode'],v['corpus']))
    if e.parent_mode!='bootstrap':rows[1]=F.replace(rows[1],38,F.replace(base64.b64decode(seed['canonicalSession']),4,8_000_000_000_000_000))
    rows[1]=F.replace(rows[1],1,e.parent)
    if e.parent_mode!='bootstrap':rows[1]=F.replace(rows[1],38,F.replace(W.field(W.wire(rows[1]),38,2),1,e.parent))
    source=F.inventory(rows);parent,child=map(W.S.raw,W.S.messages(W.decoded(v['wireBase64']),1))
    parent=F.replace(F.replace(parent,12,F.body(e.left_wire)),33,SESSION+'-0')
    child=F.replace(F.replace(F.replace(child,8,'aad:temp'),21,'aad:temp'),33,SESSION+'-0')
    parent=F.replace(F.replace(parent,1,e.parent),38,F.replace(W.field(W.wire(parent),38,2),1,e.parent))
    value=dict(kind='split',parent=e.parent,child=W.text(W.wire(child),1),session=SESSION,sequence=1,formTitle='Fixture title',httpFactoryExecuted=False,
               documentData=F.b64(F.f(14,F.TITLE)+F.f(1,parent)+F.f(1,child)))
    cp=dict(version=16,scope=W.S.SCOPE.copy(),thread='thread',document='document',target=e.parent,session=SESSION,sequence=1,draft=e.full_wire,
            action={'SplitPreEnter':dict(child=value['child'],position='aad:temp',offset_utf16=e.offset,enter_draft=W.rtml(e.live),parent_draft=e.left_wire,child_draft=e.tail_wire,
                                        pre_enter_updates=list(e.pre_reports),child_updates=list(e.child_reports))},
            sections=[list(base64.b64decode(row['raw'])) for row in sorted(source['sections'],key=lambda r:r['id'])],request=list(base64.b64decode(value['documentData'])))
    return e,source,value,[parent,child],cp

def journal(cp):return dict(scope=W.S.SCOPE.copy(),receipt=dict(operation='pre-middle-test',checkpoint=cp))

def parent_ack(outgoing,source,content,sequence,parent):
    row=M.authored(F.replace(outgoing,1,'paragraph'),F.replace(source,1,'paragraph'),content,sequence)
    return F.replace(row,1,parent)


def parent_continued(source,content,session,sequence,parent):
    value,row=M.continued(F.replace(source,1,'paragraph'),content,session,sequence)
    row=F.replace(row,1,parent)
    return dict(value,parent=parent,documentData=F.b64(F.f(14,F.TITLE)+F.f(1,row))),row


def recovery_packets(v,phase):
    e,source,_,_,cp=fixture(v);stages,terminal,_=M.R.packets(phase,e.corpus)
    labels=dict(splitMode='middle',corpus=e.corpus,typing=e.typing,parentMode=e.parent_mode,preEnter=e.pre_enter,parentId=e.parent)
    for stage in stages:
        stage.update(**labels,canonical=copy.deepcopy(source))
        if stage['stage']=='stale':
            row=stage['canonical']['sections'][-1];row.update(sequence=2000,raw=F.b64(F.replace(base64.b64decode(row['raw']),2,2000)))
        if stage['journal']['active']:stage['journal']['active']['checkpoint']=copy.deepcopy(cp)
        p=stage['preview'];p.update(originalChild=None,initialTail=dict(text=e.tail) if p['panel'] and e.child_reports else None,
                                   preEnter=dict(text=e.live,historyNotice=R.NOTICE) if p['panel'] else None)
        if p['panel']:
            p['split']=[dict(kind='original',text=e.full),dict(kind='parent',text=e.seed),dict(kind='child',text=e.updates[-1])]
            p['current']=[dict(id='title',deleted=False,text='Fixture title'),dict(id=e.parent,deleted=False,text=e.full),dict(id='neighbor',deleted=False,text='Another paragraph')]
        if stage['stage']=='review-pair':stage['reviewGeometry']=dict(ok=True,preEnter=True,text=e.live,historyNotice=R.NOTICE,visible=True,rect=[50,100,300,100],viewport=p['viewport'])
    terminal.update(**labels,canonical=copy.deepcopy(source),journal=copy.deepcopy(stages[-1]['journal']))
    return stages,terminal,cp


def native_receipt(v,scenario='success'):
    from types import SimpleNamespace
    cfg,initial,write,sent,cp=fixture(v);child=write['child'];corpus=cfg.corpus;typing=cfg.typing;parent_mode=cfg.parent_mode
    e=SimpleNamespace(left=cfg.seed,full=cfg.full,updates=cfg.updates,continued=cfg.continued,edited=cfg.parent_edited)
    captures=json.loads((ROOT/'tmp/canvas-preenter-middle-20261023/desktop/browser/captures/controller-cb312eb221fccff86e6fb970305aee14.json').read_text(encoding='utf-8'))
    real=next(c['capture']['split'] for c in captures if c['expected']['preEnter']==cfg.pre_enter)
    def witness(generation,corpus,typing):
        value=copy.deepcopy(real);value.update(generation=str(generation),visit='0');return value
    names=('typed-child','cancelled','saving','confirmed','child-editing','continued-saving','continued-confirmed','parent-editing','parent-saving','parent-confirmed') if scenario=='success' else ('typed-child','cancelled','refused','cancelled-stale') if scenario=='stale' else ('typed-child','cancelled','saving','uncertain')
    original=dict(preEnter=cfg.pre_enter,parentId=cfg.parent,mode='middle',corpus=corpus,typing=typing,parentMode=parent_mode,scenario=scenario,pending=False,preflights=[],writes=[],acknowledgements=[],readbacks=[],reservations=0,
                  fresh=initial,visible=initial,visit=0,read=1,stale=False,loading=False,capacityReason=None,operation=None)
    raws=[base64.b64decode(r['raw']) for r in initial['sections']]
    pair=[parent_ack(sent[0],raws[1],e.left,2000,cfg.parent),M.authored(sent[1],None,e.updates[-1],1000)]
    read0=F.inventory([raws[0],*pair,raws[2]])
    wchild,crow=M.continued(pair[1],e.continued,'c'*12);childrow=M.authored(crow,pair[1],e.continued,3000);read1=F.inventory([raws[0],pair[0],childrow,raws[2]])
    wparent,prow=parent_continued(pair[0],e.edited,'c'*12,2,cfg.parent);parentrow=parent_ack(prow,pair[0],e.edited,3000,cfg.parent);read2=F.inventory([raws[0],parentrow,childrow,raws[2]])
    empty=dict(scope=W.S.SCOPE.copy(),receipt=None);held=dict(scope=W.S.SCOPE.copy(),receipt=dict(operation='owned-operation',checkpoint=cp))
    cancelled,saved=witness(10,corpus,typing),witness(11,corpus,typing);stages=[]
    for i,name in enumerate(names):
        source=copy.deepcopy(original);pending=name in ('saving','continued-saving','parent-saving');uncertain=name=='uncertain';accepted=scenario=='success' and i>=3
        if i>=2:
            fresh=copy.deepcopy(initial)
            if scenario=='stale':fresh['sections'][1].update(sequence=3000,raw=F.b64(F.replace(raws[1],2,3000)))
            source.update(fresh=fresh,preflights=[dict(source=initial,fresh=fresh,outcome='Changed' if scenario=='stale' else 'prepared')])
            if scenario!='stale':source.update(writes=[write],reservations=1)
            if i>=3 and scenario!='stale':source['acknowledgements']=[F.acknowledgement(pair,partial=scenario=='partial')]
        if accepted:
            source.update(fresh=read0,visible=read0,readbacks=[read0])
            for j,start,w,prior,next_read,row in [(1,5,wchild,read0,read1,childrow),(2,8,wparent,read1,read2,parentrow)]:
                if i>=start:
                    source['writes'].append(w);source['preflights'].append(dict(source=prior,fresh=prior,outcome='prepared'))
                    if i>start:source['acknowledgements'].append(F.acknowledgement([row],continued=True));source['readbacks'].append(next_read);source.update(fresh=next_read,visible=next_read)
        source.update(pending=pending,stale=pending or uncertain,operation=dict(stage='Uncertain' if uncertain else 'Saving') if pending or uncertain else None)
        geo=dict(ok=True,noOverflow=True)
        isheld=name in ('typed-child','saving','refused','uncertain')
        parts=[e.left,e.updates[-1]] if isheld else [e.updates[-1]] if name=='child-editing' else [e.continued] if name=='continued-saving' else [e.left] if name=='parent-editing' else [e.edited] if name=='parent-saving' else None
        if parts is not None:
            owner=cancelled if name=='typed-child' else saved
            native=F.native(parts,int(owner['generation']) if isheld else 104 if i in (4,5) else 107,not(pending or uncertain),W.S.ERROR if name=='refused' else None)
            if isheld:
                t=owner['sequence'];native.update(blocks=t['after'],rawBlocks=t['rawAfter'],typing=None,placeholder=None,sequenceProof=t)
            else:native.update(rawBlocks=copy.deepcopy(native['blocks']),typing=None,placeholder=None,sequenceProof=None)
            geo['native']=native
        else:
            paragraphs=[dict(id=cfg.parent,text=e.edited if i>=9 else e.left if accepted else e.full)]
            if accepted:paragraphs.append(dict(id=child,text=e.continued if i>=6 else e.updates[-1]))
            paragraphs.append(dict(id='neighbor',text='Another paragraph'));geo.update(native=None,paragraphs=paragraphs)
        stages.append(dict(ok=True,noSession=True,noTransport=True,stage=name,ordinal=i,fixture=copy.deepcopy(source),geometry=geo,journal=copy.deepcopy(held if name=='saving' or uncertain else empty)))
    return dict(preEnter=cfg.pre_enter,parentId=cfg.parent,ok=True,noSession=True,noTransport=True,trustedNativeEnter=True,httpFactoryExecuted=False,mode='middle',corpus=corpus,typing=typing,parentMode=parent_mode,scenario=scenario,
            original=original,initialJournal=empty,cancelledWitness=cancelled,savedWitness=saved,duplicate=dict(ok=True,clicks=2),stages=stages,fixture=copy.deepcopy(stages[-1]['fixture']),layout=copy.deepcopy(stages[-1]['geometry']),finalJournal=copy.deepcopy(stages[-1]['journal']))


class PreMiddle(unittest.TestCase):
    def test_inert_recovery_live_history_notice_and_local_unlock_custody(self):
        chosen=[v for v in VECTORS if v['corpus']=='unicode' and v['parentCadence']!='untouched' and v['childCadence'] in ('none','erase')]
        self.assertEqual(len(chosen),18)
        for v in chosen:
            e=config(v)
            for phase in R.STAGES:
                stages,terminal,cp=recovery_packets(v,phase)
                R.validate(phase,stages,terminal,cp,e.corpus,e.typing,e.parent_mode,e.pre_enter)
                if phase=='review':
                    for key,wrong in [('text','lost'),('historyNotice','')]:
                        bad=copy.deepcopy(stages);next(s for s in bad if s['preview'].get('preEnter'))['preview']['preEnter'][key]=wrong
                        with self.assertRaises(RuntimeError):R.validate(phase,bad,terminal,cp,e.corpus,e.typing,e.parent_mode,e.pre_enter)
                    bad=copy.deepcopy(stages);next(s for s in bad if s['stage']=='review-pair')['reviewGeometry']['visible']=False
                    with self.assertRaises(RuntimeError):R.validate(phase,bad,terminal,cp,e.corpus,e.typing,e.parent_mode,e.pre_enter)

    def test_original_packets_histories_v16_and_whole_ack(self):
        vectors=[v for v in VECTORS if v['parentCadence']!='untouched'];self.assertEqual(len(vectors),72)
        for v in vectors:
            with self.subTest(vector=v['ordinal']):
                e,source,value,sent,cp=fixture(v);before=W.inventory(source,e);child,parent,tail,_=W.write(value,before[e.parent],e)
                W.journal(journal(cp),e,value,source)
                self.assertEqual(list(e.pre_reports),v['preReports']);self.assertEqual(list(e.child_reports),v['childUpdates'])
                parent_ack=M.authored(F.replace(sent[0],1,'paragraph'),F.replace(W.S.raw(before[e.parent]),1,'paragraph'),e.seed,2000)
                parent_ack=F.replace(parent_ack,1,e.parent)
                rows=[parent_ack,M.authored(sent[1],None,e.updates[-1],1000)]
                O.ack(F.acknowledgement(rows),[parent,tail],before,{e.parent:e.left_wire,child:e.reports[-1]},{e.parent:2000,child:1000},parent_id=e.parent)
                for incomplete in (rows[:1],rows[1:]):
                    with self.assertRaises(RuntimeError):O.ack(F.acknowledgement(incomplete),[parent,tail],before,{e.parent:e.left_wire,child:e.reports[-1]},{e.parent:2000,child:1000},parent_id=e.parent)

    def test_source_live_history_and_inert_schema_cannot_be_substituted(self):
        vectors=[v for v in VECTORS if v['parentCadence']!='untouched' and v['childCadence']=='exact-revert']
        self.assertEqual(len(vectors),18)
        for v in vectors:
            with self.subTest(vector=v['ordinal']):
                e,source,value,sent,cp=fixture(v);before=W.inventory(source,e)[e.parent];pc=W.field(W.wire(sent[0]),38,2);lineage=W.field(W.wire(pc),10,2)
                for tag,wrong in ((4,e.source_offset if e.pre_enter=='correction' else e.offset+1),(5,'[]'),(7,base64.b64decode(next(s for s in SEEDS if (s['mode'],s['corpus'])==(v['mode'],v['corpus']))['encodedDoc']))):
                    row=F.replace(sent[0],38,F.replace(pc,10,F.replace(lineage,tag,wrong)))
                    with self.assertRaises(RuntimeError):W.write(dict(value,documentData=F.b64(F.f(14,F.TITLE)+F.f(1,row)+F.f(1,sent[1]))),before,e)
                for key in ('version','draft','enter_draft','pre_enter_updates','child_updates','request','sections'):
                    bad=copy.deepcopy(cp)
                    if key=='version':bad[key]=8
                    elif key=='draft':bad[key]=W.rtml(e.live)+'wrong'
                    elif key in ('request','sections'):bad[key].pop()
                    elif key=='enter_draft':bad['action']['SplitPreEnter'][key]='wrong'
                    else:bad['action']['SplitPreEnter'][key]=[]
                    with self.assertRaises(RuntimeError):W.journal(journal(bad),e,value,source)

    def test_actual_browser_captures_and_complete_native_oracle(self):
        for pre,child,mode in [('correction','correction','bootstrap'),('exact-revert','exact-revert','retained-positive'),('erase-retype','erase','retained-zero')]:
            v=next(v for v in VECTORS if v['mode']==mode and v['corpus']=='unicode' and v['parentCadence']==pre and v['childCadence']==child)
            e=config(v)
            for scenario in ('success','stale','partial'):
                value=native_receipt(v,scenario);O.validate(value,e.corpus,e.typing,e.parent_mode,e.pre_enter)
                if scenario=='success':
                    for key,wrong in [('offset',e.offset+1),('revision',1),('before',F.blocks('forged'))]:
                        bad=copy.deepcopy(value);bad['savedWitness'][key]=wrong
                        with self.assertRaises(RuntimeError):O.validate(bad,e.corpus,e.typing,e.parent_mode,e.pre_enter)

    def test_requests_and_cli_refuse_unpinned_selectors_without_launch(self):
        for corpus in ('ascii','unicode'):
            for pre in ('correction','revert','retype'):
                for typing in ('none','correction','erase','restore'):
                    e=W.config(corpus,typing,'bootstrap',pre)
                    value=dict(ok=True,fixture='canvas-structure',nonce='a'*32,ordinal=0,corpus=corpus,mode='middle',typing=typing,preEnter=pre,parentMode='bootstrap',parentId=e.parent,seed=e.full,offset=e.source_offset,revision=0,fieldId='canvas-text-draft',generation='2',visit='0')
                    args=('canvas-structure','a'*32,0,corpus,'middle',typing,pre);I.validate_request(value,*args)
                    for key,wrong in [('seed',value['seed']+'x'),('offset',e.offset+1),('revision',1),('preEnter','none'),('nonce','b'*32),('ordinal',True)]:
                        with self.assertRaises(RuntimeError):I.validate_request(dict(value,**{key:wrong}),*args)
        for script in ('check-canvas-split-native.py','check-canvas-split-recovery-native.py'):
            common=['--fixture','canvas-structure' if script=='check-canvas-split-native.py' else 'canvas-local-review','--captures',str(ROOT/'tmp/must-not-create-premiddle-cli')]
            if script=='check-canvas-split-native.py':common+=['--scenario','success']
            for args in (['--mode','end','--pre-enter','correction'],['--mode','middle','--parent-mode','retained-zero']):
                run=subprocess.run(['python',str(ROOT/'scripts'/script),*common,*args],cwd=ROOT,text=True,encoding='utf-8',capture_output=True,timeout=20)
                self.assertEqual(run.returncode,2)
        self.assertFalse((ROOT/'tmp/must-not-create-premiddle-cli').exists())

if __name__=='__main__':unittest.main()
