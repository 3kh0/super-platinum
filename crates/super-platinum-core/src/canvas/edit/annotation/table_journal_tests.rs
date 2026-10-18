//! Real private SQLite stores and actual version-10 annotation transitions.
use super::*;
use crate::canvas::edit::structure::recovery::journal::{Journal, annotations::*};

struct Temp(std::path::PathBuf);
impl Temp {
    fn new() -> Self {
        let path =
            std::env::temp_dir().join(format!("canvas-table-annotation-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn open(&self) -> Journal {
        Journal::open(
            self.0.join("annotation.sqlite"),
            "147b5480-9f1a-4443-89da-836472cbb209",
        )
        .unwrap()
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

fn changed_table(s: &CanvasCollabSnapshot) -> CanvasCollabSnapshot {
    let mut s = s.clone();
    let at = s.sections.iter().position(|r| r.id() == "table").unwrap();
    s.sections[at] = changed(&s.sections[at], n(2, 72));
    s
}

#[test]
fn table_actual_restart_both_bootstrap_states_recheck_exact_parent_and_keep_body_result() {
    let dir = Temp::new();
    let (s, _) = table_body_fixture();
    let p = table_prepare(&s);
    let rows = canonical_responses(&p, &s);
    let mut j = dir.open();
    let receipt = begin(&mut j, &p);
    let scope = receipt.record().scope().clone();
    drop(j);
    let mut j = dir.open();
    let started = j.annotation(&scope).unwrap().unwrap();
    assert_eq!(started.stage(), AnnotationStage::AnchorStarted);
    let record = serde_json::to_value(started.receipt().record()).unwrap();
    assert_eq!(record["version"], 10);
    assert_eq!(record["section_plan"]["table_layout"], true);
    assert!(
        j.continue_annotation_bootstrap(&started, &s, 1_720_000_000_001)
            .unwrap()
            .is_none()
    );
    let canonical = fresh(&s, &rows);
    let proof = p
        .validate_ack(&ack(&rows))
        .unwrap()
        .confirm(&canonical)
        .unwrap();
    assert!(j.annotation_anchor_confirmed(&receipt, &proof).unwrap());
    drop(j);
    let mut j = dir.open();
    let confirmed = j.annotation(&scope).unwrap().unwrap();
    assert_eq!(confirmed.stage(), AnnotationStage::AnchorConfirmed);
    let exact = serde_json::to_value(&confirmed).unwrap();
    assert!(exact["canonical_anchor"].is_array());
    assert_eq!(
        exact["canonical_changed_controls"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert!(
        j.continue_annotation_bootstrap(&confirmed, &changed_table(&canonical), 1_720_000_000_002)
            .is_err()
    );
    assert_eq!(
        serde_json::to_value(j.annotation(&scope).unwrap().unwrap()).unwrap(),
        exact
    );
    let retry = j
        .continue_annotation_bootstrap(&confirmed, &canonical, 1_720_000_000_002)
        .unwrap()
        .unwrap();
    retry.record().validate().unwrap();
    // The continuation transaction spends the exact confirmed admission and
    // retains BootstrapStarted. A lost return never recreates a send permit.
    let pending = j.annotation(&scope).unwrap().unwrap();
    assert_eq!(pending.stage(), AnnotationStage::BootstrapStarted);
    assert!(
        j.continue_annotation_bootstrap(&confirmed, &canonical, 1_720_000_000_002)
            .unwrap()
            .is_none()
    );
    assert!(j.annotation_not_sent(&pending).unwrap());
    drop(j);
    let mut j = dir.open();
    let not_sent = j.annotation(&scope).unwrap().unwrap();
    assert_eq!(not_sent.stage(), AnnotationStage::BootstrapNotSent);
    let exact = serde_json::to_value(&not_sent).unwrap();
    assert!(
        j.continue_annotation_bootstrap(&not_sent, &changed_table(&canonical), 1_720_000_000_003)
            .is_err()
    );
    assert_eq!(
        serde_json::to_value(j.annotation(&scope).unwrap().unwrap()).unwrap(),
        exact
    );
    let retry = j
        .continue_annotation_bootstrap(&not_sent, &canonical, 1_720_000_000_003)
        .unwrap()
        .unwrap();
    retry.record().validate().unwrap();
    assert!(retry.record().annotation() == receipt.record());
}

#[test]
fn table_local_disposition_and_late_ack_preserve_exact_layout_without_replacing_new_attempt() {
    let dir = Temp::new();
    let mut j = dir.open();
    let (s, _) = table_body_fixture();
    let p = table_prepare(&s);
    let receipt = begin(&mut j, &p);
    let pending = j.annotation(receipt.record().scope()).unwrap().unwrap();
    let rows = canonical_responses(&p, &s);
    let accepted = p.validate_ack(&ack(&rows)).unwrap();
    let evidence = accepted.evidence().unwrap();
    assert!(accepted.check_confirmation(&s).is_err());
    let review = receipt.record().review_current(&s).unwrap();
    assert!(review.matches_current(&s).unwrap());
    assert!(!review.matches_current(&changed_table(&s)).unwrap());
    let mut missing = s.clone();
    missing.sections.retain(|s| s.id() != "table");
    assert!(!review.matches_current(&missing).unwrap());
    let observed = review.prior_evidence().unwrap();
    receipt
        .record()
        .validate_prior_evidence(Some(&observed))
        .unwrap();
    assert!(
        receipt
            .record()
            .validate_prior_evidence(observed.get("annotation"))
            .is_err()
    );
    assert!(
        j.resolve_acknowledged_annotation_locally(
            &pending,
            receipt.record().review_current(&s).unwrap(),
            evidence.clone(),
            AnnotationDecision::RemoveLocalLockOutcomeUnknown
        )
        .unwrap()
    );
    let next = begin(&mut j, &table_prepare(&s));
    let before =
        serde_json::to_value(j.annotation(next.record().scope()).unwrap().unwrap()).unwrap();
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
    assert_eq!(
        serde_json::to_value(j.annotation(next.record().scope()).unwrap().unwrap()).unwrap(),
        before
    );
    let history = j
        .annotation_history(receipt.record().scope(), None)
        .unwrap();
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
    assert_eq!(reviewed.observed["prior"]["table"], observed["table"]);
    // The original full section plan is insert-only receipt custody and still
    // validates version 10 after the active row belongs to another UUID.
    let record: AnnotationRecord =
        serde_json::from_value(serde_json::to_value(receipt.record()).unwrap()).unwrap();
    record.validate().unwrap();
    assert_eq!(
        serde_json::to_value(&record).unwrap()["section_plan"]["sections"],
        serde_json::json!(s.sections.iter().map(|s| s.raw().raw()).collect::<Vec<_>>())
    );
}
