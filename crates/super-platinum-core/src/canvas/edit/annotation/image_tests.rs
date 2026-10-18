//! Real prepare/ACK/canonical/journal paths; native export is explicit input.
use super::*;
fn image_fixture(fields: &[F]) -> CanvasCollabSnapshot {
    let mut s = fixture();
    let i = s.sections.iter().position(|s| s.id() == "item").unwrap();
    let mut anchor = s.sections[i].raw().remove(38).unwrap();
    for field in [n(9, 2), n(10, 11), m(12, &raw(&[m(2, &raw(fields))]))] {
        anchor = anchor
            .merge_partial(&RawMessage::decode(field.raw()).unwrap())
            .unwrap();
    }
    s.sections[i] = Section::from_raw(anchor).unwrap();
    s
}
fn image_prepare(s: &CanvasCollabSnapshot, export: ImageExport) -> PreparedAnnotation {
    let SectionAnnotationChoice::New(intent) = AnnotationIntent::for_image(
        s,
        "item",
        &AnnotationPresentation::ordinary(None, None).unwrap(),
        export,
    )
    .unwrap() else {
        panic!("new image annotation")
    };
    intent.prepare(s).unwrap()
}
#[test]
fn canonical_image_export_outcomes_preserve_alt_precedence_and_numeric_file_stub() {
    for (fields, expected) in [
        (vec![t(7, "plan.png")], "plan.png"),
        (
            vec![t(7, "ignored"), t(15, "A & B <C> \"quote\"")],
            "A & B <C> \"quote\"",
        ),
        (vec![t(7, "ignored"), t(15, "")], "Image"),
        (vec![t(7, "ignored"), t(15, "ignored"), n(16, 1)], "Image"),
        (vec![t(15, "A\r\nB\rC\0D")], "A\nB\nC\u{fffd}D"),
        (vec![t(15, "Owner's drawing")], "Owner"),
        (vec![t(15, "'empty parsed alt")], "Image"),
    ] {
        for outcome in [
            ImageExport::Exported,
            ImageExport::NoImage,
            ImageExport::EmptyData,
            ImageExport::ExportError,
        ] {
            let mut fields = fields.clone();
            fields.push(m(
                3,
                &raw(&[t(5, "FIMAGE"), t(99, "unknown original image field")]),
            ));
            let s = image_fixture(&fields);
            let p = image_prepare(&s, outcome);
            let record = p.record().unwrap();
            assert_eq!(serde_json::to_value(&record).unwrap()["version"], 9);
            assert_eq!(record.snippet()["fileIds"], serde_json::json!(["FIMAGE"]));
            assert_eq!(
                record.snippet()["objectStubs"],
                serde_json::json!([{"type":4,"id":"FIMAGE"}])
            );
            assert_eq!(record.snippet()["mrkdwn"], "");
            let rich: serde_json::Value =
                serde_json::from_str(record.snippet()["richText"].as_str().unwrap()).unwrap();
            if outcome == ImageExport::Exported {
                assert_eq!(
                    rich["blocks"][0]["elements"][0]["elements"][0]["text"],
                    expected
                );
            } else {
                assert_eq!(rich["blocks"][0]["elements"], serde_json::json!([]));
            }
            assert_eq!(record.anchor().unwrap(), *s.section("item").unwrap());
            let round: AnnotationRecord =
                serde_json::from_value(serde_json::to_value(record).unwrap()).unwrap();
            round.validate().unwrap();
            // Ordinary text/section ingress cannot fabricate native export.
            assert!(
                AnnotationIntent::for_section(
                    &s,
                    "item",
                    &AnnotationPresentation::ordinary(None, None).unwrap()
                )
                .is_err()
            );
        }
    }
}

