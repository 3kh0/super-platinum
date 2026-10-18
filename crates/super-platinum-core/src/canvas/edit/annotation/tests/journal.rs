//! Real temporary SQLite transitions, never the user's journal or Slack.
use super::*;
use crate::canvas::comments::bootstrap::BootstrapIntent;
use crate::canvas::edit::structure::recovery::journal::{Journal, annotations::*};
const OWNER: &str = "147b5480-9f1a-4443-89da-836472cbb209";
struct Temp(std::path::PathBuf);
impl Temp {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!("canvas-annotation-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn path(&self) -> &std::path::Path {
        &self.0
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(self.0.join("annotation.sqlite"));
        let _ = std::fs::remove_dir(&self.0);
    }
}
fn setup() -> (Temp, Journal, CanvasCollabSnapshot, PreparedAnnotation) {
    let d = Temp::new();
    let j = Journal::open(d.path().join("annotation.sqlite"), OWNER).unwrap();
    let s = fixture();
    let p = prepare(&s);
    (d, j, s, p)
}
fn begin(j: &mut Journal, p: &PreparedAnnotation) -> AnnotationReceipt {
    let AnnotationBegin::Started(r) = j.begin_annotation(p.record().unwrap()).unwrap() else {
        panic!("start")
    };
    r
}
fn verified(
    p: PreparedAnnotation,
    s: &CanvasCollabSnapshot,
) -> (VerifiedAnnotation, CanvasCollabSnapshot) {
    let control = receipt(&p);
    let accepted = p.validate_ack(&ack(&control)).unwrap();
    let mut fresh = s.clone();
    fresh.sections.push(control);
    (accepted.confirm(&fresh).unwrap(), fresh)
}
#[test]
fn started_survives_restart_and_a_read_cannot_authorize_bootstrap() {
    let (d, mut j, s, p) = setup();
    let r = begin(&mut j, &p);
    let scope = r.record().scope().clone();
    drop(j);
    let mut j = Journal::open(d.path().join("annotation.sqlite"), OWNER).unwrap();
    let pending = j.annotation(&scope).unwrap().unwrap();
    assert_eq!(pending.stage(), AnnotationStage::AnchorStarted);
    assert!(
        j.continue_annotation_bootstrap(&pending, &s, 1_700_000_000_001)
            .unwrap()
            .is_none()
    );
    assert!(matches!(
        j.begin_annotation(p.record().unwrap()).unwrap(),
        AnnotationBegin::Blocked(_)
    ));
    assert!(j.annotation_not_sent(&pending).unwrap());
    assert!(j.annotation_not_sent(&pending).unwrap());
    assert!(j.annotation(&scope).unwrap().is_none());
    let history = j.annotation_history(&scope, None).unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].status, "not_sent");
}
#[test]
fn stale_not_sent_cannot_release_a_new_explicit_bootstrap_attempt() {
    let (_d, mut j, s, p) = setup();
    let r = begin(&mut j, &p);
    let (proof, fresh) = verified(p, &s);
    assert!(j.annotation_anchor_confirmed(&r, &proof).unwrap());
    let intent = BootstrapIntent::capture(proof, 1_700_000_000_001).unwrap();
    assert!(j.begin_annotation_bootstrap(&r, &intent).unwrap());
    assert!(!j.begin_annotation_bootstrap(&r, &intent).unwrap());
    let first = j.annotation(r.record().scope()).unwrap().unwrap();
    assert!(j.annotation_not_sent(&first).unwrap());
    assert!(j.annotation_not_sent(&first).unwrap());
    let not_sent = j.annotation(r.record().scope()).unwrap().unwrap();
    let retry = j
        .continue_annotation_bootstrap(&not_sent, &fresh, 1_700_000_000_001)
        .unwrap()
        .unwrap();
    // Even a repeated wall-clock millisecond is a distinct local admission.
    assert_eq!(retry.record().latest(), intent.record().latest());
    assert!(!j.annotation_not_sent(&first).unwrap());
    assert!(
        j.continue_annotation_bootstrap(&not_sent, &fresh, 1_700_000_000_003)
            .unwrap()
            .is_none()
    );
    assert_eq!(
        j.annotation(r.record().scope()).unwrap().unwrap().stage(),
        AnnotationStage::BootstrapStarted
    );
    assert_eq!(
        j.annotation_history(r.record().scope(), None).unwrap()[0].status,
        "bootstrap_not_sent"
    );
}
#[test]
fn root_acceptance_is_exact_and_local_save_retry_is_idempotent() {
    let (d, mut j, s, p) = setup();
    let r = begin(&mut j, &p);
    let (proof, _fresh) = verified(p, &s);
    assert!(j.annotation_anchor_confirmed(&r, &proof).unwrap());
    let intent = BootstrapIntent::capture(proof, 1_700_000_000_001).unwrap();
    assert!(j.begin_annotation_bootstrap(&r, &intent).unwrap());
    let file = &r.record().scope().file;
    let channel = crate::canvas::comments::channel(file).unwrap();
    let raw = serde_json::json!({"ok":true,"has_more":false,"messages":[{"type":"message","channel":channel,"ts":"1700000000.000001","subtype":"document_comment_root","document_comment":{"thread_id":r.record().control_id()},"text":""}]});
    let accepted = intent.accept(&raw).unwrap();
    let pending = j.annotation(r.record().scope()).unwrap().unwrap();
    assert!(
        j.annotation_bootstrap_accepted(&pending, &accepted)
            .unwrap()
    );
    drop(j);
    let mut j = Journal::open(d.path().join("annotation.sqlite"), OWNER).unwrap();
    assert!(
        j.annotation_bootstrap_accepted(&pending, &accepted)
            .unwrap()
    );
    assert!(j.annotation(r.record().scope()).unwrap().is_none());
    let rows = j.annotation_history(r.record().scope(), None).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].status, "root_accepted");
    assert_eq!(rows[0].observed["ts"], "1700000000.000001");
    assert_eq!(
        rows[0].root().unwrap().ts.as_deref(),
        Some("1700000000.000001")
    );
}
#[test]
fn malformed_stored_stage_fails_closed_without_erasing_original_content() {
    let (d, mut j, _s, p) = setup();
    let r = begin(&mut j, &p);
    drop(j);
    let conn = rusqlite::Connection::open(d.path().join("annotation.sqlite")).unwrap();
    let original: String = conn
        .query_row("SELECT body FROM annotation_attempts", [], |row| row.get(0))
        .unwrap();
    let mut changed: serde_json::Value = serde_json::from_str(&original).unwrap();
    changed["stage"] = serde_json::json!("BootstrapStarted");
    conn.execute(
        "UPDATE annotation_attempts SET body=?1",
        [serde_json::to_string(&changed).unwrap()],
    )
    .unwrap();
    drop(conn);
    let j = Journal::open(d.path().join("annotation.sqlite"), OWNER).unwrap();
    assert!(j.annotation(r.record().scope()).is_err());
    assert!(!original.is_empty());
}

