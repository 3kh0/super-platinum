"""Portable validation of inert split recovery, not remote outcome acceptance.

The root runner separately binds original closed receipts to actual SQLite rows.
Opaque CRDT relationships use the accepted split parser; Automerge semantics are
checked by core/product tests, not reconstructed by this Python oracle.
"""
import base64
import math

import canvas_split_oracle as split
from canvas_heading import require, decoded, field
from canvas_split_corpus import corpus_config, require_corpus

STAGES = {
    "seed": ("seeded",),
    "restart": ("restored", "reopened"),
    "review": ("review-pair", "review-controls", "cancelled", "stale", "account-changed", "review-confirmation", "removed", "refreshed"),
    "storage-failure": ("review-pair", "review-confirmation", "storage-failed", "retry-confirmation", "removed", "refreshed"),
    "reviewed": ("archive-restored",),
}
CHANGED = "This canvas changed. Cancel and refresh before reviewing again."
STORAGE = "The local record could not be updated. It remains locked. Refresh to check its state."
ACCOUNT = "00000000-0000-4000-8000-000000009994"
OTHER_ACCOUNT = "00000000-0000-4000-8000-000000009993"
WARNINGS = (
    "Either part, both parts, or neither part may have reached Slack. Removing this lock leaves both outcomes unknown.",
    "Matching text does not establish whether the saved change succeeded.",
    "Removing this local edit lock does not confirm success, undo a change, or send it again.",
    "The saved record will remain on this computer with an unknown outcome.",
)


def retained_pair(checkpoint, original, corpus="ascii", parent_mode="bootstrap"):
    """Inspect the retained request as data, never as a request to dispatch."""
    split.inventory(original, corpus=corpus)
    require(isinstance(checkpoint, dict), "Missing original durable split checkpoint")
    action = checkpoint.get("action", {}).get("Split", {})
    retained = dict(documentData=base64.b64encode(split.byte_vector(checkpoint.get("request"))).decode(),
                    parent=checkpoint.get("target"), child=action.get("child"),
                    session=checkpoint.get("session"), sequence=checkpoint.get("sequence"),
                    formTitle="Fixture title", httpFactoryExecuted=False)
    child, parent, _, lineage = split.split_write(retained, corpus)
    split.parent_source_binding(split.inventory(original,corpus=corpus)["paragraph"],parent,lineage,parent_mode)
    split.journal(dict(scope=split.SCOPE, receipt=dict(operation="retained-data", checkpoint=checkpoint)), retained, original, corpus)
    return child


def canonical(value, original, changed=False):
    """Only the explicitly injected neighbor revision may differ from source."""
    require(isinstance(value, dict) and set(value) == set(original), "Recovery inventory shape changed")
    require(all(value.get(k) == original[k] for k in original if k != "sections"), "Recovery document owner/raw bytes changed")
    rows = value.get("sections")
    require(isinstance(rows, list) and len(rows) == 3, "Recovery canonical inventory incomplete")
    for current, prior in zip(rows, original["sections"]):
        require(current.get("id") == prior["id"], "Recovery canonical order changed")
        if changed and prior["id"] == "neighbor":
            now, before = decoded(current.get("raw")), decoded(prior["raw"])
            require(type(current.get("sequence")) is int and current["sequence"] == field(now, 2, 0) == 2000
                    and split.stable(now, (2,)) == split.stable(before, (2,)), "Stale comparison changed more than neighbor revision")
        else:
            require(current == prior, "Recovery changed canonical content or raw inventory")


