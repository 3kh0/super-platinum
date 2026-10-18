use super::*;

pub(super) fn shell() -> (ShellState, u64) {
    let mut s = super::super::tests::shell();
    let editor = super::super::tests::editor(&mut s);
    let h = s.app_home.about.management.surface.as_ref().unwrap();
    let visit = h.id;
    let doc = Document {
        activity: State {
            active: true,
            ..State::default()
        },
        id: next(),
        account: h.account.clone(),
        team: h.owner.ctx.session.team_id.clone(),
        user: h.owner.ctx.session.user_id.clone(),
        title: editor.raw_workflow()["title"].as_str().unwrap().into(),
        description: editor.raw_workflow()["description"]
            .as_str()
            .unwrap_or("")
            .into(),
        editor,
        revision: next(),
        observed: None,
        pending: None,
        notice: None,
    };
    s.app_home
        .about
        .management
        .surface
        .as_mut()
        .unwrap()
        .document = Some(doc.id);
    s.app_home.about.management.documents.push(doc);
    assert!(permitted(&s, visit));
    (s, visit)
}

fn page(query: &ActivityQuery, cursor: &str) -> ActivityPage {
    ActivityPage::decode(
        query,
        json!({"ok":true,"workflow_events":[{
        "workflow_execution_id":"EXECUTION","workflow_id":"WF_REVIEW"}],
        "response_metadata":{"next_cursor":cursor}}),
    )
    .unwrap()
}

#[test]
fn default_query_omits_status_user_and_dates_instead_of_manufacturing_filters() {
    let q = query(
        "WF_REVIEW",
        &Filters::default(),
        Dates {
            from: None,
            to: None,
        },
    )
    .unwrap();
    assert_eq!(q.limit(), 100);
    assert_eq!(q.filters(), &ActivityFilters::default());
    assert_eq!(q.request().arguments()["filters"], json!({}));
    assert!(q.request().arguments().get("cursor").is_none());
    assert!(
        q.request()
            .arguments()
            .get("parent_workflow_execution_id")
            .is_none()
    );
}

#[test]
fn query_requires_native_date_presence_and_order_and_source_status_limit_choices() {
    let mut f = Filters::default();
    f.from = "2026-03-08".into();
    assert!(
        query(
            "WF_REVIEW",
            &f,
            Dates {
                from: None,
                to: None
            }
        )
        .is_err()
    );
    let q = query(
        "WF_REVIEW",
        &f,
        Dates {
            from: Some(1772946000),
            to: None,
        },
    )
    .unwrap();
    assert_eq!(q.filters().from_timestamp, Some(1772946000));
    f.to = "2026-03-09".into();
    assert!(
        query(
            "WF_REVIEW",
            &f,
            Dates {
                from: Some(20),
                to: Some(19)
            }
        )
        .is_err()
    );
    f.from.clear();
    f.to.clear();
    assert!(
        query(
            "WF_REVIEW",
            &f,
            Dates {
                from: Some(0),
                to: None
            }
        )
        .is_err()
    );
    for status in [
        "",
        "SUCCESS",
        "PENDING",
        "ERROR",
        "CANCELED",
        "PENDING_USER_ACTION",
    ] {
        f.status = status.into();
        assert!(
            query(
                "WF_REVIEW",
                &f,
                Dates {
                    from: None,
                    to: None
                }
            )
            .is_ok()
        );
    }
    f.status = "EXPIRED".into();
    assert!(
        query(
            "WF_REVIEW",
            &f,
            Dates {
                from: None,
                to: None
            }
        )
        .is_err()
    );
    f.status.clear();
    for limit in ["20", "50", "75", "100"] {
        f.limit = limit.into();
        assert!(
            query(
                "WF_REVIEW",
                &f,
                Dates {
                    from: None,
                    to: None
                }
            )
            .is_ok()
        );
    }
    for limit in ["", "0", "101", "many"] {
        f.limit = limit.into();
        assert!(
            query(
                "WF_REVIEW",
                &f,
                Dates {
                    from: None,
                    to: None
                }
            )
            .is_err()
        );
    }
}

#[test]
fn next_previous_history_is_bounded_and_keeps_exact_parent_filters_and_limit() {
    let q = ActivityQuery::new(
        "WF_REVIEW",
        ActivityFilters {
            status: Some("ERROR".into()),
            from_timestamp: Some(10),
            to_timestamp: Some(20),
            ..Default::default()
        },
    )
    .unwrap()
    .with_limit(20)
    .unwrap()
    .with_parent(Some("PARENT"))
    .unwrap();
    let mut a = State::default();
    adopt_page(&mut a, page(&q, "cursor-1"), Page::First);
    for index in 1..=40 {
        let next = a.page.as_ref().unwrap().next_query().unwrap();
        adopt_page(
            &mut a,
            page(&next, &format!("cursor-{}", index + 1)),
            Page::Next,
        );
    }
    assert_eq!(a.previous.len(), 32);
    for prior in &a.previous {
        assert_eq!(prior.filters(), q.filters());
        assert_eq!(prior.parent_execution_id(), Some("PARENT"));
        assert_eq!(prior.limit(), 20);
    }
    let prior = a.previous.last().unwrap().clone();
    adopt_page(&mut a, page(&prior, "return-cursor"), Page::Previous);
    assert_eq!(a.page.as_ref().unwrap().query(), &prior);
    assert_eq!(a.previous.len(), 31);
    adopt_page(&mut a, page(&q, ""), Page::First);
    assert!(a.previous.is_empty());
    assert!(a.page.as_ref().unwrap().next_query().is_none());
}

#[test]
fn filter_and_back_navigation_preserve_dirty_unresolved_metadata_and_observation() {
    let (mut s, visit) = shell();
    let d = document_mut(&mut s, visit).unwrap();
    d.title = "Exact proposed title".into();
    d.description = "  Retain proposed text\nsecond line\n".into();
    d.pending = Some(73);
    d.observed = Some(d.editor.clone());
    d.notice = Some("The write response is unknown.".into());
    let before = d.clone();
    assert!(before.dirty());
    edit(&mut s, visit, Field::Status, "ERROR".into());
    back(&mut s, visit);
    let after = document(&s, visit).unwrap();
    assert!(!after.activity.active);
    assert_eq!(after.title, before.title);
    assert_eq!(after.description, before.description);
    assert_eq!(after.pending, before.pending);
    assert_eq!(after.notice, before.notice);
    assert_eq!(after.revision, before.revision);
    assert!(after.editor.same_revision(&before.editor));
    assert!(
        after
            .observed
            .as_ref()
            .unwrap()
            .same_revision(before.observed.as_ref().unwrap())
    );
    assert_eq!(super::super::fixture::snapshot(&s)["writes"], 0);
}

#[test]
fn close_detail_requires_current_generation_and_filter_changes_retire_detail() {
    let (mut s, visit) = shell();
    let d = document_mut(&mut s, visit).unwrap();
    d.activity.detail_row = Some(json!({"workflow_execution_id":"EXECUTION"}));
    d.activity.detail_revision = next();
    let revision = d.activity.detail_revision;
    close_detail(&mut s, visit, revision.wrapping_add(1));
    assert!(document(&s, visit).unwrap().activity.detail_row.is_some());
    edit(&mut s, visit, Field::From, "2026-03-08".into());
    let a = &document(&s, visit).unwrap().activity;
    assert!(a.detail_row.is_none());
    assert_ne!(a.detail_revision, revision);
    let current = a.detail_revision;
    close_detail(&mut s, visit, revision);
    assert_eq!(
        document(&s, visit).unwrap().activity.detail_revision,
        current
    );
}
