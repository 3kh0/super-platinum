"""Independent original-runtime vectors, trusted captures and rejected forged custody."""
import base64
import copy
import json
from pathlib import Path
import subprocess
import unittest
import canvas_split_input as I
import canvas_second_middle_wire as W
import canvas_second_middle_oracle as O
import canvas_second_middle_recovery_oracle as R
import test_canvas_split_oracle as F
import test_canvas_middle_typing_fixtures as M

ROOT=Path(__file__).resolve().parent.parent
REF=ROOT/'tmp/canvas-second-middle-20261023/reference'
VECTORS=json.loads((REF/'bindings.json').read_text(encoding='utf-8'))['vectors']
SEEDS=json.loads((ROOT/'tmp/canvas-preenter-middle-20261023/reference/source-seeds.json').read_text(encoding='utf-8'))['seeds']
SESSION='0123456789ab'

def config(v):return W.config(v['corpus'],'retained' if v['mode']=='retained-positive' else v['mode'],v['secondTarget'],v['cadence'])

def fixture(v):
    e=config(v);source=F.source_rows(e.corpus);source[1]=F.replace(source[1],12,F.body(e.full_wire));source[1]=F.replace(source[1],1,e.parent)
    seed=next(s for s in SEEDS if (s['mode'],s['corpus'])==(v['mode'],v['corpus']))
    if e.parent_mode!='bootstrap':source[1]=F.replace(source[1],38,F.replace(F.replace(base64.b64decode(seed['canonicalSession']),4,8_000_000_000_000_000),1,e.parent))
    original=F.inventory(source);rows=list(map(W.S.raw,W.S.messages(W.decoded(v['wireBase64']),1)))
    for i,row in enumerate(rows):
        row=F.replace(row,33,SESSION+'-0')
        if i:row=F.replace(F.replace(row,8,e.positions[i]+':temp'),21,e.positions[i]+':temp')
        else:row=F.replace(F.replace(row,12,F.body(W.rtml(e.final[0]))),1,e.parent);row=F.replace(row,38,F.replace(W.field(W.wire(row),38,2),1,e.parent))
        rows[i]=row
    ids=[W.text(W.wire(row),1) for row in rows]
    value=dict(kind='second-middle',parent=e.parent,child=ids[1],secondChild=ids[2],members=[dict(id=ids[i],finalRtml=W.rtml(e.final[i])) for i in (1,2)],visualOrder=list(e.visual),session=SESSION,sequence=1,formTitle='Fixture title',httpFactoryExecuted=False,
               documentData=F.b64(F.f(14,F.TITLE)+b''.join(F.f(1,row) for row in rows)))
    cp=dict(version=18,scope=W.S.SCOPE.copy(),thread='thread',document='document',target=e.parent,session=SESSION,sequence=1,draft=e.full_wire,
            action={'SplitSequence':W.action(e,ids[1:])},sections=[list(base64.b64decode(row['raw'])) for row in sorted(original['sections'],key=lambda r:r['id'])],request=list(base64.b64decode(value['documentData'])))
    return e,original,value,rows,cp

def journal(cp):return dict(scope=W.S.SCOPE.copy(),receipt=dict(operation='second-middle-test',checkpoint=cp))

def captures():
    return json.loads((ROOT/'tmp/canvas-second-middle-20261023/desktop/browser/captures/controller-92f79c5088605436996611b7a23be3d5.json').read_text(encoding='utf-8'))

def authored(row,source,content,sequence,index,e):
    identity=W.text(W.wire(row),1)
    if index==0:
        out=M.authored(F.replace(row,1,'paragraph'),F.replace(source,1,'paragraph'),content,sequence)
        return F.replace(out,1,identity)
    out=M.authored(row,source,content,sequence)
    return F.replace(F.replace(out,8,e.positions[index]),21,e.positions[index])

def continued(row,content,sequence,index,e):
    identity=W.text(W.wire(row),1)
    if index==0:
        value,out=M.continued(F.replace(row,1,'paragraph'),content,'c'*12,sequence);out=F.replace(out,1,identity)
        return dict(value,parent=identity,documentData=F.b64(F.f(14,F.TITLE)+F.f(1,out))),out
    return M.continued(row,content,'c'*12,sequence)

def labels(e,recovery=False):return dict(corpus=e.corpus,parentMode=e.parent_mode,parentId=e.parent,typing='none',preEnter='none',middleParent='none',middleCadence='parent-first',secondMiddle=e.second_middle,secondMiddleCadence=e.second_cadence,**{'splitMode' if recovery else 'mode':'middle'})

