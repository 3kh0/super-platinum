"""Inert v5 existing-pair recovery; no insertion, replay or remote success claim."""
import base64
import math

import canvas_end_reuse_oracle as reuse
import canvas_end_split_oracle as end
import canvas_split_oracle as shared
import canvas_reuse_base as bases
import canvas_split_recovery_oracle as review
from canvas_heading import require, decoded, field

STAGES = review.STAGES


def retained_pair(checkpoint, original, corpus="ascii", reuse_base="positive"):
    rows = reuse.inventory(original, corpus=corpus, reuse_base=reuse_base)
    require(isinstance(checkpoint, dict), "Missing v5 reuse checkpoint")
    action = checkpoint.get("action", {}).get("EndReuse", {})
    retained = dict(kind="reuse", documentData=base64.b64encode(shared.byte_vector(checkpoint.get("request"))).decode(),
                    parent=checkpoint.get("target"), child=action.get("child"), session=checkpoint.get("session"),
                    sequence=checkpoint.get("sequence"), formTitle="Fixture title", httpFactoryExecuted=False)
    child, _, _ = reuse.reuse_write(retained, rows, corpus, reuse_base)
    reuse.journal(dict(scope=shared.SCOPE, receipt=dict(operation="retained-data", checkpoint=checkpoint)), retained, original, corpus, reuse_base)
    return child


def canonical(value, original, changed=False):
    require(isinstance(value, dict) and set(value) == set(original)
            and all(value.get(k) == original[k] for k in original if k != "sections"), "Reuse recovery raw owner/document changed")
    rows = value.get("sections")
    require(isinstance(rows, list) and len(rows) == 4, "Reuse recovery lost existing inventory")
    for current, prior in zip(rows, original["sections"]):
        require(current.get("id") == prior["id"], "Reuse recovery reordered identities")
        if changed and prior["id"] == "neighbor":
            now, before = decoded(current.get("raw")), decoded(prior["raw"])
            require(type(current.get("sequence")) is int and current["sequence"] == field(now, 2, 0) == 2000
                    and shared.stable(now, (2,)) == shared.stable(before, (2,)), "Reuse review stale fixture changed beyond neighbor revision")
        else: require(current == prior, "Reuse recovery changed canonical content/history")


def preview(value, name, viewport, corpus="ascii", reuse_base="positive"):
    cohort = bases.config(reuse_base)
    expected = end.corpus_config(corpus)
    require(isinstance(value, dict) and value.get("ok") is True and value.get("editorAbsent") is True
            and value.get("viewport") == viewport and type(value.get("reviewActionPresent")) is bool, "Reuse review not native/read-only")
    panel = name in ("review-pair", "review-controls", "stale", "review-confirmation", "storage-failed", "retry-confirmation")
    require(value.get("panel") is panel, "Reuse review panel presence changed")
    if name == "account-changed": require(value["reviewActionPresent"] is False, "Old reuse review action survived account change")
    if not panel:
        require(all(value.get(k) == [] for k in ("split", "current", "missing"))
                and all(value.get(k) is None for k in ("checked", "checkboxDisabled", "cancelDisabled", "submitDisabled", "error", "warning")),
                "Closed reuse review retained old controls/content")
        return
    require(value.get("split") == [dict(kind="original", text=expected.seed), dict(kind="parent", text=expected.seed), dict(kind="child", text="")], "Reuse intended pair changed original text/empty child")
    require(value.get("current") == [dict(id="title", deleted=False, text="Fixture title"), dict(id="neighbor", deleted=False, text="Another paragraph"),
            dict(id="paragraph", deleted=False, text=expected.seed), dict(id=cohort.child, deleted=False, text="")]
            and all(row.get("deleted") is False for row in value["current"]) and value.get("missing") == [], "Reuse review omitted/duplicated/misrepresented existing empty child")
    warning = value.get("warning")
    require(isinstance(warning, str) and all(text in warning for text in review.WARNINGS)
            and "Reuse the existing empty paragraph after this paragraph" in warning and warning.count("Empty paragraph") == 2,
            "Reuse review lost existing-child explanation/empty markers/unknown outcome")
    checked = name not in ("review-pair", "review-controls"); disabled = name in ("stale", "storage-failed")
    require(value.get("checked") is checked and value.get("checkboxDisabled") is disabled and value.get("submitDisabled") is (disabled or not checked)
            and value.get("cancelDisabled") is False and value.get("error") == (review.STORAGE if name == "storage-failed" else review.CHANGED if disabled else None),
            "Reuse local decision/failure controls changed")


