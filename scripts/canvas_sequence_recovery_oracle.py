"""V14 whole ordered operation is inert retained data, never replay authority."""
import base64
import math
import canvas_sequence_wire as W
import canvas_sequence_oracle as N
import canvas_split_recovery_oracle as R
from canvas_heading import require, decoded, field

STAGES=R.STAGES
WARNING='Any, all, or none of these paragraph changes may have reached Slack. Removing this lock leaves every outcome unknown.'
NOTICE='Edits remain part of this unresolved change, including paragraphs that became empty again.'


def retained_members(checkpoint,original,append_count=1,history='bootstrap',layout='no-following',cadence='straight',corpus='ascii'):
    """Ordered appended IDs, with no transport/replay authority."""
    e=W.config(append_count,history,layout,cadence,corpus);rows=W.inventory(original,e)
    require(isinstance(checkpoint,dict),'Missing sequence checkpoint')
    action=checkpoint.get('action',{}).get('EndSequence',{});members=action.get('members')
    require(isinstance(members,list) and len(members)==e.append_count,'Retained member list missing')
    value=dict(kind='sequence',parent=checkpoint.get('target'),members=[dict(m,finalRtml=W.M.rtml(e.final[i])) for i,m in enumerate(members)],
               session=checkpoint.get('session'),sequence=checkpoint.get('sequence'),formTitle='Fixture title',httpFactoryExecuted=False,clearsChild=False,
               documentData=base64.b64encode(W.S.byte_vector(checkpoint.get('request'))).decode())
    selected,_=W.write(value,rows,e);W.journal(dict(scope=W.S.SCOPE,receipt=dict(operation='inert-only',checkpoint=checkpoint)),e,value,original)
    return selected


def canonical(value,original,e,changed=False):
    require(isinstance(value,dict) and set(value)==set(original) and all(value.get(k)==original[k] for k in original if k!='sections'),
            'Repeat recovery document owner/raw shape changed')
    rows=value.get('sections');require(isinstance(rows,list) and len(rows)==len(original['sections']),'Repeat recovery inventory incomplete')
    for current,prior in zip(rows,original['sections']):
        require(current.get('id')==prior['id'],'Repeat recovery canonical order changed')
        if changed and prior['id']==e.original:
            now,before=decoded(current.get('raw')),decoded(prior['raw'])
            require(type(current.get('sequence')) is int and current['sequence']==field(now,2,0)==2000
                    and W.S.stable(now,(2,))==W.S.stable(before,(2,)),'Repeat recovery stale original changed beyond revision')
        else:require(current==prior,'Repeat recovery changed an untouched canonical row')


def preview(value,name,viewport,e,selected):
    require(isinstance(value,dict) and value.get('ok') is True and value.get('editorAbsent') is True and value.get('viewport')==viewport
            and type(value.get('reviewActionPresent')) is bool,'Sequence recovery read-only proof missing')
    panel=name in ('review-pair','review-controls','stale','review-confirmation','storage-failed','retry-confirmation')
    require(value.get('panel') is panel and all(value.get(k) is None for k in ('insertion','startTyping','initialTail','originalChild')),'Sequence review panel/action changed')
    require(value.get('split')==[] and value.get('emptyLabels')==[],'Sequence acquired unrelated split preview')
    if name=='account-changed':require(value['reviewActionPresent'] is False,'Old-owner sequence review survived')
    if not panel:
        require(value.get('sequence') is None and value.get('current')==value.get('missing')==[]
                and all(value.get(k) is None for k in ('checked','checkboxDisabled','cancelDisabled','submitDisabled','error','warning')),'Closed sequence retained controls');return
    members=[dict(id=identity,index=i,text=e.final[i],reused=e.reused and i==0,sourceText='' if e.reused and i==0 else None,
                  reportCount=len(e.reports[i]),emptyLabel=None if e.final[i] else 'Empty paragraph') for i,identity in enumerate(selected)]
    require(value.get('sequence')==dict(originalText=e.seed,parentText=e.seed,members=members)
            and all(type(m.get('index')) is int and type(m.get('reused')) is bool and type(m.get('reportCount')) is int for m in value['sequence']['members']),
            'Sequence whole ordered retained preview changed')
    current=[dict(id='title',deleted=False,text='Fixture title')]+[dict(id=r['id'],deleted=False,text=r['text']) for r in N.reader(e)]
    require(value.get('current')==current and value.get('missing')==list(selected[1:] if e.reused else selected),'Sequence current source/missing IDs changed')
    action='Reuse the following empty paragraph and continue adding paragraphs' if e.reused else 'Add and edit paragraphs after this paragraph'
    warning=value.get('warning');expected=(action,WARNING,*R.WARNINGS[1:])+((NOTICE,) if any(e.reports) else ())
    require(isinstance(warning,str) and all(s in warning for s in expected),'Sequence unknown outcome/history explanation missing')
    checked=name not in ('review-pair','review-controls');disabled=name in ('stale','storage-failed')
    require(value.get('checked') is checked and value.get('checkboxDisabled') is disabled and value.get('submitDisabled') is (disabled or not checked)
            and value.get('cancelDisabled') is False and value.get('error')==(R.STORAGE if name=='storage-failed' else R.CHANGED if disabled else None),'Sequence review failure/decision controls changed')


