//! Real isolated SQLite stores; no global opener, sessions, or remote writes.
use super::*;
use crate::canvas::comments::bootstrap::BootstrapIntent;
use crate::canvas::edit::structure::recovery::journal::{Journal, annotations::*};

const OWNER: &str = "147b5480-9f1a-4443-89da-836472cbb209";
struct Temp(std::path::PathBuf);
impl Temp {
    fn new() -> Self {
        let path =
            std::env::temp_dir().join(format!("canvas-multi-annotation-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn open(&self) -> Journal {
        Journal::open(self.0.join("annotation.sqlite"), OWNER).unwrap()
    }
}
impl Drop for Temp {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(self.0.join("annotation.sqlite"));
        let _ = std::fs::remove_dir(&self.0);
    }
}
fn begin(j: &mut Journal, p: &PreparedAnnotation) -> AnnotationReceipt {
    let AnnotationBegin::Started(receipt) = j.begin_annotation(p.record().unwrap()).unwrap() else {
        panic!("start")
    };
    receipt
}

#[test]
fn every_durable_stage_preserves_full_control_batch_and_not_sent_resume_requires_exact_canonical_read()
 {
    let dir = Temp::new();
    let s = fixture();
    let p = prepare(&s);
    let received = responses(&p, &s);
    let mut j = dir.open();
    let r = begin(&mut j, &p);
    let scope = r.record().scope().clone();
    drop(j);
    let mut j = dir.open();
    let pending = j.annotation(&scope).unwrap().unwrap();
    assert_eq!(pending.stage(), AnnotationStage::AnchorStarted);
    assert!(
        j.continue_annotation_bootstrap(&pending, &s, 1_700_000_000_001)
            .unwrap()
            .is_none()
    );
    let accepted = p.validate_ack(&ack(&received)).unwrap();
    let canonical = fresh(&s, &received);
    let proof = accepted.confirm(&canonical).unwrap();
    assert!(j.annotation_anchor_confirmed(&r, &proof).unwrap());
    let expected = serde_json::to_value(j.annotation(&scope).unwrap().unwrap()).unwrap();
    assert_eq!(
        expected["canonical_changed_controls"],
        serde_json::json!(
            received[..2]
                .iter()
                .map(|s| s.raw().raw())
                .collect::<Vec<_>>()
        )
    );
    drop(j);
    let mut j = dir.open();
    let confirmed = j.annotation(&scope).unwrap().unwrap();
    assert_eq!(confirmed.stage(), AnnotationStage::AnchorConfirmed);
    assert_eq!(serde_json::to_value(&confirmed).unwrap(), expected);
    let mut changed_read = canonical.clone();
    let index = changed_read
        .sections
        .iter()
        .position(|s| s.id() == "z-stub")
        .unwrap();
    changed_read.sections[index] = changed(&changed_read.sections[index], n(2, 10002));
    assert!(
        j.continue_annotation_bootstrap(&confirmed, &changed_read, 1_700_000_000_001)
            .is_err()
    );
    assert!(j.annotation(&scope).unwrap().unwrap() == confirmed);
    let intent = BootstrapIntent::capture(proof, 1_700_000_000_001).unwrap();
    assert!(j.begin_annotation_bootstrap(&r, &intent).unwrap());
    let pending = j.annotation(&scope).unwrap().unwrap();
    let first = serde_json::to_value(&pending).unwrap();
    drop(j);
    let mut j = dir.open();
    assert_eq!(
        serde_json::to_value(j.annotation(&scope).unwrap().unwrap()).unwrap(),
        first
    );
    assert!(j.annotation_not_sent(&pending).unwrap());
    let not_sent = j.annotation(&scope).unwrap().unwrap();
    assert_eq!(not_sent.stage(), AnnotationStage::BootstrapNotSent);
    drop(j);
    let mut j = dir.open();
    let not_sent = j.annotation(&scope).unwrap().unwrap();
    let retry = j
        .continue_annotation_bootstrap(&not_sent, &canonical, 1_700_000_000_002)
        .unwrap()
        .unwrap();
    let encoded = serde_json::to_value(retry.record()).unwrap();
    assert_eq!(
        encoded["canonical_changed_controls"],
        expected["canonical_changed_controls"]
    );
    assert!(!j.annotation_not_sent(&pending).unwrap());
    assert_eq!(j.annotation_history(&scope, None).unwrap().len(), 1);
}

#[test]
fn unconfirmed_disposition_and_late_ack_keep_every_actual_old_result_without_touching_new_attempt()
{
    let dir = Temp::new();
    let mut j = dir.open();
    let s = fixture();
    let p = prepare(&s);
    let r = begin(&mut j, &p);
    let pending = j.annotation(r.record().scope()).unwrap().unwrap();
    let received = responses(&p, &s);
    let accepted = p.validate_ack(&ack(&received)).unwrap();
    let evidence = accepted.evidence().unwrap();
    assert!(accepted.check_confirmation(&s).is_err());
    assert!(
        j.resolve_acknowledged_annotation_locally(
            &pending,
            r.record().review_current(&s).unwrap(),
            evidence.clone(),
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
    drop(j);
    let mut j = dir.open();
    assert!(
        j.preserve_annotation_acknowledgement(&pending, &evidence)
            .unwrap()
    );
    assert!(j.annotation(next.record().scope()).unwrap().unwrap() == before);
    let history = j.annotation_history(r.record().scope(), None).unwrap();
    assert_eq!(history.len(), 2);
    for row in &history {
        assert_eq!(
            row.observed["acknowledged_changed_controls"],
            serde_json::json!(evidence.changed_controls)
        );
        assert!(row.root().is_none());
    }
    let reviewed = history
        .iter()
        .find(|r| r.status == "acknowledged_unconfirmed")
        .unwrap();
    assert_eq!(
        reviewed.observed["prior"]["controls"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        reviewed.observed["prior"]["metadata"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn stored_missing_or_foreign_batch_evidence_refuses_without_recreating_bootstrap_authority() {
    let dir = Temp::new();
    let mut j = dir.open();
    let s = fixture();
    let p = prepare(&s);
    let r = begin(&mut j, &p);
    let received = responses(&p, &s);
    let proof = p
        .validate_ack(&ack(&received))
        .unwrap()
        .confirm(&fresh(&s, &received))
        .unwrap();
    assert!(j.annotation_anchor_confirmed(&r, &proof).unwrap());
    drop(j);
    let conn = rusqlite::Connection::open(dir.0.join("annotation.sqlite")).unwrap();
    let original: String = conn
        .query_row("SELECT body FROM annotation_attempts", [], |r| r.get(0))
        .unwrap();
    let saved: serde_json::Value = serde_json::from_str(&original).unwrap();
    assert_eq!(
        saved["canonical_changed_controls"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    for vector in [
        serde_json::json!([]),
        serde_json::json!([received[0].raw().raw()]),
        serde_json::json!([
            received[0].raw().raw(),
            changed(&received[1], t(7, "foreign")).raw().raw()
        ]),
    ] {
        let mut corrupt = saved.clone();
        corrupt["canonical_changed_controls"] = vector;
        conn.execute(
            "UPDATE annotation_attempts SET body=?1",
            [serde_json::to_string(&corrupt).unwrap()],
        )
        .unwrap();
        assert!(dir.open().annotation(r.record().scope()).is_err());
    }
    conn.execute("UPDATE annotation_attempts SET body=?1", [original])
        .unwrap();
    assert!(dir.open().annotation(r.record().scope()).unwrap().is_some());
}
