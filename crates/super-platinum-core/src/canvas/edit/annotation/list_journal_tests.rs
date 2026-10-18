//! Actual isolated SQLite persistence, preserving received results and children.
use super::*;
use crate::canvas::comments::bootstrap::BootstrapIntent;
use crate::canvas::edit::structure::recovery::journal::{Journal, annotations::*};
struct Temp(std::path::PathBuf);
impl Temp {
    fn new() -> Self {
        let path =
            std::env::temp_dir().join(format!("canvas-list-annotation-{}", uuid::Uuid::new_v4()));
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
        panic!()
    };
    receipt
}

#[test]
fn list_version_eight_restart_and_not_sent_bootstrap_keep_exact_ordered_children() {
    let dir = Temp::new();
    let mut s = list_fixture(7, true);
    let deleted = s.sections.iter().position(|s| s.id() == "child-b").unwrap();
    s.sections[deleted] = changed(&s.sections[deleted], n(3, 1));
    let p = prepare(&s);
    let actual = responses(&p, &s);
    let mut j = dir.open();
    let receipt = begin(&mut j, &p);
    let scope = receipt.record().scope().clone();
    drop(j);
    let mut j = dir.open();
    let started = j.annotation(&scope).unwrap().unwrap();
    assert_eq!(started.stage(), AnnotationStage::AnchorStarted);
    assert!(
        j.continue_annotation_bootstrap(&started, &s, 1_700_000_000_001)
            .unwrap()
            .is_none()
    );
    let canonical = fresh(&s, &actual);
    let proof = p
        .validate_ack(&ack(&actual))
        .unwrap()
        .confirm(&canonical)
        .unwrap();
    assert!(j.annotation_anchor_confirmed(&receipt, &proof).unwrap());
    let before = serde_json::to_value(j.annotation(&scope).unwrap().unwrap()).unwrap();
    drop(j);
    let mut j = dir.open();
    let confirmed = j.annotation(&scope).unwrap().unwrap();
    assert_eq!(serde_json::to_value(&confirmed).unwrap(), before);
    let mut changed_children = canonical.clone();
    let i = changed_children
        .sections
        .iter()
        .position(|s| s.id() == "child-a")
        .unwrap();
    changed_children.sections[i] = changed(&changed_children.sections[i], n(2, 100));
    assert!(
        j.continue_annotation_bootstrap(&confirmed, &changed_children, 1_700_000_000_001)
            .is_err()
    );
    assert_eq!(
        serde_json::to_value(j.annotation(&scope).unwrap().unwrap()).unwrap(),
        before
    );
    let deleted_changes: Vec<_> = (0..2)
        .map(|mode| {
            let mut value = canonical.clone();
            let i = value
                .sections
                .iter()
                .position(|s| s.id() == "child-b")
                .unwrap();
            if mode == 0 {
                value.sections.remove(i);
            } else {
                value.sections[i] = changed(&value.sections[i], n(2, 100));
            }
            value
        })
        .collect();
    for value in &deleted_changes {
        assert!(
            j.continue_annotation_bootstrap(&confirmed, value, 1_700_000_000_001)
                .is_err()
        );
        assert_eq!(
            serde_json::to_value(j.annotation(&scope).unwrap().unwrap()).unwrap(),
            before
        );
    }
    let bootstrap = BootstrapIntent::capture(proof, 1_700_000_000_001).unwrap();
    assert!(j.begin_annotation_bootstrap(&receipt, &bootstrap).unwrap());
    let pending = j.annotation(&scope).unwrap().unwrap();
    assert!(j.annotation_not_sent(&pending).unwrap());
    drop(j);
    let mut j = dir.open();
    let not_sent = j.annotation(&scope).unwrap().unwrap();
    assert_eq!(not_sent.stage(), AnnotationStage::BootstrapNotSent);
    for value in &deleted_changes {
        assert!(
            j.continue_annotation_bootstrap(&not_sent, value, 1_700_000_000_002)
                .is_err()
        );
        assert!(j.annotation(&scope).unwrap().unwrap() == not_sent);
    }
    let retry = j
        .continue_annotation_bootstrap(&not_sent, &canonical, 1_700_000_000_002)
        .unwrap()
        .unwrap();
    let plan = serde_json::to_value(retry.record()).unwrap()["annotation"]["section_plan"].clone();
    assert!(plan.is_object());
    assert_eq!(plan, before["receipt"]["record"]["section_plan"]);
    assert!(!j.annotation_not_sent(&pending).unwrap());
}

#[test]
fn changed_list_children_disposition_and_late_ack_remain_inspectable_after_restart() {
    let dir = Temp::new();
    let s = list_fixture(5, true);
    let p = prepare(&s);
    let mut j = dir.open();
    let receipt = begin(&mut j, &p);
    let pending = j.annotation(receipt.record().scope()).unwrap().unwrap();
    let actual = responses(&p, &s);
    let accepted = p.validate_ack(&ack(&actual)).unwrap();
    let evidence = accepted.evidence().unwrap();
    let mut changed_read = fresh(&s, &actual);
    let i = changed_read
        .sections
        .iter()
        .position(|s| s.id() == "child-b")
        .unwrap();
    changed_read.sections.remove(i);
    assert!(accepted.check_confirmation(&changed_read).is_err());
    let observed = receipt.record().review_current(&changed_read).unwrap();
    assert_eq!(
        observed.prior_evidence().unwrap()["children"]["children"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(
        j.resolve_acknowledged_annotation_locally(
            &pending,
            observed,
            evidence.clone(),
            AnnotationDecision::RemoveLocalLockOutcomeUnknown
        )
        .unwrap()
    );
    let next = begin(&mut j, &prepare(&s));
    let next_before = j.annotation(next.record().scope()).unwrap().unwrap();
    assert!(
        j.preserve_annotation_acknowledgement(&pending, &evidence)
            .unwrap()
    );
    drop(j);
    let mut j = dir.open();
    assert!(j.annotation(next.record().scope()).unwrap().unwrap() == next_before);
    let history = j
        .annotation_history(receipt.record().scope(), None)
        .unwrap();
    assert_eq!(history.len(), 2);
    let reviewed = history
        .iter()
        .find(|r| r.status == "acknowledged_unconfirmed")
        .unwrap();
    assert_eq!(
        reviewed.observed["prior"]["children"]["children"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        reviewed.observed["acknowledged_changed_controls"],
        serde_json::json!(evidence.changed_controls)
    );
    assert!(history.iter().all(|r| r.root().is_none()));
}