#[test]
fn locally_disposed_unknown_retains_late_exact_root_without_touching_new_attempt() {
    let (_d, mut j, s, p) = setup();
    let r = begin(&mut j, &p);
    let (proof, fresh) = verified(p, &s);
    assert!(j.annotation_anchor_confirmed(&r, &proof).unwrap());
    let intent = BootstrapIntent::capture(proof, 1_700_000_000_001).unwrap();
    assert!(j.begin_annotation_bootstrap(&r, &intent).unwrap());
    let pending = j.annotation(r.record().scope()).unwrap().unwrap();
    let observed = r.record().review_current(&fresh).unwrap();
    assert!(
        j.resolve_annotation_locally(
            &pending,
            observed,
            AnnotationDecision::RemoveLocalLockOutcomeUnknown
        )
        .unwrap()
    );
    let channel = crate::canvas::comments::channel(&r.record().scope().file).unwrap();
    let raw = serde_json::json!({"ok":true,"has_more":false,"messages":[{"type":"message","channel":channel,"ts":"1700000000.000001","subtype":"document_comment_root","document_comment":{"thread_id":r.record().control_id()},"text":""}]});
    let accepted = intent.accept(&raw).unwrap();
    let new_p = prepare(&s);
    let new_r = begin(&mut j, &new_p);
    let before = j.annotation(new_r.record().scope()).unwrap().unwrap();
    assert!(
        j.preserve_annotation_root_result(&pending, &accepted)
            .unwrap()
    );
    assert!(
        j.preserve_annotation_root_result(&pending, &accepted)
            .unwrap()
    );
    assert!(j.annotation(new_r.record().scope()).unwrap().unwrap() == before);
    let history = j.annotation_history(r.record().scope(), None).unwrap();
    assert_eq!(history.len(), 2);
    assert!(
        history
            .iter()
            .any(|r| r.status == "late_root_observed" && r.observed["ts"] == "1700000000.000001")
    );
}