def reader_geometry(e,children,index):
    can_delete=not(e.second_middle=='child' and index==0)
    controls=[]
    for i,action in enumerate(('edit','before','after','delete')):
        present=can_delete or action!='delete'
        controls.append(dict(action=action,count=1 if present else 0,enabled=present,visible=present,rect=[400+i*80,120,60,32] if present else None))
    return dict(viewport=[900,600],readerTarget=dict(ok=True,id=children[index],text=e.edited[index+1],canDelete=can_delete,visible=True,rect=[50,120,330,32],controls=controls))

def native_receipt(v,scenario='success'):
    e,initial,write,sent,cp=fixture(v);children=[write['child'],write['secondChild']];ids=[e.parent,*children]
    real=next(c['capture']['split'] for c in captures() if all(c['expected'][k]==val for k,val in dict(corpus=e.corpus,secondMiddle=e.second_middle,secondMiddleCadence=e.second_cadence).items()))
    cancelled=copy.deepcopy(real);cancelled.update(generation='10',visit='0');saved=copy.deepcopy(real);saved.update(generation='11',visit='0')
    names=(('second-split','cancelled','saving','confirmed')+tuple(f'member-{i}-{s}' for i in range(2) for s in ('editing','saving','confirmed'))+('parent-editing','parent-saving','parent-confirmed') if scenario=='success' else ('second-split','cancelled','refused','cancelled-stale') if scenario=='stale' else ('second-split','cancelled','saving','uncertain'))
    original=dict(**labels(e),scenario=scenario,pending=False,preflights=[],writes=[],acknowledgements=[],readbacks=[],reservations=0,fresh=initial,visible=initial,visit=0,read=1,stale=False,loading=False,capacityReason=None,operation=None)
    source=[base64.b64decode(r['raw']) for r in initial['sections']]
    current=[authored(row,source[1] if i==0 else None,e.final[i],2000 if i==0 else 1000,i,e) for i,row in enumerate(sent)]
    def inv():return F.inventory([source[0],*current,source[2]])
    reads=[inv()];acks=[current.copy()];writes=[write]
    for j,i in enumerate((1,2,0),1):
        w,row=continued(current[i],e.edited[i],j,i,e);writes.append(w);current[i]=authored(row,current[i],e.edited[i],3000,i,e)
        acks.append([current[i]]);reads.append(inv())
    empty=dict(scope=W.S.SCOPE.copy(),receipt=None);held=journal(cp);stages=[]
    for ordinal,name in enumerate(names):
        value=copy.deepcopy(original);pending=name=='saving' or name.endswith('-saving');uncertain=name=='uncertain';accepted=scenario=='success' and ordinal>=3
        if ordinal>=2:
            fresh=copy.deepcopy(initial)
            if scenario=='stale':fresh['sections'][1].update(sequence=3000,raw=F.b64(F.replace(source[1],2,3000)))
            value.update(fresh=fresh,preflights=[dict(source=initial,fresh=fresh,outcome='Changed' if scenario=='stale' else 'prepared')])
            if scenario!='stale':value.update(writes=[write],reservations=1)
            if ordinal>=3 and scenario!='stale':value['acknowledgements']=[F.acknowledgement(acks[0],partial=scenario=='partial')]
        if accepted:
            value.update(fresh=reads[0],visible=reads[0],readbacks=[reads[0]])
            for j,start in enumerate((5,8,11),1):
                if ordinal>=start:
                    value['writes'].append(writes[j]);value['preflights'].append(dict(source=reads[j-1],fresh=reads[j-1],outcome='prepared'))
                    if ordinal>start:value['acknowledgements'].append(F.acknowledgement(acks[j],continued=True));value['readbacks'].append(reads[j]);value.update(fresh=reads[j],visible=reads[j])
        value.update(pending=pending,stale=pending or uncertain,operation=dict(stage='Uncertain' if uncertain else 'Saving') if pending or uncertain else None)
        geometry=dict(ok=True,noOverflow=True);isheld=name in ('second-split','saving','refused','uncertain');editing=name.endswith('-editing') or name.endswith('-saving')
        if isheld or editing:
            i=0 if name.startswith('parent-') else int(name.split('-')[1])+1 if name.startswith('member-') else None
            parts=[e.final[j] for j in e.visual] if isheld else [e.edited[i] if pending else e.final[i]];owner=cancelled if name=='second-split' else saved
            generation=int(owner['generation']) if isheld else 104 if i==1 else 107 if i==2 else 110
            n=F.native(parts,generation,not(pending or uncertain),W.S.ERROR if name=='refused' else None)
            n.update(typing=None,placeholder=None,sequenceProof=owner['sequence'] if isheld else None,rawBlocks=copy.deepcopy(n['blocks']))
            if isheld:n.update(blocks=owner['sequence']['after'],rawBlocks=owner['sequence']['rawAfter'])
            geometry['native']=n
        else:
            done={i for i,start in ((1,5),(2,8),(0,11)) if ordinal>start}
            geometry.update(native=None,paragraphs=([dict(id=ids[i],text=e.edited[i] if i in done else e.final[i]) for i in e.visual] if accepted else [dict(id=e.parent,text=e.full)])+[dict(id='neighbor',text='Another paragraph')])
            if name in ('member-0-confirmed','member-1-confirmed'):geometry.update(reader_geometry(e,children,int(name.split('-')[1])))
        stages.append(dict(ok=True,noSession=True,noTransport=True,stage=name,ordinal=ordinal,fixture=value,geometry=geometry,journal=held if name=='saving' or uncertain else empty))
    return dict(**labels(e),ok=True,noSession=True,noTransport=True,trustedNativeEnter=True,httpFactoryExecuted=False,scenario=scenario,original=original,initialJournal=empty,cancelledWitness=cancelled,savedWitness=saved,duplicate=dict(ok=True,clicks=2),stages=stages,fixture=stages[-1]['fixture'],layout=stages[-1]['geometry'],finalJournal=stages[-1]['journal'])

