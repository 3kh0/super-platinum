use super::*;

fn shell() -> (ShellState, u64) {
    let (mut s, visit) = super::super::tests::shell();
    let policy = policy::fixture_export_snapshot(&mut s, Some(json!("on")));
    s.app_home.about.management.surface.as_mut().unwrap().policy = Some(policy);
    let d = document_mut(&mut s, visit).unwrap();
    d.title = "Unsent title".into();
    d.description = "Unsent\nmultiline description".into();
    assert!(Ticket::capture(&s, visit).is_some());
    (s, visit)
}
fn holder(ticket: &Ticket) -> ActivityExport {
    let q = query(
        &ticket.workflow,
        &ticket.filters,
        Dates {
            from: None,
            to: None,
        },
    )
    .unwrap();
    ActivityExport::new(&q, Some(&json!("on"))).unwrap()
}
fn sent(s: &mut ShellState, visit: u64) -> (Ticket, read::Lease, PreparedRequest) {
    let ticket = Ticket::capture(s, visit).unwrap();
    let lease = reserve(s, &ticket).unwrap();
    let request = claim(s, lease.id, holder(&ticket)).unwrap();
    (ticket, lease, request)
}

#[test]
fn reservation_preserves_draft_and_claim_and_receipt_are_once_only() {
    let (mut s, visit) = shell();
    let before = document(&s, visit).unwrap().clone();
    let ticket = Ticket::capture(&s, visit).unwrap();
    let lease = reserve(&mut s, &ticket).unwrap();
    assert!(ticket.current(&s));
    assert!(reserve(&mut s, &ticket).is_none());
    assert!(Ticket::capture(&s, visit).is_none());
    let request = claim(&mut s, lease.id, holder(&ticket)).unwrap();
    assert!(fixture_plan(&s, lease.id, &request));
    assert!(claim(&mut s, lease.id, holder(&ticket)).is_none());
    stop(&mut s, lease.id);
    assert!(s.app_home.about.management.exports[0].phase == Phase::Sent);
    let raw = json!({"ok":true,"opaque":{"keep":[2,1]}});
    settle(&mut s, lease.id, Ok(raw.clone()));
    settle(
        &mut s,
        lease.id,
        Err(Error::Transport("later failure".into())),
    );
    let attempt = &s.app_home.about.management.exports[0];
    assert!(attempt.phase == Phase::Initiated);
    assert_eq!(attempt.receipt.as_ref().unwrap().as_ref().unwrap(), &raw);
    assert_eq!(attempt.initiated.as_ref().unwrap().raw(), &raw);
    assert!(
        attempt
            .initiated
            .as_ref()
            .unwrap()
            .raw()
            .get("file")
            .is_none()
    );
    assert!(!fixture_plan(&s, lease.id, &request));
    lease.finish(&mut s, None);
    let after = document(&s, visit).unwrap();
    assert_eq!(after.title, before.title);
    assert_eq!(after.description, before.description);
    assert_eq!(after.revision, before.revision);
    assert_eq!(after.pending, before.pending);
    assert!(after.editor.same_revision(&before.editor));
    assert!(Ticket::capture(&s, visit).is_some());
}

#[test]
fn source_fences_refuse_changed_owner_document_buffers_and_member_state_before_reservation() {
    let changes: Vec<Box<dyn Fn(&mut ShellState, u64)>> = vec![
        Box::new(|s, v| document_mut(s, v).unwrap().title.push('!')),
        Box::new(|s, v| document_mut(s, v).unwrap().description.push('!')),
        Box::new(|s, v| document_mut(s, v).unwrap().revision = next()),
        Box::new(|s, v| document_mut(s, v).unwrap().pending = Some(next())),
        Box::new(|s, v| document_mut(s, v).unwrap().activity.active = false),
        Box::new(|s, v| document_mut(s, v).unwrap().activity.filters.status = "ERROR".into()),
        Box::new(|s, v| document_mut(s, v).unwrap().activity.filters.revision = next()),
        Box::new(|s, v| document_mut(s, v).unwrap().activity.filters.user = "UOTHER".into()),
        Box::new(|s, v| document_mut(s, v).unwrap().activity.user_search.push('a')),
        Box::new(|s, v| document_mut(s, v).unwrap().activity.member_generation = next()),
        Box::new(|s, v| document_mut(s, v).unwrap().activity.member_composing = true),
        Box::new(|s, v| document_mut(s, v).unwrap().id = next()),
        Box::new(|s, _| s.app_home.about.management.surface.as_mut().unwrap().id = next()),
        Box::new(|s, _| s.core.active_account = Some("replacement-account".into())),
        Box::new(|s, _| s.core.account_epoch += 1),
        Box::new(|s, _| s.core.session.as_mut().unwrap().d_cookie.push('x')),
        Box::new(|s, _| {
            let team = s
                .app_home
                .about
                .management
                .surface
                .as_ref()
                .unwrap()
                .owner
                .ctx
                .session
                .team_id
                .clone();
            s.core
                .session
                .as_mut()
                .unwrap()
                .workspaces
                .get_mut(&team)
                .unwrap()
                .token
                .push('x');
        }),
        Box::new(|s, _| {
            s.core.transport = Some(std::sync::Arc::new(
                super_platinum_core::slack::Transport::disconnected_fixture(
                    s.core.session.as_ref().unwrap().d_cookie.clone(),
                ),
            ));
        }),
        Box::new(|s, _| {
            let team = s
                .app_home
                .about
                .management
                .surface
                .as_ref()
                .unwrap()
                .owner
                .ctx
                .session
                .team_id
                .clone();
            s.core.workspaces.get_mut(&team).unwrap().rt_generation += 1;
        }),
    ];
    for (i, change) in changes.into_iter().enumerate() {
        let (mut s, visit) = shell();
        let ticket = Ticket::capture(&s, visit).unwrap();
        change(&mut s, visit);
        assert!(!ticket.source_current(&s), "source change {i}");
        assert!(reserve(&mut s, &ticket).is_none(), "reservation {i}");
        assert!(s.app_home.about.management.exports.is_empty());
        assert!(
            s.app_home
                .about
                .management
                .surface
                .as_ref()
                .unwrap()
                .read
                .is_none()
        );
    }
}