def validate(phase,stages,terminal,checkpoint,append_count=1,history='bootstrap',layout='no-following',cadence='straight',corpus='ascii'):
    e=W.config(append_count,history,layout,cadence,corpus);W.labels(terminal,e,True)
    require(phase in STAGES and isinstance(stages,list) and [s.get('stage') for s in stages]==list(STAGES[phase]),'Sequence recovery stages missing/reordered')
    require(terminal.get('mode')==phase and all(terminal.get(k) is True for k in ('ok','noSession','noTransport','noReplay','outcomeUnknown','canonicalUnchanged')),
            'Sequence recovery isolation/outcome changed')
    for key,count in dict(dispatches=0,reservations=int(phase=='seed'),retirements=0).items():
        require(type(terminal.get(key)) is int and terminal[key]==count,'Sequence recovery dispatched/replayed/sealed retained data')
    geometry=terminal.get('layout',{});viewport=geometry.get('viewport')
    require(geometry.get('ok') is True and isinstance(viewport,list) and len(viewport)==2
            and all(type(n) in (int,float) and math.isfinite(n) and n>0 for n in viewport),'Sequence recovery layout missing')
    original=stages[0].get('canonical');selected=retained_members(checkpoint,original,append_count,history,layout,cadence,corpus)
    epoch=stages[0].get('owner',{}).get('epoch');require(type(epoch) is int and epoch>=0,'Sequence recovery owner epoch missing')
    guard=terminal.get('accountGuard')
    if phase=='review':
        require(isinstance(guard,dict) and all(type(guard.get(k)) is int and guard[k]>0 for k in ('beforeRead','restoredRead'))
                and guard['beforeRead']!=guard['restoredRead'],'Sequence return reused old review read')
        require(guard==dict(beforeEpoch=epoch,changedEpoch=epoch+1,changedAccount=R.OTHER_ACCOUNT,oldReviewRemoved=True,oldReviewActionAbsent=True,
                receiptPreserved=True,restoredEpoch=epoch+2,restoredAccount=R.ACCOUNT,beforeRead=guard['beforeRead'],restoredRead=guard['restoredRead'])
                and all(guard.get(k) is True for k in ('oldReviewRemoved','oldReviewActionAbsent','receiptPreserved'))
                and all(type(guard.get(k)) is int for k in ('beforeEpoch','changedEpoch','restoredEpoch')),'Sequence account remount lost exact pair/fresh ownership')
    else:require(guard is None,'Unexpected Sequence account replacement proof')
    operation=None
    for ordinal,stage in enumerate(stages):
        name=stage['stage'];W.labels(stage,e,True)
        require(stage.get('mode')==phase and type(stage.get('ordinal')) is int and stage['ordinal']==ordinal
                and all(stage.get(k) is True for k in ('ok','noSession','noTransport')) and type(stage.get('dispatches')) is int
                and stage['dispatches']==0 and stage.get('ownerValid') is (name!='account-changed'),'Sequence recovery owner/dispatch changed')
        changed=name=='account-changed';restored=phase=='review' and ordinal>STAGES[phase].index('account-changed');owner=stage.get('owner',{})
        require(owner==dict(account=R.OTHER_ACCOUNT if changed else R.ACCOUNT,epoch=epoch+(1 if changed else 2 if restored else 0),workspace='T1',user='U1')
                and type(owner.get('epoch')) is int,'Sequence owner reset across remount')
        canonical(stage.get('canonical'),original,e,name=='stale');preview(stage.get('preview'),name,viewport,e,selected)
        if name in ('review-controls','review-confirmation','retry-confirmation'):R.controls(stage.get('reviewGeometry'),name!='review-controls',viewport)
        else:require(stage.get('reviewGeometry') is None,'Unexpected Sequence review control proof')
        journal=stage.get('journal',{});archived=name in ('removed','refreshed','archive-restored')
        require(type(journal.get('archiveCount')) is int and journal['archiveCount']==int(archived)
                and journal.get('archivedExact') is (None if phase=='reviewed' else archived),'Sequence unknown-outcome archive changed')
        active=journal.get('active')
        if archived:require(active is None,'Archived Sequence pair kept active lock')
        else:
            require(isinstance(active,dict) and set(active)=={'operation','checkpoint'} and active.get('checkpoint')==checkpoint
                    and isinstance(active.get('operation'),str) and active['operation'],'Sequence exact v14 operation lost')
            if operation is None:operation=active['operation']
            require(active['operation']==operation,'Sequence review replaced original operation')
    require(terminal.get('canonical')==original==stages[-1]['canonical'] and terminal.get('journal')==stages[-1]['journal'],
            'Sequence recovery terminal differs from original last stage')
    return selected