#[test]
fn image_ack_and_bootstrap_require_unchanged_canonical_image_not_only_its_label() {
    let s = image_fixture(&[t(7, "plan.png"), m(3, &raw(&[t(5, "FIMAGE")]))]);
    let p = image_prepare(&s, ImageExport::Exported);
    let rows = responses(&p, &s);
    let record = p.record().unwrap();
    assert_eq!(p.document_data().messages(1).unwrap().len(), 3);
    assert!(!record.has_parent_update());
    let bad = image_prepare(&s, ImageExport::Exported);
    let partial = responses(&bad, &s);
    assert!(bad.validate_ack(&ack(&partial[1..])).is_err());
    let proof = p.validate_ack(&ack(&rows)).unwrap();
    let current = fresh(&s, &rows);
    for field in [
        n(2, 1234),
        m(
            12,
            &raw(&[m(
                2,
                &raw(&[t(7, "plan.png"), m(3, &raw(&[t(5, "FOTHER")]))]),
            )]),
        ),
        m(13, &raw(&[t(1, "other")])),
    ] {
        let mut changed_current = current.clone();
        let i = changed_current
            .sections
            .iter()
            .position(|s| s.id() == "item")
            .unwrap();
        changed_current.sections[i] = changed(&changed_current.sections[i], field);
        assert!(proof.check_confirmation(&changed_current).is_err());
        assert!(
            record
                .bootstrap_current_with_controls(
                    &changed_current,
                    rows.last().unwrap().raw().raw(),
                    None,
                    &rows[..rows.len() - 1]
                        .iter()
                        .map(|s| s.raw().raw().to_vec())
                        .collect::<Vec<_>>()
                )
                .is_err()
        );
    }
    let confirmed = proof.confirm(&current).unwrap();
    assert!(
        crate::canvas::comments::bootstrap::BootstrapIntent::capture(confirmed, 1_700_000_000_001)
            .is_ok()
    );
}

#[test]
fn image_layout_counts_deleted_rules_but_never_offers_them_as_comment_targets() {
    let mut s = image_fixture(&[]);
    let template = s.section("item").unwrap().clone();
    let mut rule = template.raw().remove(38).unwrap();
    for field in [
        t(1, "rule"),
        n(3, 1),
        t(8, "aabb"),
        t(21, "aabb"),
        n(9, 16),
        n(10, 18),
        m(12, &raw(&[m(43, &raw(&[n(1, 1)]))])),
    ] {
        rule = rule
            .merge_partial(&RawMessage::decode(field.raw()).unwrap())
            .unwrap();
    }
    s.sections.push(Section::from_raw(rule).unwrap());
    let p = image_prepare(&s, ImageExport::NoImage);
    let request = p.document_data().messages(1).unwrap().pop().unwrap();
    assert_eq!(
        request.string(21).unwrap(),
        Some(
            position::between(Some("aabb"), Some("aac"))
                .unwrap()
                .as_str()
        )
    );
    assert!(
        AnnotationIntent::for_image(
            &s,
            "rule",
            &AnnotationPresentation::ordinary(None, None).unwrap(),
            ImageExport::NoImage
        )
        .is_err()
    );
    assert!(
        AnnotationIntent::for_section(
            &s,
            "rule",
            &AnnotationPresentation::ordinary(None, None).unwrap()
        )
        .is_err()
    );
    assert_eq!(
        p.record().unwrap().snippet()["fileIds"],
        serde_json::json!([])
    );
}

#[test]
fn image_export_record_cannot_change_outcome_or_demote_media_version() {
    let s = image_fixture(&[t(7, "diagram.png")]);
    let record = image_prepare(&s, ImageExport::Exported).record().unwrap();
    let value = serde_json::to_value(&record).unwrap();
    for version in 1..=8 {
        let mut wrong = value.clone();
        wrong["version"] = version.into();
        assert!(
            serde_json::from_value::<AnnotationRecord>(wrong)
                .unwrap()
                .validate()
                .is_err()
        );
    }
    let mut wrong = value.clone();
    wrong["section_plan"]["image_export"] = "no_image".into();
    assert!(
        serde_json::from_value::<AnnotationRecord>(wrong)
            .unwrap()
            .validate()
            .is_err()
    );
    let mut wrong = value;
    wrong["section_plan"]["media_layout"] = false.into();
    assert!(
        serde_json::from_value::<AnnotationRecord>(wrong)
            .unwrap()
            .validate()
            .is_err()
    );
    record.validate().unwrap();
}

#[path = "image_journal_tests.rs"]
mod journal;

