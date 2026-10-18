use super::*;
use crate::canvas::edit::structure::recovery::journal::{Journal, annotations::*};
struct Temp(std::path::PathBuf);
impl Temp {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!("column-layout-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&p).unwrap();
        Self(p)
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
    let AnnotationBegin::Started(r) = j.begin_annotation(p.record().unwrap()).unwrap() else {
        panic!("started")
    };
    r
}

#[test]
fn v13_actual_restart_both_bootstrap_states_recheck_column_and_deleted_list_custody() {
    let dir = Temp::new();
    let mut s = columned(list(7), true, true);
    let i = s.sections.iter().position(|r| r.id() == "child-b").unwrap();
    s.sections[i] = changed(&s.sections[i], n(3, 1));
    let p = table_prepare(&s);
    let rows = responses(&p, &s);
    let canonical = fresh(&s, &rows);
    let mut j = dir.open();
    let receipt = begin(&mut j, &p);
    let scope = receipt.record().scope().clone();
    drop(j);
    let mut j = dir.open();
    let started = j.annotation(&scope).unwrap().unwrap();
    assert_eq!(started.stage(), AnnotationStage::AnchorStarted);
    assert_eq!(
        serde_json::to_value(started.receipt().record()).unwrap()["version"],
        13
    );
    assert!(
        j.continue_annotation_bootstrap(&started, &s, 1_720_000_000_001)
            .unwrap()
            .is_none()
    );
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
    for mode in [0, 1, 3] {
        assert!(
            j.continue_annotation_bootstrap(
                &confirmed,
                &mutated_child(&canonical, mode),
                1_720_000_000_002
            )
            .is_err()
        );
        assert!(j.annotation(&scope).unwrap().unwrap() == confirmed);
    }
    for mode in 0..4 {
        assert!(
            j.continue_annotation_bootstrap(
                &confirmed,
                &changed_columns(&canonical, mode),
                1_720_000_000_002
            )
            .is_err()
        );
        assert!(j.annotation(&scope).unwrap().unwrap() == confirmed);
    }
    let first = j
        .continue_annotation_bootstrap(&confirmed, &canonical, 1_720_000_000_002)
        .unwrap()
        .unwrap();
    first.record().validate().unwrap();
    let pending = j.annotation(&scope).unwrap().unwrap();
    assert!(j.annotation_not_sent(&pending).unwrap());
    drop(j);
    let mut j = dir.open();
    let not_sent = j.annotation(&scope).unwrap().unwrap();
    assert_eq!(not_sent.stage(), AnnotationStage::BootstrapNotSent);
    for mode in [0, 1, 3] {
        assert!(
            j.continue_annotation_bootstrap(
                &not_sent,
                &mutated_child(&canonical, mode),
                1_720_000_000_003
            )
            .is_err()
        );
        assert!(j.annotation(&scope).unwrap().unwrap() == not_sent);
    }
    for mode in 0..4 {
        assert!(
            j.continue_annotation_bootstrap(
                &not_sent,
                &changed_columns(&canonical, mode),
                1_720_000_000_003
            )
            .is_err()
        );
        assert!(j.annotation(&scope).unwrap().unwrap() == not_sent);
    }
    let retry = j
        .continue_annotation_bootstrap(&not_sent, &canonical, 1_720_000_000_003)
        .unwrap()
        .unwrap();
    assert!(retry.record().annotation() == receipt.record());
    retry.record().validate().unwrap();
}

#[test]
fn column_late_actual_ack_is_insert_only_after_changed_layout_local_disposition() {
    let dir = Temp::new();
    let s = columned(list(5), false, false);
    let p = table_prepare(&s);
    let mut j = dir.open();
    let receipt = begin(&mut j, &p);
    let scope = receipt.record().scope().clone();
    let pending = j.annotation(&scope).unwrap().unwrap();
    let rows = responses(&p, &s);
    let accepted = p.validate_ack(&ack(&rows)).unwrap();
    let evidence = accepted.evidence().unwrap();
    let changed = changed_columns(&fresh(&s, &rows), 0);
    assert!(accepted.check_confirmation(&changed).is_err());
    let observed = receipt.record().review_current(&changed).unwrap();
    let prior = observed.prior_evidence().unwrap();
    assert!(observed.preview_blocks().is_ok());
    receipt
        .record()
        .validate_prior_evidence(Some(&prior))
        .unwrap();
    assert!(
        j.resolve_acknowledged_annotation_locally(
            &pending,
            observed,
            evidence.clone(),
            AnnotationDecision::RemoveLocalLockOutcomeUnknown
        )
        .unwrap()
    );
    let next = begin(&mut j, &table_prepare(&s));
    let exact = serde_json::to_value(j.annotation(&scope).unwrap().unwrap()).unwrap();
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
        serde_json::to_value(j.annotation(&scope).unwrap().unwrap()).unwrap(),
        exact
    );
    assert_ne!(next.record().control_id(), receipt.record().control_id());
    let rows = j.annotation_history(&scope, None).unwrap();
    assert_eq!(rows.len(), 2);
    let reviewed = rows
        .iter()
        .find(|r| r.status == "acknowledged_unconfirmed")
        .unwrap();
    assert_eq!(reviewed.observed["prior"]["table"], prior["table"]);
    assert_eq!(
        reviewed.observed["acknowledged_changed_controls"],
        serde_json::json!(evidence.changed_controls)
    );
}