#[test]
fn history_is_bounded_pageable_and_rejects_status_stage_or_observation_corruption() {
    let (d, mut j, s, p) = setup();
    let scope = p.record().unwrap().scope().clone();
    for _ in 0..21 {
        let prepared = prepare(&s);
        let r = begin(&mut j, &prepared);
        let pending = j.annotation(r.record().scope()).unwrap().unwrap();
        assert!(j.annotation_not_sent(&pending).unwrap());
    }
    let first = j.annotation_history(&scope, None).unwrap();
    assert_eq!(first.len(), 20);
    let second = j.annotation_history(&scope, Some(&first[19].id)).unwrap();
    assert_eq!(second.len(), 1);
    assert!(
        first
            .iter()
            .all(|r| r.root().is_none() && r.id != second[0].id)
    );
    assert!(
        j.annotation_history(&scope, Some(&second[0].id))
            .unwrap()
            .is_empty()
    );
    let conn = rusqlite::Connection::open(d.path().join("annotation.sqlite")).unwrap();
    let id = &first[0].id;
    conn.execute(
        "UPDATE annotation_history SET status='root_accepted' WHERE id=?1",
        [id],
    )
    .unwrap();
    assert!(j.annotation_history(&scope, None).is_err());
    conn.execute(
        "UPDATE annotation_history SET status='not_sent',observed='{}' WHERE id=?1",
        [id],
    )
    .unwrap();
    assert!(j.annotation_history(&scope, None).is_err());
    conn.execute(
        "UPDATE annotation_history SET observed='null' WHERE id=?1",
        [id],
    )
    .unwrap();
    assert_eq!(j.annotation_history(&scope, None).unwrap().len(), 20);
}

#[test]
fn historical_root_requires_exact_control_and_channel_without_restoring_live_proof() {
    let (d, mut j, s, p) = setup();
    let r = begin(&mut j, &p);
    let (proof, _) = verified(p, &s);
    assert!(j.annotation_anchor_confirmed(&r, &proof).unwrap());
    let intent = BootstrapIntent::capture(proof, 1_700_000_000_001).unwrap();
    assert!(j.begin_annotation_bootstrap(&r, &intent).unwrap());
    let channel = crate::canvas::comments::channel(&r.record().scope().file).unwrap();
    let root = serde_json::json!({"type":"message","channel":channel,"ts":"1700000000.000001","subtype":"document_comment_root","document_comment":{"thread_id":r.record().control_id()},"text":""});
    let accepted = intent
        .accept(&serde_json::json!({"ok":true,"messages":[root],"has_more":false}))
        .unwrap();
    let pending = j.annotation(r.record().scope()).unwrap().unwrap();
    assert!(
        j.annotation_bootstrap_accepted(&pending, &accepted)
            .unwrap()
    );
    let saved = serde_json::to_value(accepted.root()).unwrap();
    let conn = rusqlite::Connection::open(d.path().join("annotation.sqlite")).unwrap();
    for (path, value) in [
        ("/channel", "CFOREIGN"),
        ("/document_comment/thread_id", "temp:foreign"),
        ("/thread_ts", "1700000000.000002"),
    ] {
        let mut corrupt = saved.clone();
        if path == "/thread_ts" {
            corrupt["thread_ts"] = serde_json::json!(value);
        } else {
            *corrupt.pointer_mut(path).unwrap() = serde_json::json!(value);
        }
        conn.execute(
            "UPDATE annotation_history SET observed=?1 WHERE id=?2",
            rusqlite::params![serde_json::to_string(&corrupt).unwrap(), r.id()],
        )
        .unwrap();
        assert!(
            j.annotation_history(r.record().scope(), None).is_err(),
            "{path}"
        );
    }
    conn.execute(
        "UPDATE annotation_history SET observed=?1 WHERE id=?2",
        rusqlite::params![serde_json::to_string(&saved).unwrap(), r.id()],
    )
    .unwrap();
    assert!(
        j.annotation_history(r.record().scope(), None).unwrap()[0]
            .root()
            .is_some()
    );
    assert!(j.annotation(r.record().scope()).unwrap().is_none());
}

