//! Real isolated SQLite state; observed native outcome is explicit test input.
use super::*;
use crate::canvas::comments::bootstrap::BootstrapIntent;
use crate::canvas::edit::structure::recovery::journal::{Journal, annotations::*};
struct Temp(std::path::PathBuf);
impl Temp {
    fn new() -> Self {
        let path =
            std::env::temp_dir().join(format!("canvas-image-annotation-{}", uuid::Uuid::new_v4()));
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
#[test]
fn actual_image_restart_retains_export_and_refuses_changed_image_before_not_sent_continuation() {
    let dir = Temp::new();
    let s = image_fixture(&[t(7, "diagram.png"), m(3, &raw(&[t(5, "FIMAGE")]))]);
    let p = image_prepare(&s, ImageExport::ExportError);
    let rows = responses(&p, &s);
    let mut j = dir.open();
    let AnnotationBegin::Started(receipt) = j.begin_annotation(p.record().unwrap()).unwrap() else {
        panic!("start")
    };
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
    assert_eq!(
        serde_json::to_value(started.receipt().record()).unwrap()["section_plan"]["image_export"],
        "export_error"
    );
    let canonical = fresh(&s, &rows);
    let proof = p
        .validate_ack(&ack(&rows))
        .unwrap()
        .confirm(&canonical)
        .unwrap();
    assert!(j.annotation_anchor_confirmed(&receipt, &proof).unwrap());
    let bootstrap = BootstrapIntent::capture(proof, 1_700_000_000_001).unwrap();
    assert!(j.begin_annotation_bootstrap(&receipt, &bootstrap).unwrap());
    let pending = j.annotation(&scope).unwrap().unwrap();
    assert!(j.annotation_not_sent(&pending).unwrap());
    drop(j);
    let mut j = dir.open();
    let pending = j.annotation(&scope).unwrap().unwrap();
    assert_eq!(pending.stage(), AnnotationStage::BootstrapNotSent);
    let exact = serde_json::to_value(&pending).unwrap();
    let mut other = canonical.clone();
    let i = other
        .sections
        .iter()
        .position(|s| s.id() == "item")
        .unwrap();
    other.sections[i] = changed(
        &other.sections[i],
        m(
            12,
            &raw(&[m(
                2,
                &raw(&[t(7, "same label"), m(3, &raw(&[t(5, "FOTHER")]))]),
            )]),
        ),
    );
    assert!(
        j.continue_annotation_bootstrap(&pending, &other, 1_700_000_000_002)
            .is_err()
    );
    assert_eq!(
        serde_json::to_value(j.annotation(&scope).unwrap().unwrap()).unwrap(),
        exact
    );
    let retry = j
        .continue_annotation_bootstrap(&pending, &canonical, 1_700_000_000_002)
        .unwrap()
        .unwrap();
    retry.record().validate().unwrap();
    assert_eq!(
        retry.record().annotation().snippet(),
        receipt.record().snippet()
    );
}