def controls(proof, checked, viewport):
    require(isinstance(proof, dict) and proof.get("ok") is True and proof.get("viewport") == viewport,
            "Missing measured recovery controls")
    for key, expected in dict(checked=checked, submitDisabled=not checked, cancelDisabled=False, checkboxDisabled=False).items():
        require(proof.get(key) is expected, "Recovery confirmation controls changed")
    require(isinstance(viewport, list) and len(viewport) == 2
            and all(type(n) in (int, float) and math.isfinite(n) and n > 0 for n in viewport), "Invalid review viewport")
    for name in ("checkbox", "label", "cancel", "remove"):
        item = proof.get(name, {})
        require(item.get("visible") is True and item.get("hit") is True and item.get("hitTested") is (name != "label"),
                "Review control obscured or not hit tested")
        rect, clip, point = (item.get(k) for k in ("rect", "clip", "point"))
        for values, size in ((rect, 4), (clip, 4), (point, 2)):
            require(isinstance(values, list) and len(values) == size
                    and all(type(n) in (int, float) and math.isfinite(n) for n in values), "Malformed recovery control geometry")
        require(0 <= clip[0] <= rect[0] < rect[2] <= clip[2] <= viewport[0]
                and 0 <= clip[1] <= rect[1] < rect[3] <= clip[3] <= viewport[1]
                and point == [(rect[0] + rect[2]) / 2, (rect[1] + rect[3]) / 2], "Recovery control clipped/outside viewport")


def preview(value, name, viewport, corpus="ascii"):
    expected = corpus_config(corpus)
    require(isinstance(value, dict) and value.get("ok") is True and value.get("editorAbsent") is True and value.get("viewport") == viewport,
            "Missing native read-only recovery view")
    panel = name in ("review-pair", "review-controls", "stale", "review-confirmation", "storage-failed", "retry-confirmation")
    require(value.get("panel") is panel, "Recovery panel presence changed")
    require(type(value.get("reviewActionPresent")) is bool, "Missing native review action observation")
    if name == "account-changed":
        require(value["reviewActionPresent"] is False, "Old account retained review action after remount")
    if not panel:
        require(all(value.get(k) == [] for k in ("split", "current", "missing"))
                and all(value.get(k) is None for k in ("checked", "checkboxDisabled", "cancelDisabled", "submitDisabled", "error", "warning")),
                "Closed review retained draft controls/content")
        return
    require(value.get("split") == [dict(kind="original", text=expected.seed), dict(kind="parent", text=expected.left), dict(kind="child", text=expected.right)],
            "Saved pair preview lost original paragraph or one half")
    require(value.get("current") == [dict(id="title", deleted=False, text="Fixture title"),
                                     dict(id="paragraph", deleted=False, text=expected.seed),
                                     dict(id="neighbor", deleted=False, text="Another paragraph")]
            and all(row.get("deleted") is False for row in value["current"])
            and value.get("missing") == ["child"], "Current read-only comparison incomplete or substituted saved halves")
    warning = value.get("warning")
    require(isinstance(warning, str) and all(text in warning for text in WARNINGS), "Review lost unknown-outcome warning")
    checked = name not in ("review-pair", "review-controls")
    disabled = name in ("stale", "storage-failed")
    require(value.get("checked") is checked and value.get("checkboxDisabled") is disabled
            and value.get("submitDisabled") is (disabled or not checked) and value.get("cancelDisabled") is False,
            "Recovery controls permit an unacknowledged/stale removal or block Cancel")
    require(value.get("error") == (STORAGE if name == "storage-failed" else CHANGED if disabled else None),
            "Recovery failure state changed")