#[test]
fn explicit_unconfirmed_ack_disposition_keeps_actual_ack_and_review_without_replay() {
    let (d, mut j, s, p) = setup();
    let r = begin(&mut j, &p);
    let pending = j.annotation(r.record().scope()).unwrap().unwrap();
    let control = receipt(&p);
    let accepted = p.validate_ack(&ack(&control)).unwrap();
    // Actual ACK is known, but this fresh read still lacks its new control.
    assert!(accepted.check_confirmation(&s).is_err());
    assert!(
        j.resolve_acknowledged_annotation_locally(
            &pending,
            r.record().review_current(&s).unwrap(),
            accepted.evidence().unwrap(),
            AnnotationDecision::RemoveLocalLockOutcomeUnknown
        )
        .unwrap()
    );
    drop(j);
    let mut j = Journal::open(d.path().join("annotation.sqlite"), OWNER).unwrap();
    assert!(
        j.resolve_acknowledged_annotation_locally(
            &pending,
            r.record().review_current(&s).unwrap(),
            accepted.evidence().unwrap(),
            AnnotationDecision::RemoveLocalLockOutcomeUnknown
        )
        .unwrap()
    );
    assert!(j.annotation(r.record().scope()).unwrap().is_none());
    let rows = j.annotation_history(r.record().scope(), None).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].status, "acknowledged_unconfirmed");
    assert_eq!(
        rows[0].observed["acknowledged_control"],
        serde_json::to_value(control.raw().raw()).unwrap()
    );
    assert!(rows[0].root().is_none());
    // A new explicit attempt remains untouched by a repeated old disposition.
    let next = begin(&mut j, &prepare(&s));
    assert!(
        !j.resolve_acknowledged_annotation_locally(
            &pending,
            r.record().review_current(&s).unwrap(),
            accepted.evidence().unwrap(),
            AnnotationDecision::RemoveLocalLockOutcomeUnknown
        )
        .unwrap()
    );
    assert_eq!(
        j.annotation(next.record().scope())
            .unwrap()
            .unwrap()
            .receipt()
            .id(),
        next.id()
    );
}

#[test]
fn original_ack_survives_other_disposition_and_new_attempt_with_stable_evidence_id() {
    let (d, mut j, s, p) = setup();
    let r = begin(&mut j, &p);
    let pending = j.annotation(r.record().scope()).unwrap().unwrap();
    let control = receipt(&p);
    let accepted = p.validate_ack(&ack(&control)).unwrap();
    let evidence = accepted.evidence().unwrap();
    assert!(
        j.resolve_annotation_locally(
            &pending,
            r.record().review_current(&s).unwrap(),
            AnnotationDecision::RemoveLocalLockOutcomeUnknown
        )
        .unwrap()
    );
    let next = begin(&mut j, &prepare(&s));
    let before = j.annotation(next.record().scope()).unwrap().unwrap();
    assert!(
        j.preserve_annotation_acknowledgement(&pending, &evidence)
            .unwrap()
    );
    assert!(
        j.preserve_annotation_acknowledgement(&pending, &evidence)
            .unwrap()
    );
    drop(j);
    let j = Journal::open(d.path().join("annotation.sqlite"), OWNER).unwrap();
    assert!(j.annotation(next.record().scope()).unwrap().unwrap() == before);
    let rows = j.annotation_history(r.record().scope(), None).unwrap();
    assert_eq!(rows.len(), 2);
    let known = rows
        .iter()
        .find(|r| r.status == "anchor_acknowledged")
        .unwrap();
    assert_eq!(
        known.observed["acknowledged_control"],
        serde_json::to_value(control.raw().raw()).unwrap()
    );
    assert!(known.root().is_none());
}
