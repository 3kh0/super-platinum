"""Inert v4 end-pair review; no saved request replay or inferred acceptance."""
import base64
import math

import canvas_end_split_oracle as end
import canvas_split_oracle as shared
import canvas_split_recovery_oracle as review
from canvas_heading import require


STAGES = review.STAGES


def retained_pair(checkpoint, original, corpus="ascii"):
    rows = end.inventory(original, corpus=corpus)
    require(isinstance(checkpoint, dict), "Missing inert end checkpoint")
    action = checkpoint.get("action", {}).get("End", {})
    retained = dict(documentData=base64.b64encode(shared.byte_vector(checkpoint.get("request"))).decode(),
                    parent=checkpoint.get("target"), child=action.get("child"), session=checkpoint.get("session"),
                    sequence=checkpoint.get("sequence"), formTitle="Fixture title", httpFactoryExecuted=False)
    child, _, _ = end.end_write(retained, rows["paragraph"], corpus)
    end.journal(dict(scope=shared.SCOPE, receipt=dict(operation="retained-data", checkpoint=checkpoint)), retained, original, corpus)
    return child


def preview(value, name, viewport, corpus="ascii"):
    expected = end.corpus_config(corpus)
    require(isinstance(value, dict) and value.get("ok") is True and value.get("editorAbsent") is True
            and value.get("viewport") == viewport and type(value.get("reviewActionPresent")) is bool, "End review not read-only/native")
    panel = name in ("review-pair", "review-controls", "stale", "review-confirmation", "storage-failed", "retry-confirmation")
    require(value.get("panel") is panel, "End review panel presence changed")
    if name == "account-changed": require(value["reviewActionPresent"] is False, "End review survived account remount")
    if not panel:
        require(all(value.get(k) == [] for k in ("split", "current", "missing"))
                and all(value.get(k) is None for k in ("checked", "checkboxDisabled", "cancelDisabled", "submitDisabled", "error", "warning")),
                "Closed end review retained old controls/content")
        return
    require(value.get("split") == [dict(kind="original", text=expected.seed), dict(kind="parent", text=expected.seed), dict(kind="child", text="")],
            "End review changed original parent or invented child content")
    require(value.get("current") == [dict(id="title", deleted=False, text="Fixture title"),
                                     dict(id="paragraph", deleted=False, text=expected.seed), dict(id="neighbor", deleted=False, text="Another paragraph")]
            and all(row.get("deleted") is False for row in value["current"]) and value.get("missing") == ["child"], "End review current comparison incomplete")
    warning = value.get("warning")
    require(isinstance(warning, str) and all(text in warning for text in review.WARNINGS)
            and "Add an empty paragraph after this paragraph" in warning and "Empty paragraph" in warning,
            "End review lost empty-child explanation or unknown-outcome warning")
    checked = name not in ("review-pair", "review-controls")
    disabled = name in ("stale", "storage-failed")
    require(value.get("checked") is checked and value.get("checkboxDisabled") is disabled
            and value.get("submitDisabled") is (disabled or not checked) and value.get("cancelDisabled") is False
            and value.get("error") == (review.STORAGE if name == "storage-failed" else review.CHANGED if disabled else None), "End review confirmation/failure controls changed")


