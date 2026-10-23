"""Inert v9 typed-start review: whole pair and original report history remain data."""
import base64
import math
import canvas_start_typing_wire as typed
import canvas_split_recovery_oracle as review
import canvas_split_oracle as shared
from canvas_heading import require
STAGES=review.STAGES
canonical=review.canonical


def retained_insert(checkpoint,original,corpus='ascii',typing='correction',parent_mode='bootstrap'):
    e=typed.config(corpus,typing,parent_mode);typed.inventory(original,e)
    require(isinstance(checkpoint,dict),'Missing start insertion checkpoint')
    value=dict(kind='start',parent='paragraph',child=checkpoint.get('action',{}).get('StartTyping',{}).get('child'),session=checkpoint.get('session'),sequence=checkpoint.get('sequence'),
               documentData=base64.b64encode(shared.byte_vector(checkpoint.get('request'))).decode(),formTitle='Fixture title',httpFactoryExecuted=False)
    new,_,_=typed.write(value,typed.inventory(original,e)['paragraph'],e)
    typed.journal(dict(scope=shared.SCOPE,receipt=dict(operation='retained-inert-data',checkpoint=checkpoint)),e,value,original)
    return new


def preview(value,name,viewport,e):
    require(isinstance(value,dict) and value.get('ok') is True and value.get('editorAbsent') is True and value.get('viewport')==viewport
            and type(value.get('reviewActionPresent')) is bool,'Typed-start recovery native/read-only proof missing')
    panel=name in ('review-pair','review-controls','stale','review-confirmation','storage-failed','retry-confirmation')
    require(value.get('panel') is panel,'Typed-start recovery panel changed')
    require(all(value.get(k)==[] for k in ('split','current','missing')) and value.get('insertion') is None
            and value.get('initialTail') is None and value.get('originalChild') is None,'Typed-start review acquired another mutation preview')
    if name=='account-changed':require(value['reviewActionPresent'] is False,'Old account typed-start action survived')
    if not panel:
        require(all(value.get(k) is None for k in ('startTyping','checked','checkboxDisabled','cancelDisabled','submitDisabled','error','warning')),
                'Closed typed-start review retained draft/control')
        return
    require(value.get('startTyping')==dict(originalBeforeText=e.seed,originalAfterText=e.updates[-1],newText='',emptyLabel='Empty paragraph',currentText=e.seed),
            'Typed-start recovery original/new/current comparison changed')
    copy=('Add an empty paragraph before and edit this paragraph','Original paragraph before the edit','Intended original paragraph',
          'Intended new paragraph','Current original paragraph','The new empty paragraph belongs before the original paragraph.',
          'This is a current comparison, not confirmation that the saved changes succeeded.',review.WARNINGS[0],*review.WARNINGS[2:])
    warning=value.get('warning')
    require(isinstance(warning,str) and all(s in warning for s in copy),'Typed-start whole-pair unknown-outcome explanation changed')
    checked=name not in ('review-pair','review-controls');disabled=name in ('stale','storage-failed')
    require(value.get('checked') is checked and value.get('checkboxDisabled') is disabled and value.get('submitDisabled') is (disabled or not checked)
            and value.get('cancelDisabled') is False and value.get('error')==(review.STORAGE if name=='storage-failed' else review.CHANGED if disabled else None),
            'Typed-start review failure/decision controls changed')


