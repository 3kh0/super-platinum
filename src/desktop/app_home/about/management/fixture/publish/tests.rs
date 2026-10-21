use super::*;
#[test]
fn publish_cases_are_closed_and_do_not_overlap_other_mutations() {
    for value in [
        "publish-success",
        "publish-primary-rejected",
        "publish-primary-unknown",
        "publish-sync-rejected",
        "publish-sync-unknown",
        "publish-retirement",
        "publish-validation-recovery",
        "publish-blocked",
        "publish-stale-readback",
        "publish-republish-same-version",
        "publish-republish-new-version",
        "publish-republish-changed",
        "publish-republish-unknown",
        "publish-republish-stale",
        "publish-republish-retirement",
    ] {
        assert!(supported(value), "{value}");
    }
    for value in [
        "",
        "unpublish-success",
        "delete-success",
        "publish",
        "publish-success-extra",
    ] {
        assert!(!supported(value), "{value}");
    }
}
#[test]
fn publication_fixture_preserves_original_details_and_requires_new_version() {
    let mut s = crate::app_home::about::management::tests::shell();
    let m = s.app_home.about.management.fixture.as_mut().unwrap();
    raw::seed(&mut m.workflow, &mut m.trigger);
    let original = m.workflow.clone();
    let published = raw::published(&original);
    assert_eq!(original["id"], "WfREVIEW01");
    assert_eq!(original["is_published"], true);
    assert_eq!(published["is_published"], true);
    assert_eq!(original["last_published_version_id"], "V1");
    assert_eq!(published["last_published_version_id"], "V2");
    assert_eq!(original["unpublished_change_count"], 2);
    assert_eq!(published["unpublished_change_count"], 0);
    assert_eq!(published["title"], original["title"]);
    assert_eq!(published["description"], original["description"]);
    assert_eq!(published["retained_unknown"], 42);
    let request = protocol::TriggerUpdate::synchronize(&published, &m.trigger)
        .unwrap()
        .unwrap();
    assert_eq!(
        request.arguments()["inputs"],
        raw::synchronized(&m.trigger)["inputs"]
    );
    assert_eq!(request.reason(), "publish-workflow");
}
#[test]
fn exact_publish_wire_rejects_mutated_identity_reason_or_body() {
    let s = crate::app_home::about::management::tests::shell();
    let h = s.app_home.about.management.surface.as_ref().unwrap();
    let request = || {
        h.owner.ctx.client.rest_form(
            &h.owner.ctx.session,
            "functions.workflows.publish",
            vec![
                ("workflow_id", "WfREVIEW01".into()),
                ("_x_reason", "publish-workflow".into()),
            ],
        )
    };
    let expected = request();
    assert!(wire::same_request(&request(), &expected).is_ok());
    for kind in 0..5 {
        let mut bad = request();
        if kind == 0 {
            bad.method = "GET";
        } else if kind == 1 {
            bad.url.push_str("#other");
        } else {
            let RequestBody::Form(fields) = &mut bad.body else {
                panic!("form");
            };
            if kind == 2 {
                fields.push(("workflow_id".into(), "WfOTHER01".into()));
            }
            if kind == 3 {
                fields.iter_mut().find(|(k, _)| k == "_x_reason").unwrap().1 =
                    "unpublish-workflow".into();
            }
            if kind == 4 {
                fields.iter_mut().find(|(k, _)| k == "token").unwrap().1 =
                    "another-synthetic-token".into();
            }
        }
        assert!(
            wire::same_request(&bad, &expected).is_err(),
            "mutation {kind}"
        );
    }
}
