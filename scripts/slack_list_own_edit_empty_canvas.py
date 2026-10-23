"""Original own-edit custody with explicitly owned empty Canvas presentation."""
import copy
import inspect
import json
import runpy
from pathlib import Path
import slack_list_own_edit_empty_canvas_source as S

LIFECYCLE='own-edit-empty-lifecycle'
RECOVERY='own-edit-empty-recovery'
STAGES={LIFECYCLE:('draft','editing','cancelled','saving','saved','readback','reopened'),
 RECOVERY:('draft','editing','visit-retired','restored-failed','retry-held','cancelled','late','fresh-held','fresh-edit','final-cancel')}
BASE=runpy.run_path(str(Path(__file__).with_name('slack_list_own_edit_user_titles.py')))
U=BASE['validate'].__globals__
# Reuse all owner/seed/block/journal/native/update invariants in isolated globals.
# Only exact named cases/stage predicates differ; incoming receipts are untouched.
U.update(CASES=(LIFECYCLE,'unused-hidden','unused-member-recovery',RECOVERY),STAGES=STAGES,
    PENDING={'restored-failed','retry-held','fresh-held'},
    EXT_READS={RECOVERY:(4,8,8,12,12,12,12,16,16,16)},
    EXT_POLICY={RECOVERY:(0,1,1,1,1,1,1,2,2,2)})
standard=inspect.getsource(U['validate_standard'])
assert standard.count("('own-edit-title-known','own-edit-title-hidden')")==1
standard=standard.replace("('own-edit-title-known','own-edit-title-hidden')",repr((LIFECYCLE,)))
exec(compile(standard,'<isolated original own-edit standard>','exec'),U)
extended=inspect.getsource(U['validate_extended'])
assert extended.count("('editing','saving','restored-discovery','restored-member')")==1
extended=extended.replace("('editing','saving','restored-discovery','restored-member')","('editing','saving','restored-failed','retry-held')")
extended=extended.replace("name=='failure'","name=='restored-failed'")
exec(compile(extended,'<isolated original own-edit recovery>','exec'),U)
require,received,tokens=S.require,U['received'],S.tokens
N,G,R,C,READ,H,DISK=(U[k] for k in ('N','G','R','C','READ','H','S'))
TS,SELF,CHANGE,DURABLE=(U[k] for k in ('TS','SELF','CHANGE','DURABLE'))


def held_custody(rows):
    p=rows[-1]['proof'];initial=rows[0]['model']['comments'];before=rows[1];retired=rows[2]
    change=p.get('visitReplacement',{})
    require(change.get('kind')=='synthetic-list-visit-replacement'
        and change.get('account')==initial['account'] and change.get('epoch')==initial['epoch']
        and change.get('team')=='T1' and change.get('user')==SELF
        and R['integer'](change.get('beforeVisit'),1) and R['integer'](change.get('afterVisit'),1)
        and change['beforeVisit']!=change['afterVisit'] and change.get('before')==before['model']['edit']
        and change.get('oldCurrent') is False and change.get('oldVisible') is False and change.get('oldRetained') is True,
        'Synthetic original visit replacement broadened authority')
    require(change.get('after',{}).get('active') is None and retired['model']['edit'].get('active') is None,
        'Synthetic visit retirement kept active edit grant')
    require(rows[5]['model']['edit']==rows[6]['model']['edit'] and rows[5]['editInventory']==rows[6]['editInventory']
        and rows[5]['geometry']==rows[6]['geometry'],'Late file completion changed cancelled edit or native reply')
    finished=p.get('detachedFinished',{})
    require(finished.get('current') is False and finished.get('visible') is False and finished.get('retained') is False
        and finished.get('geometry')==retired['geometry'],'Old edit native holder survived retirement')
    require(p.get('durableBeforeRetirement')==before['editInventory']==retired['editInventory'],
        'Detached capture changed original durable modified edit')
    restored=rows[4]['model']['edit']['active']
    require(p.get('heldOriginal')==restored and restored['blocks']==p['editedBlocks']
        and restored['scope']!=before['model']['edit']['active']['scope'] and restored.get('nativeReady') is False,
        'Modified durable edit was not restored under fresh pending owner')
    for index in (5,6):
        row=rows[index]
        require(row['model']['edit']['active'] is None and row['editInventory']['seed'] is None
            and row['editInventory']['blocks']==[],'Cancel/late file reply repopulated cleared durable edit')
    fresh=rows[8]['model']['edit']['active']
    require(p.get('freshSeed')==fresh and fresh['blocks']==received()['blocks'][:1] and fresh['scope']!=restored['scope'],
        'Fresh edit resurrected cancelled modified blocks or borrowed old owner')
    require(rows[9]['model']['edit']['active'] is None and rows[9]['editInventory']['seed'] is None,
        'Final Cancel did not restore independent reply')