def validate(phase, stages, terminal, checkpoint, corpus="ascii", typing="correction", parent_mode="bootstrap"):
    mode="start"; expected = typed.config(corpus, typing, parent_mode)
    typed.labels(terminal, expected, recovery=True)
    require(phase in STAGES and isinstance(stages, list) and [s.get("stage") for s in stages] == list(STAGES[phase]), "Start recovery stages missing/reordered")
    require(terminal.get("mode") == phase and terminal.get("splitMode") == mode and terminal.get("corpus") == corpus and terminal.get("typing") == typing
            and all(terminal.get(k) is True for k in ("ok", "noSession", "noTransport", "noReplay", "outcomeUnknown", "canonicalUnchanged")), "Start recovery mode/isolation changed")
    for key, count in dict(dispatches=0, reservations=int(phase == "seed"), retirements=0).items():
        require(type(terminal.get(key)) is int and terminal[key] == count, "Start recovery resumed/sealed retained edit")
    layout = terminal.get("layout", {}); viewport = layout.get("viewport")
    require(layout.get("ok") is True and isinstance(viewport, list) and len(viewport) == 2
            and all(type(n) in (int, float) and math.isfinite(n) and n > 0 for n in viewport), "Start recovery layout absent")
    original = stages[0].get("canonical"); child = retained_insert(checkpoint, original, corpus, typing, parent_mode)
    epoch = stages[0].get("owner", {}).get("epoch")
    require(type(epoch) is int and epoch >= 0, "Start recovery owner epoch absent")
    guard = terminal.get("accountGuard")
    if phase == "review":
        require(isinstance(guard, dict) and all(type(guard.get(k)) is int and guard[k] > 0 for k in ("beforeRead", "restoredRead"))
                and guard["beforeRead"] != guard["restoredRead"], "Start account return recycled review read")
        require(guard == dict(beforeEpoch=epoch, changedEpoch=epoch + 1, changedAccount=review.OTHER_ACCOUNT,
                              oldReviewRemoved=True, oldReviewActionAbsent=True, receiptPreserved=True, restoredEpoch=epoch + 2, restoredAccount=review.ACCOUNT,
                              beforeRead=guard["beforeRead"], restoredRead=guard["restoredRead"])
                and all(guard.get(k) is True for k in ("oldReviewRemoved", "oldReviewActionAbsent", "receiptPreserved"))
                and all(type(guard.get(k)) is int for k in ("beforeEpoch", "changedEpoch", "restoredEpoch")), "Start account remount lost saved pair/fresh owner")
    else: require(guard is None, "Unexpected start account replacement proof")
    operation = None
    for ordinal, stage in enumerate(stages):
        name = stage["stage"]; typed.labels(stage, expected, recovery=True)
        require(stage.get("mode") == phase and stage.get("splitMode") == mode and stage.get("corpus") == corpus and stage.get("typing") == typing
                and type(stage.get("ordinal")) is int and stage["ordinal"] == ordinal and all(stage.get(k) is True for k in ("ok", "noSession", "noTransport"))
                and type(stage.get("dispatches")) is int and stage["dispatches"] == 0 and stage.get("ownerValid") is (name != "account-changed"), "Start recovery stage owner/dispatch changed")
        changed = name == "account-changed"; restored = phase == "review" and ordinal > STAGES[phase].index("account-changed"); owner = stage.get("owner", {})
        require(owner == dict(account=review.OTHER_ACCOUNT if changed else review.ACCOUNT, epoch=epoch + (1 if changed else 2 if restored else 0), workspace="T1", user="U1")
                and type(owner.get("epoch")) is int, "Start owner reset across remount")
        canonical(stage.get("canonical"), original, changed=name == "stale")
        preview(stage.get("preview"), name, viewport, expected)
        if name in ("review-controls", "review-confirmation", "retry-confirmation"):
            review.controls(stage.get("reviewGeometry"), name != "review-controls", viewport)
        else: require(stage.get("reviewGeometry") is None, "Unexpected typed control proof")
        journal = stage.get("journal", {}); archived = name in ("removed", "refreshed", "archive-restored")
        require(type(journal.get("archiveCount")) is int and journal["archiveCount"] == int(archived)
                and journal.get("archivedExact") is (None if phase == "reviewed" else archived), "Start unknown-outcome archive changed")
        active = journal.get("active")
        if archived: require(active is None, "Start archive retained active lock")
        else:
            require(isinstance(active, dict) and set(active) == {"operation", "checkpoint"} and active.get("checkpoint") == checkpoint
                    and isinstance(active.get("operation"), str) and active["operation"], "Start exact v9 whole ordered pair lost")
            if operation is None: operation = active["operation"]
            require(active["operation"] == operation, "Start review replaced original operation")
    require(terminal.get("canonical") == original == stages[-1]["canonical"] and terminal.get("journal") == stages[-1]["journal"], "Start recovery terminal differs from original last stage")
    return child