def validate(mode, stages, terminal, checkpoint, corpus="ascii"):
    end.corpus_config(corpus)
    require(mode in STAGES and isinstance(stages, list) and [s.get("stage") for s in stages] == list(STAGES[mode]), "End recovery stages missing/reordered")
    require(terminal.get("mode") == mode and terminal.get("splitMode") == "end" and terminal.get("corpus") == corpus
            and all(terminal.get(k) is True for k in ("ok", "noSession", "noTransport", "noReplay", "outcomeUnknown", "canonicalUnchanged")), "End recovery isolation/mode changed")
    for key, count in dict(dispatches=0, reservations=int(mode == "seed"), retirements=0).items():
        require(type(terminal.get(key)) is int and terminal[key] == count, "End recovery resumed or sealed saved intent")
    layout = terminal.get("layout", {}); viewport = layout.get("viewport")
    require(layout.get("ok") is True and isinstance(viewport, list) and len(viewport) == 2
            and all(type(n) in (int, float) and math.isfinite(n) and n > 0 for n in viewport), "End recovery layout absent")
    original = stages[0].get("canonical"); child = retained_pair(checkpoint, original, corpus)
    epoch = stages[0].get("owner", {}).get("epoch")
    require(type(epoch) is int and epoch >= 0, "End recovery owner epoch absent")
    guard = terminal.get("accountGuard")
    if mode == "review":
        require(isinstance(guard, dict) and all(type(guard.get(k)) is int and guard[k] > 0 for k in ("beforeRead", "restoredRead"))
                and guard["beforeRead"] != guard["restoredRead"], "End review reused old read after account change")
        require(guard == dict(beforeEpoch=epoch, changedEpoch=epoch + 1, changedAccount=review.OTHER_ACCOUNT,
                              oldReviewRemoved=True, oldReviewActionAbsent=True, receiptPreserved=True,
                              restoredEpoch=epoch + 2, restoredAccount=review.ACCOUNT, beforeRead=guard["beforeRead"], restoredRead=guard["restoredRead"])
                and all(guard.get(k) is True for k in ("oldReviewRemoved", "oldReviewActionAbsent", "receiptPreserved"))
                and all(type(guard.get(k)) is int for k in ("beforeEpoch", "changedEpoch", "restoredEpoch")), "End account remount lost fresh owner or saved pair")
    else: require(guard is None, "Unexpected end account replacement proof")
    operation = None
    for ordinal, stage in enumerate(stages):
        name = stage["stage"]
        require(stage.get("mode") == mode and stage.get("splitMode") == "end" and stage.get("corpus") == corpus
                and type(stage.get("ordinal")) is int and stage["ordinal"] == ordinal
                and all(stage.get(k) is True for k in ("ok", "noSession", "noTransport"))
                and type(stage.get("dispatches")) is int and stage["dispatches"] == 0
                and stage.get("ownerValid") is (name != "account-changed"), "End recovery stage owner/dispatch changed")
        changed = name == "account-changed"; restored = mode == "review" and ordinal > STAGES[mode].index("account-changed")
        owner = stage.get("owner", {})
        require(owner == dict(account=review.OTHER_ACCOUNT if changed else review.ACCOUNT,
                              epoch=epoch + (1 if changed else 2 if restored else 0), workspace="T1", user="U1")
                and type(owner.get("epoch")) is int, "End owner reset across remount")
        review.canonical(stage.get("canonical"), original, changed=name == "stale")
        preview(stage.get("preview"), name, viewport, corpus)
        if name in ("review-controls", "review-confirmation", "retry-confirmation"):
            review.controls(stage.get("reviewGeometry"), name != "review-controls", viewport)
        else: require(stage.get("reviewGeometry") is None, "Unexpected end control proof")
        journal = stage.get("journal", {}); archived = name in ("removed", "refreshed", "archive-restored")
        require(type(journal.get("archiveCount")) is int and journal["archiveCount"] == int(archived)
                and journal.get("archivedExact") is (None if mode == "reviewed" else archived), "End unknown-outcome archive changed")
        active = journal.get("active")
        if archived: require(active is None, "End archive retained an active lock")
        else:
            require(isinstance(active, dict) and set(active) == {"operation", "checkpoint"} and active.get("checkpoint") == checkpoint
                    and isinstance(active.get("operation"), str) and active["operation"], "End exact whole-pair receipt lost")
            if operation is None: operation = active["operation"]
            require(active["operation"] == operation, "End review replaced retained operation")
    require(terminal.get("canonical") == original == stages[-1]["canonical"] and terminal.get("journal") == stages[-1]["journal"], "End recovery terminal differs from original last stage")
    return child