U['held_custody']=held_custody


def transcript(value,native,envelope,plain):
    rows=value.get('rows',[]);expected=tokens(received()['blocks'][:1])
    require(rows and all(r.get('id')==S.CANVAS and r.get('current') is True
        and r.get('retryable') is False for r in rows),'Original transcript receipt not settled/current')
    for row in rows:
        obs=row.get('titleUsers');items=obs.get('titles',{}).get('tokens') if isinstance(obs,dict) else None
        require(isinstance(items,list) and len(items)==1 and items[0] in expected,
            'Transcript borrowed a different original occurrence')
        S.observed(obs,envelope,items,plain)
    require(native.get('ok') is True and native.get('ts')==TS and len(native.get('tokens',[]))==2,
        'Original transcript native title missing')
    for token in native['tokens']:
        require(token.get('id')==S.CANVAS and token.get('kind')=='canvas' and token.get('text')=='Untitled'
            and token.get('renderedText')=='Untitled' and token.get('visible') is True and token.get('hit') is True,
            'Original transcript empty title not exposed')
        N['rectangle'](token.get('rect',{}),native['viewport'])
        require(token.get('textRects') and all(x.get('hit') is True for x in token['textRects']),
            'Original transcript text fragments not exposed')


def returned_transcript(value,envelope):
    rows=value.get('rows',[]);expected=tokens(received()['blocks'][:1])
    require(rows and all(r.get('id')==S.CANVAS and r.get('current') is True
        and r.get('retryable') is False for r in rows),'Returned transcript did not settle before original edit invalidation')
    for row in rows:
        obs=row.get('titleUsers',{});items=obs.get('titles',{}).get('tokens')
        require(isinstance(items,list) and len(items)==1 and items[0] in expected,
            'Returned transcript borrowed edit or foreign title occurrence')
        S.observed(obs,envelope,items,False)


