"""V12 three-member repeat is inert retained data, never a replay authority."""
import base64
import math

import canvas_repeat_enter_wire as W
import canvas_repeat_enter_oracle as N
import canvas_split_recovery_oracle as R
from canvas_heading import require, decoded, field

STAGES=R.STAGES
TRIPLE_WARNING='Any, all, or none of these three paragraph changes may have reached Slack. Removing this lock leaves all three outcomes unknown.'


def retained_pair(checkpoint,original,repeat_history='bootstrap',repeat_layout='no-following',corpus='ascii'):
    """Return (first_id, second_id); neither ID grants replay authority."""
    e=W.config(repeat_history,repeat_layout,corpus);rows=W.inventory(original,e)
    require(isinstance(checkpoint,dict),'Missing Repeat Enter retained checkpoint')
    value=dict(kind='repeat',parent=checkpoint.get('target'),child=checkpoint.get('action',{}).get('EndRepeat',{}).get('child'),
               secondChild=checkpoint.get('action',{}).get('EndRepeat',{}).get('second_child'),session=checkpoint.get('session'),sequence=checkpoint.get('sequence'),formTitle='Fixture title',httpFactoryExecuted=False,clearsChild=False,
               documentData=base64.b64encode(W.S.byte_vector(checkpoint.get('request'))).decode())
    selected,*_=W.write(value,rows,e)
    W.journal(dict(scope=W.S.SCOPE,receipt=dict(operation='retained-data-only',checkpoint=checkpoint)),e,value,original)
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


def preview(value,name,viewport,e):
    require(isinstance(value,dict) and value.get('ok') is True and value.get('editorAbsent') is True and value.get('viewport')==viewport
            and type(value.get('reviewActionPresent')) is bool,'Repeat recovery lacks native read-only proof')
    panel=name in ('review-pair','review-controls','stale','review-confirmation','storage-failed','retry-confirmation')
    require(value.get('panel') is panel,'Repeat recovery panel changed')
    require(all(value.get(k) is None for k in ('insertion','startTyping','initialTail','originalChild')),'Repeat review acquired another action preview')
    if name=='account-changed':require(value['reviewActionPresent'] is False,'Old-account Repeat review action survived')
    if not panel:
        require(all(value.get(k)==[] for k in ('split','current','missing','emptyLabels'))
                and all(value.get(k) is None for k in ('checked','checkboxDisabled','cancelDisabled','submitDisabled','error','warning')),
                'Closed Repeat review retained pair/control content');return
    require(value.get('split')==[dict(kind='original',text=e.seed),dict(kind='parent',text=e.seed),dict(kind='child',text=''),dict(kind='secondChild',text='')]
            and value.get('emptyLabels')==[]
            and value.get('missing')==(['secondChild'] if e.reused else ['child','secondChild']),'Repeat retained pair/placement markers changed')
    current=[dict(id='title',deleted=False,text='Fixture title')]+[dict(id=row['id'],deleted=False,text=row['text']) for row in N.reader(e)]
    require(value.get('current')==current,'Repeat current source comparison lost/reordered/substituted original rows')
    action='Reuse the following empty paragraph and add another after it' if e.reused else 'Add two empty paragraphs after this paragraph'
    warning=value.get('warning');require(isinstance(warning,str) and all(s in warning for s in (action,TRIPLE_WARNING,*R.WARNINGS[1:])),'Repeat review lost whole-pair unknown-outcome explanation')
    checked=name not in ('review-pair','review-controls');disabled=name in ('stale','storage-failed')
    require(value.get('checked') is checked and value.get('checkboxDisabled') is disabled and value.get('submitDisabled') is (disabled or not checked)
            and value.get('cancelDisabled') is False and value.get('error')==(R.STORAGE if name=='storage-failed' else R.CHANGED if disabled else None),
            'Repeat recovery failure/decision controls changed')