SEMANTIC_CAPTURE_NAMES=('review-original','review-first-boundary','review-second-history','review-second-before','review-second-left','review-second-right',
                        'review-member-0','review-member-1','review-member-2')

def authored_semantic_geometry(e,name,viewport):
    records={
        'review-original':('original','Original paragraph', [e.full]),
        'review-first-boundary':('first-boundary','Paragraphs immediately after the first Enter',[e.left,e.tail]),
        'review-second-history':('history','Both paragraph splits and the edits between them remain part of this unresolved change.',[]),
        'review-second-before':('before','Paragraph before the second Enter',[e.second_before]),
        'review-second-left':('left','First part immediately after the second Enter',[e.second_left]),
        'review-second-right':('right','Second part immediately after the second Enter',[e.second_right])}
    result=dict(ok=True,secondMiddle=True,visible=True,visualIndex=None,creationIndex=None,memberKind=None,emptyLabel=None,
                rect=[50,120,300,90],clip=[20,100,viewport[0]-40,viewport[1]-120],viewport=viewport)
    if name in records:
        section,label,previews=records[name];return dict(result,section=section,previews=previews,text=label+''.join(previews))
    visual=int(name.rsplit('-',1)[1]);creation=(0,2,1)[visual] if e.second_middle=='parent' else visual
    text=e.final[creation];empty='Empty paragraph' if not text else None
    return dict(result,section='member',visualIndex=visual,creationIndex=creation,memberKind=['parent','child1','child2'][creation],
                previews=[text],text='Paragraph '+str(visual+1)+text+(empty or ''),emptyLabel=empty)

def recovery_packets(v,phase):
    e,source,_,_,cp=fixture(v);stages,terminal,_=M.R.packets(phase,e.corpus)
    if phase in ('review','storage-failure'):
        first=stages.pop(0)
        stages=[dict(copy.deepcopy(first),stage=name) for name in SEMANTIC_CAPTURE_NAMES]+stages
    for ordinal,stage in enumerate(stages):
        stage.update(**labels(e,True),ordinal=ordinal,canonical=copy.deepcopy(source))
        if stage['stage']=='stale':
            row=stage['canonical']['sections'][-1];row.update(sequence=2000,raw=F.b64(F.replace(base64.b64decode(row['raw']),2,2000)))
        if stage['journal']['active']:stage['journal']['active']['checkpoint']=copy.deepcopy(cp)
        p=stage['preview'];p.update(originalChild=None,initialTail=None,preEnter=None,parentHistory=None,secondMiddle=R.second_preview(e) if p['panel'] else None)
        if p['panel']:
            p['split']=[dict(kind='original',text=e.full)];p['missing']=['child1','child2'];p['warning']=' '.join(R.WARNINGS)
            p['current']=[dict(id='title',deleted=False,text='Fixture title'),dict(id=e.parent,deleted=False,text=e.full),dict(id='neighbor',deleted=False,text='Another paragraph')]
        if stage['stage'] in SEMANTIC_CAPTURE_NAMES:stage['reviewGeometry']=authored_semantic_geometry(e,stage['stage'],p['viewport'])
    terminal.update(**labels(e,True),canonical=copy.deepcopy(source),journal=copy.deepcopy(stages[-1]['journal']))
    return stages,terminal,cp