def transcript_propagation(rows):
    """Positive file recovery queues a NEW transcript receipt, never an edit grant."""
    old_stage,new_stage,final=rows[7],rows[8],rows[9]
    p=new_stage['proof'].get('transcriptRecovery',{})
    require(all('transcriptRecovery' not in r['proof'] and 'transcriptRecoveryFinal' not in r['proof'] for r in rows[:8])
        and 'transcriptRecoveryFinal' not in new_stage['proof']
        and final['proof'].get('transcriptRecovery')==p,'Transcript recovery proof appeared early or was rewritten')
    before,after=p.get('before',{}),p.get('after',{})
    require(before==old_stage['model']['transcriptTitles'] and after==new_stage['model']['transcriptTitles']
        and before.get('running') is False and after.get('running') is False
        and before.get('generation')==after.get('generation')==new_stage['model']['comments']['comments']['generation'],
        'Transcript recovery replaced the original Comments generation')
    old_rows,new_rows=before.get('rows'),after.get('rows')
    require(isinstance(old_rows,list) and isinstance(new_rows,list) and len(old_rows)==len(new_rows)==1,
        'Transcript recovery changed bounded same-source deduplication')
    old,new=old_rows[0],new_rows[0]
    require(old.get('id')==new.get('id')==S.CANVAS and old.get('ordinal')==new.get('ordinal')==0
        and old.get('phase')=='ready' and old.get('state')=='unavailable' and old.get('current') is True
        and old.get('retryable') is True and old.get('retryAt')==0
        and new.get('phase')=='ready' and new.get('state')=='ready' and new.get('current') is True
        and new.get('retryable') is False and new.get('retryAt')==0 and new.get('notice') is None
        and R['integer'](old.get('serial'),1) and R['integer'](new.get('serial'),1) and new['serial']>old['serial'],
        'Recovery reused a stale job or silently retried an Unavailable failure')
    raw=new_stage['model']['coldFiles']['responses'][3]['raw']
    token=tokens(received()['blocks'][:1])[0]
    S.observed(old.get('titleUsers'),raw,[token],False,current=False)
    S.observed(new.get('titleUsers'),raw,[token],False)
    # Actual file source was unavailable before release; existing cold() requires
    # current original singleton4 afterward. Member-only staleness cannot satisfy it.
    require(old_stage['model']['coldFiles']['source']==old_stage['proof']['fileInvalidation']['after'],
        'Transcript requeue did not follow actual same-file source retirement')
    require(p.get('readLedgerBefore')==old_stage['model']['coldFiles']['requests']
        and p.get('readLedgerAfter')==p['readLedgerBefore']==new_stage['model']['coldFiles']['requests']
        and len(p['readLedgerAfter'])==4,'Independent transcript recovery performed extra files.info')
    transcript(after,p.get('native',{}),raw,False)
    active=new_stage['model']['edit']['active'];mount=p.get('editorMount',{})
    require(mount.get('ok') is True and mount.get('scope')==active['scope']
        and str(mount.get('generation'))==str(active['generation']) and R['uuid'](mount.get('mount'))
        and mount.get('blocks')==active['blocks'],'Recovered title borrowed a different editor mount')
    for key in ('editorBefore','editorAfter'):
        native=p.get(key,{})
        U['extended_geometry'](native,1,new_stage['proof']['replyBlocks'],active['blocks'],new_stage['proof']['editedBlocks'],DURABLE)
        require(native.get('edit',{}).get('sameNode') is True and native.get('reply',{}).get('sameNode') is True,
            'Transcript recovery replaced the original edit/reply native nodes')
    require(p['editorBefore']['edit']['blocks']==p['editorAfter']['edit']['blocks']==active['blocks']
        and p['editorBefore']['reply']['blocks']==p['editorAfter']['reply']['blocks']==new_stage['proof']['replyBlocks'],
        'Transcript recovery changed original edit/reply wire')
    last=final['proof'].get('transcriptRecoveryFinal',{})
    require(last.get('after')==after==final['model']['transcriptTitles']
        and last.get('readLedgerAfter')==p['readLedgerAfter']==final['model']['coldFiles']['requests'],
        'Cancel replaced recovered transcript or repeated metadata acquisition')
    transcript(last['after'],last.get('native',{}),raw,False)