#[test]
fn exact_policy_and_original_read_lease_are_required_at_claim() {
    for kind in 0..4 {
        let (mut s, visit) = shell();
        let ticket = Ticket::capture(&s, visit).unwrap();
        let lease = reserve(&mut s, &ticket).unwrap();
        match kind {
            0 => document_mut(&mut s, visit).unwrap().description.push('!'),
            1 => {
                let policy = policy::fixture_export_snapshot(&mut s, Some(json!("off")));
                s.app_home.about.management.surface.as_mut().unwrap().policy = Some(policy);
                assert!(ticket.source_current(&s));
                assert!(!ticket.current(&s));
            }
            2 => s.app_home.about.management.surface.as_mut().unwrap().read = Some(next()),
            3 => s.core.account_epoch += 1,
            _ => unreachable!(),
        }
        assert!(
            claim(&mut s, lease.id, holder(&ticket)).is_none(),
            "claim {kind}"
        );
        let attempt = &s.app_home.about.management.exports[0];
        assert!(attempt.phase == Phase::Preparing);
        assert!(attempt.request.is_none());
        assert!(attempt.receipt.is_none());
        stop(&mut s, lease.id);
        assert!(s.app_home.about.management.exports[0].phase == Phase::NotSent);
        if kind == 2 {
            let successor = s.app_home.about.management.surface.as_ref().unwrap().read;
            lease.finish(&mut s, None);
            assert_eq!(
                s.app_home.about.management.surface.as_ref().unwrap().read,
                successor
            );
        }
    }
}

#[test]
fn terminal_classification_retains_exact_result_without_promoting_ambiguous_ack() {
    let cases = vec![
        (Ok(json!({"ok":true})), Phase::Initiated),
        (
            Ok(json!({"ok":false,"error":"not_allowed","partial":{"keep":1}})),
            Phase::Rejected,
        ),
        (Err(Error::Offline("not dispatched".into())), Phase::NotSent),
        (
            Err(Error::Api("workflow_management_not_sent_method".into())),
            Phase::NotSent,
        ),
        (
            Err(Error::RateLimited {
                retry_after_secs: Some(7),
            }),
            Phase::Rejected,
        ),
        (Err(Error::Transport("reply lost".into())), Phase::Unknown),
        (Err(Error::Api("unexpected".into())), Phase::Unknown),
        (
            Ok(json!({"ok":true,"error":"contradictory"})),
            Phase::Unknown,
        ),
        (Ok(json!({"ok":"true"})), Phase::Unknown),
        (Ok(json!({"unrecognized":true})), Phase::Unknown),
        (Ok(Value::Null), Phase::Unknown),
        (Ok(json!([])), Phase::Unknown),
    ];
    for (result, phase) in cases {
        let (mut s, visit) = shell();
        let (_, lease, _) = sent(&mut s, visit);
        settle(&mut s, lease.id, result.clone());
        let a = &s.app_home.about.management.exports[0];
        assert!(
            a.phase == phase,
            "expected {} got {}",
            phase.name(),
            a.phase.name()
        );
        match (&result, a.receipt.as_ref().unwrap()) {
            (Ok(expected), Ok(actual)) => assert_eq!(actual, expected),
            (Err(expected), Err(actual)) => {
                assert_eq!(format!("{actual:?}"), format!("{expected:?}"))
            }
            _ => panic!("changed result kind"),
        }
        assert_eq!(a.initiated.is_some(), phase == Phase::Initiated);
        lease.finish(&mut s, None);
        assert_eq!(
            blocked(&s, document(&s, visit).unwrap()),
            phase == Phase::Unknown
        );
    }
}