def validate(mode, stages, terminal, checkpoint, corpus="ascii", reuse_base="positive"):
    end.corpus_config(corpus)
    cohort = bases.config(reuse_base); bases.labels(terminal, cohort)
    require(mode in STAGES and isinstance(stages, list) and [s.get("stage") for s in stages] == list(STAGES[mode]), "Reuse recovery stages missing/reordered")
    require(terminal.get("mode") == mode and terminal.get("splitMode") == "reuse" and terminal.get("corpus") == corpus
            and all(terminal.get(k) is True for k in ("ok", "noSession", "noTransport", "noReplay", "outcomeUnknown", "canonicalUnchanged")), "Reuse recovery mode/isolation changed")
    for key, count in dict(dispatches=0, reservations=int(mode == "seed"), retirements=0).items():
        require(type(terminal.get(key)) is int and terminal[key] == count, "Reuse recovery resumed/sealed retained edit")
    layout = terminal.get("layout", {}); viewport = layout.get("viewport")
    require(layout.get("ok") is True and isinstance(viewport, list) and len(viewport) == 2
            and all(type(n) in (int, float) and math.isfinite(n) and n > 0 for n in viewport), "Reuse recovery layout absent")
    original = stages[0].get("canonical"); child = retained_pair(checkpoint, original, corpus, reuse_base)
    epoch = stages[0].get("owner", {}).get("epoch")
    require(type(epoch) is int and epoch >= 0, "Reuse recovery owner epoch absent")
    guard = terminal.get("accountGuard")
    if mode == "review":
        require(isinstance(guard, dict) and all(type(guard.get(k)) is int and guard[k] > 0 for k in ("beforeRead", "restoredRead"))
                and guard["beforeRead"] != guard["restoredRead"], "Reuse account return recycled review read")
        require(guard == dict(beforeEpoch=epoch, changedEpoch=epoch + 1, changedAccount=review.OTHER_ACCOUNT,
                              oldReviewRemoved=True, oldReviewActionAbsent=True, receiptPreserved=True, restoredEpoch=epoch + 2, restoredAccount=review.ACCOUNT,
                              beforeRead=guard["beforeRead"], restoredRead=guard["restoredRead"])
                and all(guard.get(k) is True for k in ("oldReviewRemoved", "oldReviewActionAbsent", "receiptPreserved"))
                and all(type(guard.get(k)) is int for k in ("beforeEpoch", "changedEpoch", "restoredEpoch")), "Reuse account remount lost saved pair/fresh owner")
    else: require(guard is None, "Unexpected reuse account replacement proof")
    operation = None
    for ordinal, stage in enumerate(stages):
        name = stage["stage"]; bases.labels(stage, cohort)
        require(stage.get("mode") == mode and stage.get("splitMode") == "reuse" and stage.get("corpus") == corpus
                and type(stage.get("ordinal")) is int and stage["ordinal"] == ordinal and all(stage.get(k) is True for k in ("ok", "noSession", "noTransport"))
                and type(stage.get("dispatches")) is int and stage["dispatches"] == 0 and stage.get("ownerValid") is (name != "account-changed"), "Reuse recovery stage owner/dispatch changed")
        changed = name == "account-changed"; restored = mode == "review" and ordinal > STAGES[mode].index("account-changed"); owner = stage.get("owner", {})
        require(owner == dict(account=review.OTHER_ACCOUNT if changed else review.ACCOUNT, epoch=epoch + (1 if changed else 2 if restored else 0), workspace="T1", user="U1")
                and type(owner.get("epoch")) is int, "Reuse owner reset across remount")
        canonical(stage.get("canonical"), original, changed=name == "stale")
        preview(stage.get("preview"), name, viewport, corpus, reuse_base)
        if name in ("review-controls", "review-confirmation", "retry-confirmation"):
            review.controls(stage.get("reviewGeometry"), name != "review-controls", viewport)
        else: require(stage.get("reviewGeometry") is None, "Unexpected reuse control proof")
        journal = stage.get("journal", {}); archived = name in ("removed", "refreshed", "archive-restored")
        require(type(journal.get("archiveCount")) is int and journal["archiveCount"] == int(archived)
                and journal.get("archivedExact") is (None if mode == "reviewed" else archived), "Reuse unknown-outcome archive changed")
        active = journal.get("active")
        if archived: require(active is None, "Reuse archive retained active lock")
        else:
            require(isinstance(active, dict) and set(active) == {"operation", "checkpoint"} and active.get("checkpoint") == checkpoint
                    and isinstance(active.get("operation"), str) and active["operation"], "Reuse exact v5 whole pair lost")
            if operation is None: operation = active["operation"]
            require(active["operation"] == operation, "Reuse review replaced original operation")
    require(terminal.get("canonical") == original == stages[-1]["canonical"] and terminal.get("journal") == stages[-1]["journal"], "Reuse recovery terminal differs from original last stage")
    return child