def title_proofs(receipt,case):
    rows=receipt['stages'];plain=case==LIFECYCLE
    ready_counts=(0,1,1,2,2,2,2) if plain else (0,1,1,1,1,1,1,1,2,2)
    history=[];previous=None;seed=U['durable_seed'](rows[0]['model']['comments']);scope=rows[0]['journal']['scope']
    for index,row in enumerate(rows):
        p,m,name=row['proof'],row['model'],row['stage'];source=m.get('titleSource')
        S.unused_member_source(source,case)
        envelope=S.cold(row,plain,index,previous)
        if index==0:
            require(p.get('initialTitleSource')==source and p.get('initialTranscript')==m.get('transcriptTitles'),
                'Initial independently settled transcript/source replaced')
            transcript(p['initialTranscript'],p['initialTranscriptNative'],envelope,plain)
        else:
            for key in ('initialTitleSource','initialTranscript','initialTranscriptNative'):
                require(p.get(key)==rows[0]['proof'].get(key),'Original initial title proof rewritten')
        ready=p.get('titleReady')
        require(isinstance(ready,list) and len(ready)==ready_counts[index] and ready[:len(history)]==history,
            'Original pre-input title admission history changed')
        for entry in ready:S.ready(entry,envelope,received()['blocks'][:1],plain)
        history=copy.deepcopy(ready)
        a=m['edit'].get('active')
        if a is not None:
            j=a.get('titleUsers')
            require(isinstance(j,dict) and j.get('scope')==a.get('scope') and j.get('generation')==a.get('generation')
                and j.get('ts')==TS and j.get('tokens')==tokens(received()['blocks'][:1]),
                'Independent title job borrowed another edit owner/token')
            admitted=[v['job'] for v in ready if v['job']['scope']==a['scope']]
            if a.get('nativeReady') is True:
                require(len(admitted)==1 and all(j.get(k)==admitted[0].get(k) for k in
                    ('scope','generation','ts','revision','blocks','tokens')),
                    'Mounted edit lacks original pre-input title provenance')
            else:
                require(j.get('blocks')==a.get('blocks') and j.get('revision')==a.get('revision')
                    and j.get('current') is True and j.get('observed') is None,
                    'Pending title source differs from original durable edit')
        require(row['journal']==rows[0]['journal'],'Title work changed independent reply journal')
        inv=row['editInventory']
        if name=='saving':U['durable_inventory'](inv,scope,seed,p['editedBlocks'],phase='Pending',ack=None)
        elif name in ('draft','cancelled','late','final-cancel','saved','readback','reopened'):
            U['durable_inventory'](inv,scope,None,[])
        else:
            blocks=p['editedBlocks'] if not plain and index in (1,2,3,4) else (
                p['cancelledBlocks'] if plain and name=='editing' else received()['blocks'][:1])
            U['durable_inventory'](inv,scope,seed,blocks)
        actions=p.get('titleActions');expected=[]
        if not plain:
            if index>=3:expected.append('slack-list-record-comments')
            if index>=4:expected.append('slack-list-comment-edit-title-retry')
            if index>=5:expected.append('slack-list-comment-edit-cancel')
            if index>=9:expected.append('slack-list-comment-edit-cancel')
        require(isinstance(actions,list) and [v.get('id') for v in actions]==expected,
            'Original native title actions omitted/repeated')
        for action in actions:U['native_button'](action.get('native'),action.get('id'),row['geometry'].get('viewport',rows[0]['geometry']['viewport']))
        if previous:
            for key in ('titleActions','releases'):
                require(p.get(key,[])[:len(previous['proof'].get(key,[]))]==previous['proof'].get(key,[]),
                    'Original native action/release proof rewritten')
            for key in ('fileInvalidation','heldFile','freshFile','restoredOriginal','heldOriginal','freshSeed',
                        'visitReplacement','detachedFinished','durableBeforeRetirement'):
                if key in previous['proof']:
                    require(p.get(key)==previous['proof'][key],'Original immutable custody proof rewritten: '+key)
        if not plain and index>=3:
            require(p.get('restoredOriginal')==rows[3]['model']['edit']['active'],
                'Original failed durable edit proof replaced')
            require(p.get('returnTranscript')==rows[3]['proof'].get('returnTranscript'),
                'Original returned transcript proof recaptured')
            returned_transcript(p.get('returnTranscript',{}),envelope)
        if not plain and index>=7:
            require(p['freshFile']['job']==rows[7]['model']['edit']['active']['titleUsers'],
                'Fresh pending acquisition proof recaptured from successor')
        previous=row
    return receipt


def validate(fixture,receipt,directory,*,expected_case,dimensions=None,png=True):
    require(expected_case in STAGES,'Unknown own-edit empty Canvas case')
    fn=U['validate_standard'] if expected_case==LIFECYCLE else U['validate_extended']
    fn(fixture,receipt,directory,expected_case=expected_case,dimensions=dimensions,png=png)
    title_proofs(receipt,expected_case)
    if expected_case==RECOVERY:transcript_propagation(receipt['stages'])
    else:require(all('transcriptRecovery' not in r['proof'] and 'transcriptRecoveryFinal' not in r['proof'] for r in receipt['stages']),
        'Recovery proof escaped the exact named case')
    return receipt


def validator(case):
    require(case in STAGES,'Unknown own-edit empty Canvas case')
    return lambda fixture,receipt,directory,**kw:validate(fixture,receipt,directory,expected_case=case,**kw)