def validate(mode, stages, terminal, checkpoint, corpus="ascii", parent_mode="bootstrap"):
    split.parent_mode_label(terminal,parent_mode)
    corpus_config(corpus)
    require_corpus(terminal, corpus)
    require(mode in STAGES and isinstance(stages, list) and [s.get("stage") for s in stages] == list(STAGES[mode]),
            "Missing/reordered original recovery stages")
    require(isinstance(terminal, dict) and terminal.get("mode") == mode
            and all(terminal.get(k) is True for k in ("ok", "noSession", "noTransport", "noReplay", "outcomeUnknown", "canonicalUnchanged")),
            "Recovery terminal failed isolation or unknown-outcome contract")
    for key, count in dict(dispatches=0, reservations=int(mode == "seed"), retirements=0).items():
        require(type(terminal.get(key)) is int and terminal[key] == count, "Recovery replayed/reserved/retired a saved edit")
    layout = terminal.get("layout", {})
    viewport = layout.get("viewport")
    require(layout.get("ok") is True and isinstance(viewport, list) and len(viewport) == 2
            and all(type(n) in (int, float) and math.isfinite(n) and n > 0 for n in viewport), "Recovery terminal layout missing")
    original = stages[0].get("canonical")
    child = retained_pair(checkpoint, original, corpus, parent_mode)
    epoch = stages[0].get("owner", {}).get("epoch")
    require(type(epoch) is int and epoch >= 0, "Missing original account epoch")
    guard = terminal.get("accountGuard")
    if mode == "review":
        require(isinstance(guard, dict)
                and all(type(guard.get(k)) is int and guard[k] > 0 for k in ("beforeRead", "restoredRead"))
                and guard["beforeRead"] != guard["restoredRead"], "Restored account reused the old review read")
        require(guard == dict(beforeEpoch=epoch, changedEpoch=epoch + 1, changedAccount=OTHER_ACCOUNT,
                              oldReviewRemoved=True, oldReviewActionAbsent=True, receiptPreserved=True,
                              restoredEpoch=epoch + 2, restoredAccount=ACCOUNT,
                              beforeRead=guard["beforeRead"], restoredRead=guard["restoredRead"])
                and all(guard.get(k) is True for k in ("oldReviewRemoved", "oldReviewActionAbsent", "receiptPreserved"))
                and all(type(guard.get(k)) is int for k in ("beforeEpoch", "changedEpoch", "restoredEpoch")),
                "Account remount did not discard old review or restore a fresh monotonic owner")
    else:
        require(guard is None, "Unexpected account replacement proof")
    operation = None
    for ordinal, stage in enumerate(stages):
        require_corpus(stage, corpus)
        split.parent_mode_label(stage,parent_mode)
        name = stage["stage"]
        require(stage.get("mode") == mode and type(stage.get("ordinal")) is int and stage["ordinal"] == ordinal
                and all(stage.get(k) is True for k in ("ok", "noSession", "noTransport"))
                and type(stage.get("dispatches")) is int and stage["dispatches"] == 0
                and stage.get("ownerValid") is (name != "account-changed"), "Recovery stage owner/order or dispatch count changed")
        changed = name == "account-changed"
        restored = mode == "review" and ordinal > STAGES[mode].index("account-changed")
        owner = stage.get("owner", {})
        require(owner == dict(account=OTHER_ACCOUNT if changed else ACCOUNT,
                              epoch=epoch + (1 if changed else 2 if restored else 0), workspace="T1", user="U1")
                and type(owner.get("epoch")) is int, "Recovery owner was reset or replaced across account remount")
        canonical(stage.get("canonical"), original, changed=name == "stale")
        preview(stage.get("preview"), name, viewport, corpus)
        measured = name in ("review-controls", "review-confirmation", "retry-confirmation")
        if measured:
            controls(stage.get("reviewGeometry"), name != "review-controls", viewport)
        else:
            require(stage.get("reviewGeometry") is None, "Unexpected recovery control proof")
        journal = stage.get("journal", {})
        archived = name in ("removed", "refreshed", "archive-restored")
        require(type(journal.get("archiveCount")) is int and journal["archiveCount"] == int(archived)
                and journal.get("archivedExact") is (None if mode == "reviewed" else archived), "Recovery archive decision/custody changed")
        active = journal.get("active")
        if archived:
            require(active is None, "Archived local decision retained active lock")
        else:
            require(isinstance(active, dict) and set(active) == {"operation", "checkpoint"}
                    and active.get("checkpoint") == checkpoint and isinstance(active.get("operation"), str)
                    and active["operation"], "Recovery lost exact inert saved pair")
            if operation is None:
                operation = active["operation"]
            require(active["operation"] == operation, "Review replaced original durable operation")
    require(terminal.get("canonical") == original == stages[-1]["canonical"]
            and terminal.get("journal") == stages[-1]["journal"], "Terminal differs from original last recovery observation")
    return child