def validate(phase,stages,terminal,checkpoint,repeat_history='bootstrap',repeat_layout='no-following',corpus='ascii'):
    e=W.config(repeat_history,repeat_layout,corpus);W.labels(terminal,e,True)
    require(phase in STAGES and isinstance(stages,list) and [s.get('stage') for s in stages]==list(STAGES[phase]),'Repeat recovery stages missing/reordered')
    require(terminal.get('mode')==phase and all(terminal.get(k) is True for k in ('ok','noSession','noTransport','noReplay','outcomeUnknown','canonicalUnchanged')),
            'Repeat recovery isolation/outcome changed')
    for key,count in dict(dispatches=0,reservations=int(phase=='seed'),retirements=0).items():
        require(type(terminal.get(key)) is int and terminal[key]==count,'Repeat recovery dispatched/replayed/sealed retained data')
    layout=terminal.get('layout',{});viewport=layout.get('viewport')
    require(layout.get('ok') is True and isinstance(viewport,list) and len(viewport)==2
            and all(type(n) in (int,float) and math.isfinite(n) and n>0 for n in viewport),'Repeat recovery layout missing')
    original=stages[0].get('canonical');selected=retained_pair(checkpoint,original,repeat_history,repeat_layout,corpus)
    epoch=stages[0].get('owner',{}).get('epoch');require(type(epoch) is int and epoch>=0,'Repeat recovery owner epoch missing')
    guard=terminal.get('accountGuard')
    if phase=='review':
        require(isinstance(guard,dict) and all(type(guard.get(k)) is int and guard[k]>0 for k in ('beforeRead','restoredRead'))
                and guard['beforeRead']!=guard['restoredRead'],'Repeat return reused old review read')
        require(guard==dict(beforeEpoch=epoch,changedEpoch=epoch+1,changedAccount=R.OTHER_ACCOUNT,oldReviewRemoved=True,oldReviewActionAbsent=True,
                receiptPreserved=True,restoredEpoch=epoch+2,restoredAccount=R.ACCOUNT,beforeRead=guard['beforeRead'],restoredRead=guard['restoredRead'])
                and all(guard.get(k) is True for k in ('oldReviewRemoved','oldReviewActionAbsent','receiptPreserved'))
                and all(type(guard.get(k)) is int for k in ('beforeEpoch','changedEpoch','restoredEpoch')),'Repeat account remount lost exact pair/fresh ownership')
    else:require(guard is None,'Unexpected Repeat account replacement proof')
    operation=None
    for ordinal,stage in enumerate(stages):
        name=stage['stage'];W.labels(stage,e,True)
        require(stage.get('mode')==phase and type(stage.get('ordinal')) is int and stage['ordinal']==ordinal
                and all(stage.get(k) is True for k in ('ok','noSession','noTransport')) and type(stage.get('dispatches')) is int
                and stage['dispatches']==0 and stage.get('ownerValid') is (name!='account-changed'),'Repeat recovery owner/dispatch changed')
        changed=name=='account-changed';restored=phase=='review' and ordinal>STAGES[phase].index('account-changed');owner=stage.get('owner',{})
        require(owner==dict(account=R.OTHER_ACCOUNT if changed else R.ACCOUNT,epoch=epoch+(1 if changed else 2 if restored else 0),workspace='T1',user='U1')
                and type(owner.get('epoch')) is int,'Repeat owner reset across remount')
        canonical(stage.get('canonical'),original,e,name=='stale');preview(stage.get('preview'),name,viewport,e)
        if name in ('review-controls','review-confirmation','retry-confirmation'):R.controls(stage.get('reviewGeometry'),name!='review-controls',viewport)
        else:require(stage.get('reviewGeometry') is None,'Unexpected Repeat review control proof')
        journal=stage.get('journal',{});archived=name in ('removed','refreshed','archive-restored')
        require(type(journal.get('archiveCount')) is int and journal['archiveCount']==int(archived)
                and journal.get('archivedExact') is (None if phase=='reviewed' else archived),'Repeat unknown-outcome archive changed')
        active=journal.get('active')
        if archived:require(active is None,'Archived Repeat pair kept active lock')
        else:
            require(isinstance(active,dict) and set(active)=={'operation','checkpoint'} and active.get('checkpoint')==checkpoint
                    and isinstance(active.get('operation'),str) and active['operation'],'Repeat exact v12 triple lost')
            if operation is None:operation=active['operation']
            require(active['operation']==operation,'Repeat review replaced original operation')
    require(terminal.get('canonical')==original==stages[-1]['canonical'] and terminal.get('journal')==stages[-1]['journal'],
            'Repeat recovery terminal differs from original last stage')
    return selected
