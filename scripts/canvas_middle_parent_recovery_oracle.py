"""Inert v17 middle-parent recovery with live draft and complete history custody."""
import base64
import math
import canvas_middle_parent_wire as typed
import canvas_split_recovery_oracle as review
import canvas_split_oracle as shared
from canvas_heading import require
STAGES=review.STAGES
PARENT_NOTICE='The first paragraph was edited after Enter. Those edits remain part of this unresolved change, even when the text was changed back.'
NOTICE="Edits made before Enter remain part of this unresolved change, even when the text was changed back."
canonical=review.canonical


def retained_pair(checkpoint,original,corpus='ascii',typing='none',parent_mode='bootstrap',pre_enter='none',middle_parent='correction',middle_cadence='parent-first'):
    e=typed.config(corpus,typing,parent_mode,pre_enter,middle_parent,middle_cadence);rows=typed.inventory(original,e)
    require(isinstance(checkpoint,dict),'Missing middle typed checkpoint');action=checkpoint.get('action',{}).get('SplitParent',{})
    value=dict(kind='split',documentData=base64.b64encode(shared.byte_vector(checkpoint.get('request'))).decode(),parent=checkpoint.get('target'),
               child=action.get('child'),session=checkpoint.get('session'),sequence=checkpoint.get('sequence'),formTitle='Fixture title',httpFactoryExecuted=False)
    child,*_=typed.write(value,rows[e.parent],e)
    typed.journal(dict(scope=shared.SCOPE,receipt=dict(operation='retained-data',checkpoint=checkpoint)),e,value,original)
    return child


def preview(value,name,viewport,e):
    require(isinstance(value,dict) and value.get('ok') is True and value.get('editorAbsent') is True and value.get('viewport')==viewport
            and type(value.get('reviewActionPresent')) is bool,'Middle recovery native/read-only proof missing')
    panel=name in ('review-pair','review-controls','stale','review-confirmation','storage-failed','retry-confirmation')
    require(value.get('panel') is panel,'Middle recovery panel presence changed')
    if name=='account-changed':require(value['reviewActionPresent'] is False,'Old account review action survived')
    if not panel:
        require(all(value.get(k)==[] for k in ('split','current','missing')) and all(value.get(k) is None for k in
                ('checked','checkboxDisabled','cancelDisabled','submitDisabled','error','warning','initialTail','originalChild','preEnter','parentHistory')),'Closed middle recovery retained draft/control')
        return
    require(value.get('split')==[dict(kind='original',text=e.full),dict(kind='parent',text=e.seed),dict(kind='child',text=e.updates[-1])]
            and value.get('initialTail')==(dict(text=e.tail) if e.child_reports else None) and value.get('originalChild') is None,'Middle recovery initial/final split custody changed')
    require(value.get('current')==[dict(id='title',deleted=False,text='Fixture title'),dict(id=e.parent,deleted=False,text=e.full),dict(id='neighbor',deleted=False,text='Another paragraph')]
            and value.get('missing')==['child'],'Middle recovery replaced actual canonical comparison')
    require(value.get('preEnter')==(dict(text=e.live,historyNotice=NOTICE) if e.pre_reports else None),'Middle recovery prelude draft/history notice lost')
    require(value.get('parentHistory')==dict(text=e.initial_parent,historyNotice=PARENT_NOTICE,emptyLabel='Empty paragraph' if not e.seed else None),'Middle recovery initial parent/history or final empty label lost')
    warning=value.get('warning');require(isinstance(warning,str) and all(t in warning for t in review.WARNINGS),'Middle unknown pair outcome warning missing')
    checked=name not in ('review-pair','review-controls');disabled=name in ('stale','storage-failed')
    require(value.get('checked') is checked and value.get('checkboxDisabled') is disabled and value.get('submitDisabled') is (disabled or not checked)
            and value.get('cancelDisabled') is False and value.get('error')==(review.STORAGE if name=='storage-failed' else review.CHANGED if disabled else None),'Middle recovery failure/decision controls changed')


