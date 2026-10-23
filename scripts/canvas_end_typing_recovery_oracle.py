"""Inert v6 ordered child-history recovery; exact pair, no replay or success claim."""
import base64
import math

import canvas_end_typing_oracle as typed
import canvas_end_split_oracle as end
import canvas_split_oracle as shared
import canvas_split_recovery_oracle as review
from canvas_heading import require, decoded, field

STAGES = review.STAGES


def retained_pair(checkpoint, original, corpus="ascii", mode="end", typing="correction", reuse_base="positive"):
    expected = typed.config(corpus, mode, typing, reuse_base)
    rows = typed.inventory(original, expected)
    require(isinstance(checkpoint, dict), "Missing inert typed checkpoint")
    action = checkpoint.get("action", {}).get("EndTyping", {})
    retained = dict(kind=mode, documentData=base64.b64encode(shared.byte_vector(checkpoint.get("request"))).decode(),
                    parent=checkpoint.get("target"), child=action.get("child"), session=checkpoint.get("session"),
                    sequence=checkpoint.get("sequence"), formTitle="Fixture title", httpFactoryExecuted=False)
    child, _, _ = typed.typed_write(retained, rows, expected)
    typed.journal(dict(scope=shared.SCOPE, receipt=dict(operation="retained-data", checkpoint=checkpoint)), expected, retained, original)
    return child


def canonical(value, original, changed=False):
    require(isinstance(value, dict) and set(value) == set(original)
            and all(value.get(k) == original[k] for k in original if k != "sections"), "Typed recovery raw owner/document changed")
    rows = value.get("sections")
    require(isinstance(rows, list) and len(rows) == len(original["sections"]), "Typed recovery lost existing inventory")
    for current, prior in zip(rows, original["sections"]):
        require(current.get("id") == prior["id"], "Typed recovery reordered identities")
        if changed and prior["id"] == "neighbor":
            now, before = decoded(current.get("raw")), decoded(prior["raw"])
            require(type(current.get("sequence")) is int and current["sequence"] == field(now, 2, 0) == 2000
                    and shared.stable(now, (2,)) == shared.stable(before, (2,)), "Typed review stale fixture changed beyond neighbor revision")
        else: require(current == prior, "Typed recovery changed canonical content/history")


def preview(value, name, viewport, expected):
    require(isinstance(value, dict) and value.get("ok") is True and value.get("editorAbsent") is True
            and value.get("viewport") == viewport and type(value.get("reviewActionPresent")) is bool, "Typed review not native/read-only")
    panel = name in ("review-pair", "review-controls", "stale", "review-confirmation", "storage-failed", "retry-confirmation")
    require(value.get("panel") is panel, "Typed review panel presence changed")
    if name == "account-changed": require(value["reviewActionPresent"] is False, "Old typed review action survived account change")
    if not panel:
        require(all(value.get(k) == [] for k in ("split", "current", "missing"))
                and all(value.get(k) is None for k in ("checked", "checkboxDisabled", "cancelDisabled", "submitDisabled", "error", "warning")),
                "Closed typed review retained old controls/content")
        return
    require(value.get("split") == [dict(kind="original", text=expected.seed), dict(kind="parent", text=expected.seed),
            dict(kind="child", text=expected.updates[-1])], "Typed intended pair differs from actual ordered checkpoint draft")
    current = [dict(id="title", deleted=False, text="Fixture title")]
    parent = dict(id="paragraph", deleted=False, text=expected.seed); neighbor = dict(id="neighbor", deleted=False, text="Another paragraph")
    current += [neighbor, parent, dict(id=expected.child, deleted=False, text="")] if expected.mode == "reuse" else [parent, neighbor]
    require(value.get("current") == current and all(row.get("deleted") is False for row in value["current"])
            and value.get("missing") == ([] if expected.mode == "reuse" else ["child"]), "Typed recovery current inventory incomplete")
    nonempty = bool(expected.updates[-1]); reused = expected.mode == "reuse"
    action = ("Reuse and edit the existing empty paragraph after this paragraph" if nonempty else "Reuse the existing empty paragraph after this paragraph") if reused else (
              "Add a paragraph after this paragraph" if nonempty else "Add an empty paragraph after this paragraph")
    warning = value.get("warning")
    require(isinstance(warning, str) and all(text in warning for text in review.WARNINGS) and action in warning
            and warning.count("Empty paragraph") == int(reused) + int(not nonempty), "Typed review lost intended/current empty distinction or unknown outcome")
    checked = name not in ("review-pair", "review-controls"); disabled = name in ("stale", "storage-failed")
    require(value.get("checked") is checked and value.get("checkboxDisabled") is disabled and value.get("submitDisabled") is (disabled or not checked)
            and value.get("cancelDisabled") is False and value.get("error") == (review.STORAGE if name == "storage-failed" else review.CHANGED if disabled else None),
            "Typed local decision/failure controls changed")


def validate(phase, stages, terminal, checkpoint, corpus="ascii", mode="end", typing="correction", reuse_base="positive"):
    expected = typed.config(corpus, mode, typing, reuse_base)
    typed.labels(terminal, expected, recovery=True)
    require(phase in STAGES and isinstance(stages, list) and [s.get("stage") for s in stages] == list(STAGES[phase]), "Typed recovery stages missing/reordered")
    require(terminal.get("mode") == phase and terminal.get("splitMode") == mode and terminal.get("corpus") == corpus and terminal.get("typing") == typing
            and all(terminal.get(k) is True for k in ("ok", "noSession", "noTransport", "noReplay", "outcomeUnknown", "canonicalUnchanged")), "Typed recovery mode/isolation changed")
    for key, count in dict(dispatches=0, reservations=int(phase == "seed"), retirements=0).items():
        require(type(terminal.get(key)) is int and terminal[key] == count, "Typed recovery resumed/sealed retained edit")
    layout = terminal.get("layout", {}); viewport = layout.get("viewport")
    require(layout.get("ok") is True and isinstance(viewport, list) and len(viewport) == 2
            and all(type(n) in (int, float) and math.isfinite(n) and n > 0 for n in viewport), "Typed recovery layout absent")
    original = stages[0].get("canonical"); child = retained_pair(checkpoint, original, corpus, mode, typing, reuse_base)
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
        else: require(stage.get("reviewGeometry") is None, "Unexpected typed control proof")
        journal = stage.get("journal", {}); archived = name in ("removed", "refreshed", "archive-restored")
        require(type(journal.get("archiveCount")) is int and journal["archiveCount"] == int(archived)
                and journal.get("archivedExact") is (None if phase == "reviewed" else archived), "Typed unknown-outcome archive changed")
        active = journal.get("active")
        if archived: require(active is None, "Typed archive retained active lock")
        else:
            require(isinstance(active, dict) and set(active) == {"operation", "checkpoint"} and active.get("checkpoint") == checkpoint
                    and isinstance(active.get("operation"), str) and active["operation"], "Typed exact v6 whole ordered pair lost")
            if operation is None: operation = active["operation"]
            require(active["operation"] == operation, "Typed review replaced original operation")
    require(terminal.get("canonical") == original == stages[-1]["canonical"] and terminal.get("journal") == stages[-1]["journal"], "Typed recovery terminal differs from original last stage")
    return child