#[test]
fn native_image_mapping_requires_exact_resource_prefix_thread_and_original_identity() {
    let mut s = image_fixture(&[
        t(7, "diagram.png"),
        m(3, &raw(&[t(5, "FIMAGE")])),
        m(5, &raw(&[t(1, "first-preview")])),
        m(5, &raw(&[t(1, "second-preview"), t(5, "FPREVIEW")])),
    ]);
    let options = s
        .init
        .options()
        .replace_fields(8, &[m(8, &raw(&[t(2, "https://fixture.invalid")]))])
        .unwrap();
    let workspace = crate::config::WorkspaceSession {
        team_id: s.init.workspace_id().into(),
        enterprise_id: None,
        user_id: s.init.options().string(33).unwrap().unwrap().into(),
        name: "Fixture".into(),
        url: "https://fixture.invalid".into(),
        token: "fixture-only".into(),
    };
    s.init = crate::slack::api::CanvasControllerInit::parse(
        serde_json::json!({
            "ok":true,"quip_user_id":s.init.quip_user_id(),
            "init_options":crate::slack::api::encode_canvas_base64(options.raw())
        }),
        &workspace,
    )
    .unwrap();
    let seed = image_seed(&s, "item").unwrap();
    assert!(seed.has_image_source());
    let exact = format!(
        "https://fixture.invalid/-/collab-slack-blob/{}/FIMAGE",
        s.thread_id()
    );
    assert!(seed.matches_rendered_url(&exact));
    assert!(seed.matches_rendered_url(&exact.replace("collab-slack-blob", "blob")));
    for path in ["blob", "collab-slack-blob"] {
        for id in ["first-preview", "second-preview", "FPREVIEW"] {
            assert!(seed.matches_rendered_url(&format!(
                "https://fixture.invalid/-/{path}/{}/{id}",
                s.thread_id()
            )));
        }
    }
    assert!(seed.matches_rendered_url(&format!("{exact}?s=signature&name=diagram.png")));
    for wrong in [
        exact.replace("fixture.invalid", "other.invalid"),
        exact.replace(s.thread_id(), "OtherThread"),
        exact.replace("FIMAGE", "FOTHER"),
        format!("{exact}?redirect=other"),
        format!("{exact}?s=one&s=two"),
        format!("{exact}#other"),
        exact.replace("https:", "http:"),
    ] {
        assert!(!seed.matches_rendered_url(&wrong));
    }
}

#[test]
fn image_original_fallback_uses_first_widest_hashed_size_and_respects_present_empty_original() {
    let sizes = vec![
        m(4, &raw(&[t(1, "small"), n(2, 10), t(5, "FSMALL")])),
        m(4, &raw(&[t(1, "wide"), n(2, 20), t(5, "FFIRST")])),
        m(4, &raw(&[t(1, "same-width"), n(2, 20), t(5, "FSECOND")])),
        m(4, &raw(&[n(2, 30), t(5, "FUNHASHED")])),
    ];
    let s = image_fixture(&sizes);
    let original = s.sections.clone();
    assert_eq!(
        image_prepare(&s, ImageExport::ExportError)
            .record()
            .unwrap()
            .snippet()["fileIds"],
        serde_json::json!(["FFIRST"])
    );
    assert_eq!(s.sections, original);
    let mut explicit = sizes;
    explicit.push(m(3, &raw(&[t(5, "")])));
    let s = image_fixture(&explicit);
    assert_eq!(
        image_prepare(&s, ImageExport::NoImage)
            .record()
            .unwrap()
            .snippet()["fileIds"],
        serde_json::json!([])
    );
    for (sizes, expected) in [
        (
            vec![m(4, &raw(&[t(1, "hash"), t(5, "FABSENT")]))],
            serde_json::json!([]),
        ),
        (
            vec![m(4, &raw(&[t(1, "hash"), n(2, 0), t(5, "FZERO")]))],
            serde_json::json!(["FZERO"]),
        ),
        (
            vec![
                m(4, &raw(&[t(1, "hash"), t(5, "FABSENT")])),
                m(4, &raw(&[t(1, "zero"), n(2, 0), t(5, "FZERO")])),
            ],
            serde_json::json!(["FZERO"]),
        ),
    ] {
        let snapshot = image_fixture(&sizes);
        assert_eq!(
            image_prepare(&snapshot, ImageExport::ExportError)
                .record()
                .unwrap()
                .snippet()["fileIds"],
            expected
        );
    }
}