def validate(phase, stages, terminal, checkpoint, corpus="ascii", typing="none", parent_mode="bootstrap", pre_enter="none", middle_parent="correction", middle_cadence="parent-first"):
    mode="middle"; expected = typed.config(corpus, typing, parent_mode,pre_enter,middle_parent,middle_cadence)
    typed.labels(terminal, expected, recovery=True)
    require(phase in STAGES and isinstance(stages, list) and [s.get("stage") for s in stages] == list(STAGES[phase]), "Typed recovery stages missing/reordered")
    require(terminal.get("mode") == phase and terminal.get("splitMode") == mode and terminal.get("corpus") == corpus and terminal.get("typing") == typing
            and all(terminal.get(k) is True for k in ("ok", "noSession", "noTransport", "noReplay", "outcomeUnknown", "canonicalUnchanged")), "Typed recovery mode/isolation changed")
    for key, count in dict(dispatches=0, reservations=int(phase == "seed"), retirements=0).items():
        require(type(terminal.get(key)) is int and terminal[key] == count, "Typed recovery resumed/sealed retained edit")
    layout = terminal.get("layout", {}); viewport = layout.get("viewport")
    require(layout.get("ok") is True and isinstance(viewport, list) and len(viewport) == 2
            and all(type(n) in (int, float) and math.isfinite(n) and n > 0 for n in viewport), "Typed recovery layout absent")
    original = stages[0].get("canonical"); child = retained_pair(checkpoint, original, corpus, typing, parent_mode,pre_enter,middle_parent,middle_cadence)
    epoch = stages[0].get("owner", {}).get("epoch")
    require(type(epoch) is int and epoch >= 0, "Typed recovery owner epoch absent")
    guard = terminal.get("accountGuard")
    if phase == "review":
        require(isinstance(guard, dict) and all(type(guard.get(k)) is int and guard[k] > 0 for k in ("beforeRead", "restoredRead"))
                and guard["beforeRead"] != guard["restoredRead"], "Typed account return recycled review read")
        require(guard == dict(beforeEpoch=epoch, changedEpoch=epoch + 1, changedAccount=review.OTHER_ACCOUNT,
                              oldReviewRemoved=True, oldReviewActionAbsent=True, receiptPreserved=True, restoredEpoch=epoch + 2, restoredAccount=review.ACCOUNT,
                              beforeRead=guard["beforeRead"], restoredRead=guard["restoredRead"])
                and all(guard.get(k) is True for k in ("oldReviewRemoved", "oldReviewActionAbsent", "receiptPreserved"))
                and all(type(guard.get(k)) is int for k in ("beforeEpoch", "changedEpoch", "restoredEpoch")), "Typed account remount lost saved pair/fresh owner")
    else: require(guard is None, "Unexpected typed account replacement proof")
    operation = None
    for ordinal, stage in enumerate(stages):
        name = stage["stage"]; typed.labels(stage, expected, recovery=True)
        require(stage.get("mode") == phase and stage.get("splitMode") == mode and stage.get("corpus") == corpus and stage.get("typing") == typing
                and type(stage.get("ordinal")) is int and stage["ordinal"] == ordinal and all(stage.get(k) is True for k in ("ok", "noSession", "noTransport"))
                and type(stage.get("dispatches")) is int and stage["dispatches"] == 0 and stage.get("ownerValid") is (name != "account-changed"), "Typed recovery stage owner/dispatch changed")
        changed = name == "account-changed"; restored = phase == "review" and ordinal > STAGES[phase].index("account-changed"); owner = stage.get("owner", {})
        require(owner == dict(account=review.OTHER_ACCOUNT if changed else review.ACCOUNT, epoch=epoch + (1 if changed else 2 if restored else 0), workspace="T1", user="U1")
                and type(owner.get("epoch")) is int, "Typed owner reset across remount")
        canonical(stage.get("canonical"), original, changed=name == "stale")
        preview(stage.get("preview"), name, viewport, expected)
        if name in ("review-controls", "review-confirmation", "retry-confirmation"):
            review.controls(stage.get("reviewGeometry"), name != "review-controls", viewport)
        elif name=='review-pair':
            geometry=stage.get('reviewGeometry',{});rect=geometry.get('rect')
            require(geometry.get('ok') is True and geometry.get('parentHistory') is True and geometry.get('visible') is True
                    and geometry.get('text')==expected.initial_parent and geometry.get('historyNotice')==PARENT_NOTICE and geometry.get('viewport')==viewport
                    and isinstance(rect,list) and len(rect)==4 and all(type(v) in (int,float) and math.isfinite(v) for v in rect)
                    and rect[2]>0 and rect[3]>0 and rect[0]>=0 and rect[1]>=0 and rect[0]+rect[2]<=viewport[0]+1 and rect[1]+rect[3]<=viewport[1]+1,
                    'Pre-middle live draft/history notice not fully visible')
        else: require(stage.get("reviewGeometry") is None, "Unexpected typed control proof")
        journal = stage.get("journal", {}); archived = name in ("removed", "refreshed", "archive-restored")
        require(type(journal.get("archiveCount")) is int and journal["archiveCount"] == int(archived)
                and journal.get("archivedExact") is (None if phase == "reviewed" else archived), "Typed unknown-outcome archive changed")
        active = journal.get("active")
        if archived: require(active is None, "Typed archive retained active lock")
        else:
            require(isinstance(active, dict) and set(active) == {"operation", "checkpoint"} and active.get("checkpoint") == checkpoint
                    and isinstance(active.get("operation"), str) and active["operation"], "Typed exact v8 whole ordered pair lost")
            if operation is None: operation = active["operation"]
            require(active["operation"] == operation, "Typed review replaced original operation")
    require(terminal.get("canonical") == original == stages[-1]["canonical"] and terminal.get("journal") == stages[-1]["journal"], "Typed recovery terminal differs from original last stage")
    return child