#[test]
fn unknown_blocks_same_export_across_search_status_page_but_not_other_filter_account_or_document() {
    let (mut s, visit) = shell();
    let (_, lease, _) = sent(&mut s, visit);
    settle(
        &mut s,
        lease.id,
        Err(Error::Transport("unknown delivery".into())),
    );
    lease.finish(&mut s, None);
    let original = document(&s, visit).unwrap().clone();
    assert!(blocked(&s, &original));
    // These table changes do not change the export wire filter set.
    let q = ActivityQuery::new("WF_REVIEW", ActivityFilters::default())
        .unwrap()
        .with_limit(20)
        .unwrap()
        .with_parent(Some("PARENT"))
        .unwrap()
        .with_cursor(Some("cursor-2"))
        .unwrap();
    let page = ActivityPage::decode(
        &q,
        json!({"ok":true,"workflow_events":[],"response_metadata":{"next_cursor":"cursor-3"}}),
    )
    .unwrap();
    let changed = document_mut(&mut s, visit).unwrap();
    changed.activity.filters.status = "ERROR".into();
    changed.activity.filters.limit = "20".into();
    changed.activity.filters.revision = next();
    changed.activity.user_search = "different search".into();
    changed.activity.member_generation = next();
    changed.activity.page = Some(page);
    changed.activity.previous.push(q);
    assert!(blocked(&s, document(&s, visit).unwrap()));
    assert!(Ticket::capture(&s, visit).is_none());
    for kind in 0..6 {
        let mut other = original.clone();
        match kind {
            0 => other.activity.filters.user = "UOTHER".into(),
            1 => other.activity.filters.from = "2026-10-01".into(),
            2 => other.activity.filters.to = "2026-10-18".into(),
            3 => other.account = Some("other-account".into()),
            4 => other.id = next(),
            5 => other.user = "UOTHER".into(),
            _ => unreachable!(),
        }
        assert!(!blocked(&s, &other), "distinct export scope {kind}");
    }
    // Changing filters away and back cannot erase the original uncertainty.
    document_mut(&mut s, visit).unwrap().activity.filters.user = "UOTHER".into();
    assert!(Ticket::capture(&s, visit).is_some());
    document_mut(&mut s, visit)
        .unwrap()
        .activity
        .filters
        .user
        .clear();
    assert!(Ticket::capture(&s, visit).is_none());
}

#[test]
fn preparing_and_sent_block_even_changed_filters_and_unsent_receipt_cannot_settle() {
    for dispatch in [false, true] {
        let (mut s, visit) = shell();
        let ticket = Ticket::capture(&s, visit).unwrap();
        let lease = reserve(&mut s, &ticket).unwrap();
        if dispatch {
            claim(&mut s, lease.id, holder(&ticket)).unwrap();
        }
        let mut other_filters = document(&s, visit).unwrap().clone();
        other_filters.activity.filters.user = "UOTHER".into();
        assert!(blocked(&s, &other_filters));
        if !dispatch {
            settle(&mut s, lease.id, Ok(json!({"ok":true})));
            assert!(s.app_home.about.management.exports[0].receipt.is_none());
            stop(&mut s, lease.id);
            stop(&mut s, lease.id);
            assert!(s.app_home.about.management.exports[0].phase == Phase::NotSent);
            assert!(!blocked(&s, &other_filters));
        }
    }
}

#[test]
fn shell_carry_retains_original_receipt_and_late_reply_without_authorizing_new_owner() {
    for late in [false, true] {
        let (mut previous, visit) = shell();
        let (ticket, lease, _) = sent(&mut previous, visit);
        let original = document(&previous, visit).unwrap().clone();
        if !late {
            settle(
                &mut previous,
                lease.id,
                Err(Error::Transport("unknown delivery".into())),
            );
        }
        let (mut loaded, _) = shell();
        loaded.core.active_account = Some("replacement-account".into());
        super::super::super::carry_pending(&mut previous, &mut loaded);
        assert!(previous.app_home.about.management.exports.is_empty());
        assert!(previous.app_home.about.management.documents.is_empty());
        assert!(!ticket.current(&loaded));
        assert!(reserve(&mut loaded, &ticket).is_none());
        if late {
            settle(
                &mut loaded,
                lease.id,
                Err(Error::Transport("late unknown delivery".into())),
            );
        }
        let attempt = loaded
            .app_home
            .about
            .management
            .exports
            .iter()
            .find(|a| a.id == lease.id)
            .unwrap();
        assert!(attempt.phase == Phase::Unknown);
        assert!(attempt.receipt.is_some());
        assert!(blocked(&loaded, &original));
        let retained = loaded
            .app_home
            .about
            .management
            .documents
            .iter()
            .find(|d| d.id == original.id)
            .unwrap();
        assert_eq!(retained.title, original.title);
        assert_eq!(retained.description, original.description);
        assert_eq!(retained.pending, original.pending);
        let current_read = loaded
            .app_home
            .about
            .management
            .surface
            .as_ref()
            .unwrap()
            .read;
        lease.finish(&mut loaded, None);
        assert_eq!(
            loaded
                .app_home
                .about
                .management
                .surface
                .as_ref()
                .unwrap()
                .read,
            current_read
        );
    }
}