class SecondMiddleTests(unittest.TestCase):
    def test_recovery_semantics_require_exact_sections_and_scroll_clip(self):
        for v in VECTORS:
            e=config(v)
            for viewport in ([1400,900],[668,380]):
                for name in SEMANTIC_CAPTURE_NAMES:
                    value=authored_semantic_geometry(e,name,viewport);R.semantic_geometry(value,name,viewport,e)
                    for mutation in ('text','previews','emptyLabel','creationIndex','memberKind','section','visualIndex','clip','rect','nonfinite','viewport'):
                        bad=copy.deepcopy(value)
                        if mutation=='clip':bad['clip']=[20,180,viewport[0]-40,100] # Visible in window, clipped in actual reader.
                        elif mutation=='rect':bad['rect'][3]=viewport[1]
                        elif mutation=='nonfinite':bad['clip'][1]=float('nan')
                        elif mutation=='viewport':bad['viewport']=[viewport[0],viewport[1]+1]
                        elif mutation=='creationIndex':bad[mutation]=True if value[mutation]==1 else 1
                        elif mutation=='visualIndex':bad[mutation]=True if value[mutation]==1 else 1
                        else:bad[mutation]='forged'
                        with self.subTest(vector=v['ordinal'],viewport=viewport,name=name,mutation=mutation):
                            with self.assertRaises(RuntimeError):R.semantic_geometry(bad,name,viewport,e)

    def test_recovery_stage_schedule_requires_every_semantic_section(self):
        v=VECTORS[3];e=config(v)
        for phase in ('review','storage-failure'):
            stages,terminal,cp=recovery_packets(v,phase)
            self.assertEqual([s['stage'] for s in stages[:9]],list(SEMANTIC_CAPTURE_NAMES))
            for index in range(9):
                bad=copy.deepcopy(stages);bad.pop(index)
                with self.assertRaises(RuntimeError):R.validate(phase,bad,terminal,cp,e.corpus,e.parent_mode,e.second_middle,e.second_cadence)
            bad=copy.deepcopy(stages);bad[0],bad[1]=bad[1],bad[0]
            with self.assertRaises(RuntimeError):R.validate(phase,bad,terminal,cp,e.corpus,e.parent_mode,e.second_middle,e.second_cadence)
            bad=copy.deepcopy(stages);bad[0]['stage']='review-pair'
            with self.assertRaises(RuntimeError):R.validate(phase,bad,terminal,cp,e.corpus,e.parent_mode,e.second_middle,e.second_cadence)

    def test_reader_action_eligibility_is_bound_to_exact_lineage_owner(self):
        for v in VECTORS:
            e,_,write,_,_=fixture(v);children=[write['child'],write['secondChild']]
            for index in range(2):
                geometry=reader_geometry(e,children,index);O.reader_target(geometry,e,children,index)
                for mutation in ('delete','edit','before','after','target','can-delete','clipped','missing-record'):
                    bad=copy.deepcopy(geometry);target=bad['readerTarget']
                    if mutation=='delete':
                        c=target['controls'][-1];present=c['count']==0
                        c.update(count=int(present),enabled=present,visible=present,rect=[640,120,60,32] if present else None)
                    elif mutation in ('edit','before','after'):
                        c=next(c for c in target['controls'] if c['action']==mutation);c.update(count=0,enabled=False,visible=False,rect=None)
                    elif mutation=='target':target['id']=children[1-index]
                    elif mutation=='can-delete':target['canDelete']=not target['canDelete']
                    elif mutation=='clipped':target['controls'][0]['rect']=[890,120,60,32]
                    else:target['controls'].pop()
                    with self.subTest(vector=v['ordinal'],member=index,mutation=mutation):
                        with self.assertRaises(RuntimeError):O.reader_target(bad,e,children,index)

    def test_inventory_storage_order_is_distinct_from_verified_visual_positions(self):
        for v in VECTORS:
            e,initial,value,sent,_=fixture(v);ids=[e.parent,value['child'],value['secondChild']]
            source=[base64.b64decode(r['raw']) for r in initial['sections']]
            rows=[authored(row,source[1] if i==0 else None,e.final[i],2000 if i==0 else 1000,i,e) for i,row in enumerate(sent)]
            # Canonical storage follows creation order in the actual fixture.
            # A separately serialized inventory may be in positional order.
            read=F.inventory([source[0],*rows,source[2]])
            for order in ((0,1,2),e.visual,(2,0,1)):
                alternate=F.inventory([source[0],*[rows[i] for i in order],source[2]])
                self.assertEqual(W.inventory(alternate,e,initial,ids[1:]),W.inventory(read,e,initial,ids[1:]))
            for mutation in ('missing','duplicate','unknown','raw-id','swapped-positions','duplicate-position','wrong-position'):
                bad=copy.deepcopy(read)
                if mutation=='missing':bad['sections'].pop(2)
                elif mutation=='duplicate':bad['sections'][3]=copy.deepcopy(bad['sections'][2])
                elif mutation=='unknown':bad['sections'][3]['id']='unknown'
                elif mutation=='raw-id':bad['sections'][3]['raw']=F.b64(F.replace(rows[2],1,ids[1]))
                elif mutation=='swapped-positions':
                    bad['sections'][2]['raw']=F.b64(F.replace(rows[1],8,e.positions[2]))
                    bad['sections'][3]['raw']=F.b64(F.replace(rows[2],8,e.positions[1]))
                else:bad['sections'][3]['raw']=F.b64(F.replace(rows[2],8,e.positions[1] if mutation=='duplicate-position' else 'aaz'))
                with self.subTest(vector=v['ordinal'],mutation=mutation):
                    with self.assertRaises(RuntimeError):W.inventory(bad,e,initial,ids[1:])

    def test_all_three_ack_members_and_independent_visual_readback(self):
        for v in VECTORS:
            e,initial,value,sent,_=fixture(v);source=W.inventory(initial,e);ids=[e.parent,value['child'],value['secondChild']]
            raw=[base64.b64decode(r['raw']) for r in initial['sections']]
            rows=[authored(row,raw[1] if i==0 else None,e.final[i],2000 if i==0 else 1000,i,e) for i,row in enumerate(sent)]
            contents={ids[i]:W.rtml(e.final[i]) for i in range(3)};sequences={ids[i]:2000 if i==0 else 1000 for i in range(3)}
            O.ack(F.acknowledgement(rows),list(map(W.wire,sent)),source,contents,sequences,parent_id=e.parent)
            read=F.inventory([raw[0],*rows,raw[2]]);W.inventory(read,e,initial,ids[1:])
            for malformed in (rows[:1],rows[:2],[rows[0],rows[2]],rows[::-1],[rows[0],rows[1],rows[1]]):
                with self.assertRaises(RuntimeError):O.ack(F.acknowledgement(malformed),list(map(W.wire,sent)),source,contents,sequences,parent_id=e.parent)
            bad=copy.deepcopy(read);bad['sections'].pop(2)
            with self.assertRaises(RuntimeError):W.inventory(bad,e,initial,ids[1:])

    def test_python_request_and_cli_conflicts_stop_before_native(self):
        for v in VECTORS:
            e=config(v);q=dict(ok=True,fixture='canvas-structure',nonce='a'*32,ordinal=0,fieldId='canvas-text-draft',generation='4',visit='0',seed=e.full,offset=e.offset,revision=0,**labels(e))
            args=('canvas-structure','a'*32,0,e.corpus,'middle','none','none',e.parent_mode,'none','parent-first',e.second_middle,e.second_cadence)
            I.validate_request(q,*args)
            for key in ('secondMiddle','secondMiddleCadence','parentMode','parentId','seed','offset','ordinal'):
                with self.assertRaises(RuntimeError):I.validate_request(dict(q,**{key:None}),*args)
        for script in ('check-canvas-split-native.py','check-canvas-split-recovery-native.py'):
            baseline=['python',str(ROOT/'scripts'/script),'--fixture','canvas-structure' if 'recovery' not in script else 'canvas-local-review']
            if 'recovery' not in script:baseline+=['--scenario','success']
            else:baseline+=['--captures',str(ROOT/'tmp/canvas-second-middle-20261023/scripts/never-launched')]
            for extra in (['--second-middle','parent','--typing','correction'],['--second-middle','child','--pre-enter','revert'],['--second-middle','parent','--mode','end'],['--second-middle-cadence','edited']):
                r=subprocess.run(baseline+extra,cwd=ROOT,capture_output=True,text=True,timeout=10)
                self.assertEqual(r.returncode,2,r.stderr);self.assertIn('Second middle requires',r.stderr)

    def test_actual_captures_and_boundary_mutations(self):
        for c in captures():
            q=c['expected'];e=W.config(q['corpus'],q['parentMode'],q['secondMiddle'],q['secondMiddleCadence']);w=c['capture']['split'];O.witness(w,e)
            self.assertEqual(list(e.final),c['expectedFinalTexts'])
            self.assertEqual(list(e.visual),c['expectedVisualOrder'])
            second=next(i for i,x in enumerate(w['sequence']['events']) if x['kind']=='second-middle')
            for key in ('activeIndex','start','beforeText','beforeInputTrusted','revision'):
                forged=copy.deepcopy(w);forged['sequence']['events'][second][key]=None
                with self.assertRaises(RuntimeError):O.witness(forged,e)

    def test_native_full_three_partial_stale_and_later_each_row(self):
        for v in [VECTORS[i] for i in (0,3,10,13,20,23)]:
            e=config(v)
            for scenario in ('success','stale','partial'):
                value=native_receipt(v,scenario);O.validate(value,e.corpus,e.parent_mode,e.second_middle,e.second_cadence)
                forged=copy.deepcopy(value);forged['stages'][0]['geometry']['native']['sequenceProof']=None
                with self.assertRaises(RuntimeError):O.validate(forged,e.corpus,e.parent_mode,e.second_middle,e.second_cadence)

    def test_inert_recovery_all_phases_and_final_identity_refusals(self):
        for v in [VECTORS[i] for i in (0,3,10,13,20,23)]:
            e=config(v)
            for phase in R.STAGES:
                stages,terminal,cp=recovery_packets(v,phase);R.validate(phase,stages,terminal,cp,e.corpus,e.parent_mode,e.second_middle,e.second_cadence)
                if phase in ('review','storage-failure'):
                    for key in ('before','visualOrder','creation'):
                        bad=copy.deepcopy(stages);bad[0]['preview']['secondMiddle'][key]=None
                        with self.assertRaises(RuntimeError):R.validate(phase,bad,terminal,cp,e.corpus,e.parent_mode,e.second_middle,e.second_cadence)

    def test_original_24_complete_histories_lineages_and_durable_three(self):
        for v in VECTORS:
            with self.subTest(v=v['ordinal']):
                e,source,value,rows,cp=fixture(v);original=W.inventory(source,e);children,actual,infos=W.write(value,original[e.parent],e)
                W.journal(journal(cp),e,value,source)
                self.assertEqual(list(map(W.S.raw,actual)),rows)
                self.assertEqual([W.rtml(t) for t in e.final],[r['rtml'] for r in v['finalRows']])
                for key in ('second_enter_draft','second_offset_utf16','second_target','final_updates'):
                    bad=copy.deepcopy(cp);bad['action']['SplitSequence'][key]=None
                    with self.assertRaises(RuntimeError):W.journal(journal(bad),e,value,source)
                # Identical bootstrap heads do not license swapped child identities or visual-order packets.
                for packet in ([rows[0],rows[2],rows[1]],rows[:2]):
                    bad=dict(value,documentData=F.b64(F.f(14,F.TITLE)+b''.join(F.f(1,row) for row in packet)))
                    with self.assertRaises(RuntimeError):W.write(bad,original[e.parent],e)
                for row_index,records in enumerate(infos):
                    for line_index,_ in enumerate(records):
                        crdt=W.wire(W.field(W.wire(rows[row_index]),38,2));n=0;fields=[]
                        for f in crdt:
                            if f[0]==10:
                                if n==line_index:n+=1;continue
                                n+=1
                            fields.append(f[3])
                        altered=rows.copy();altered[row_index]=F.replace(altered[row_index],38,b''.join(fields))
                        bad=dict(value,documentData=F.b64(F.f(14,F.TITLE)+b''.join(F.f(1,row) for row in altered)))
                        with self.assertRaises(RuntimeError):W.write(bad,original[e.parent],e)

if __name__=='__main__':unittest.main()
