use super::*;
use super_platinum_core::{config::Session, state::Workspace};
fn shell() -> ShellState {
    let mut s = ShellState::login(crate::media::MediaRegistry::default());
    let make = |id: &str| WorkspaceSession {
        team_id: id.into(),
        enterprise_id: None,
        user_id: "U00000009".into(),
        name: id.into(),
        url: "https://example.invalid".into(),
        token: format!("synthetic-{id}"),
    };
    let auth = Session {
        d_cookie: "synthetic-cookie".into(),
        unified_user_client_enabled: BTreeMap::new(),
        workspaces: BTreeMap::from([("T1".into(), make("T1")), ("T2".into(), make("T2"))]),
    };
    s.core.active_account = Some("A".into());
    s.core.active_team = Some("T1".into());
    s.core.transport = Some(Arc::new(Transport::new("synthetic-cookie").unwrap()));
    s.core.workspaces.clear();
    for (team, owner) in &auth.workspaces {
        let mut w = Workspace::from_session(owner);
        w.directory_members
            .begin("A", owner, &BTreeSet::new())
            .unwrap();
        s.core.workspaces.insert(team.clone(), w);
    }
    s.core.session = Some(auth);
    s
}
fn ids(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| (*s).into()).collect()
}
#[test]
fn independent_teams_share_one_registry_but_do_not_share_a_dispatch() {
    let mut s = shell();
    let a = register(&mut s, "T1", &ids(&["U00000001"]))
        .unwrap()
        .unwrap();
    let b = register(&mut s, "T2", &ids(&["U00000001"]))
        .unwrap()
        .unwrap();
    let one = arm(&mut s, &a, Instant::now()).unwrap().unwrap();
    let two = arm(&mut s, &b, Instant::now()).unwrap().unwrap();
    let first = dispatch(&mut s, &one, one.due()).unwrap();
    let second = dispatch(&mut s, &two, two.due()).unwrap();
    assert!(first.current(&s));
    assert!(second.current(&s));
    s.core.active_team = Some("T2".into());
    assert!(first.current(&s));
    assert_eq!(first.context().session.team_id, "T1");
    assert_eq!(second.context().session.team_id, "T2");
}
#[test]
fn all_account_credentials_config_transport_epoch_and_rt_witnesses_matter() {
    for variant in 0..7 {
        let mut s = shell();
        let c = Context::capture(&s, "T1").unwrap();
        match variant {
            0 => s.core.active_account = Some("B".into()),
            1 => s.core.session.as_mut().unwrap().d_cookie = "replacement".into(),
            2 => {
                s.core
                    .session
                    .as_mut()
                    .unwrap()
                    .workspaces
                    .get_mut("T2")
                    .unwrap()
                    .token = "replacement-secondary".into()
            }
            3 => {
                s.core
                    .session
                    .as_mut()
                    .unwrap()
                    .unified_user_client_enabled
                    .insert("T1".into(), true);
            }
            4 => s.core.transport = Some(Arc::new(Transport::new("synthetic-cookie").unwrap())),
            5 => s.core.account_epoch += 1,
            _ => s.core.workspaces.get_mut("T1").unwrap().rt_generation += 1,
        }
        assert!(!c.current(&s), "variant {variant}");
    }
}
#[test]
fn retired_query_does_not_discard_a_siblings_network_receipt() {
    let mut s = shell();
    let first = register(&mut s, "T1", &ids(&["U00000001"]))
        .unwrap()
        .unwrap();
    let wake = arm(&mut s, &first, Instant::now()).unwrap().unwrap();
    let sibling = register(&mut s, "T1", &ids(&["U00000001", "U00000002"]))
        .unwrap()
        .unwrap();
    assert!(arm(&mut s, &sibling, Instant::now()).unwrap().is_none());
    let d = dispatch(&mut s, &wake, wake.due()).unwrap();
    assert_eq!(d.capture().ids(), ids(&["U00000001", "U00000002"]));
    assert_eq!(d.interaction_before().get("U00000001"), Some(&None));
    assert!(!d.interaction_before().contains_key("U00000009"));
    retire(&mut s, &first);
    assert!(d.current(&s));
    publish_observed_terminal(&mut s, d, NetworkTerminal::Complete(vec![]), vec![], vec![])
        .unwrap();
    let (lease, receipt) = claim_adoption(&mut s, &sibling).unwrap().unwrap();
    assert!(lease.current(&s));
    assert!(receipt.current(&s));
    assert!(settle(&mut s, &lease).is_err()); // Raw completion is not installed source/native.
    release_adoption(&mut s, &lease).unwrap();
    assert!(!lease.current(&s));
    assert!(claim_adoption(&mut s, &sibling).unwrap().is_some());
}
#[test]
fn raw_source_or_native_drift_refuses_publication_without_missing_projection() {
    for native_change in [false, true] {
        let mut s = shell();
        let a = register(&mut s, "T1", &ids(&["U00000001"]))
            .unwrap()
            .unwrap();
        let wake = arm(&mut s, &a, Instant::now()).unwrap().unwrap();
        let d = dispatch(&mut s, &wake, wake.due()).unwrap();
        if native_change {
            s.core.workspaces.get_mut("T1").unwrap().users.insert(
                "U00000001".into(),
                serde_json::from_value(serde_json::json!({"id":"U00000001"})).unwrap(),
            );
        } else {
            let owner = s.core.session.as_ref().unwrap().workspaces["T1"].clone();
            s.core
                .workspaces
                .get_mut("T1")
                .unwrap()
                .directory_members
                .begin("A", &owner, &BTreeSet::new())
                .unwrap();
        }
        assert!(!d.current(&s));
        assert!(
            publish_observed_terminal(&mut s, d.clone(), NetworkTerminal::Failed, vec![], vec![])
                .is_err()
        );
        refuse(&mut s, &d).unwrap();
        assert_eq!(status(&s, &a), claim::Status::Retired);
    }
}
#[tokio::test]
async fn sibling_waiting_is_event_driven_not_a_polling_timer() {
    let mut s = shell();
    let mut a = register(&mut s, "T1", &ids(&["U00000001"]))
        .unwrap()
        .unwrap();
    let wake = arm(&mut s, &a, Instant::now()).unwrap().unwrap();
    let d = dispatch(&mut s, &wake, wake.due()).unwrap();
    publish_observed_terminal(&mut s, d, NetworkTerminal::Complete(vec![]), vec![], vec![])
        .unwrap();
    // watch retains a changed version even if notification preceded the await.
    tokio::time::timeout(std::time::Duration::from_millis(100), a.changed())
        .await
        .unwrap()
        .unwrap();
    assert!(claim_adoption(&mut s, &a).unwrap().is_some());
}
#[test]
fn shared_runtime_preserves_shell_send_contract() {
    fn send<T: Send>() {}
    send::<Registry>();
    send::<Dispatch>();
    send::<Outcome>();
    send::<ShellState>();
}
#[test]
fn shell_rebuild_keeps_same_context_claim_and_rejects_replacement_transport() {
    for same_transport in [true, false] {
        let mut old = shell();
        let a = register(&mut old, "T1", &ids(&["U00000001"]))
            .unwrap()
            .unwrap();
        let wake = arm(&mut old, &a, Instant::now()).unwrap().unwrap();
        let d = dispatch(&mut old, &wake, wake.due()).unwrap();
        let mut loaded = shell();
        if same_transport {
            loaded.core.transport = old.core.transport.clone();
        }
        loaded
            .core
            .workspaces
            .get_mut("T1")
            .unwrap()
            .directory_members = old.core.workspaces["T1"].directory_members.clone();
        carry_into_fresh_shell(&mut old, &mut loaded);
        assert_eq!(d.current(&loaded), same_transport);
        assert!(!d.current(&old));
    }
}

#[test]
fn invalidating_absent_source_retires_old_read_without_fabricating_a_member() {
    let mut s = shell();
    let requested = ids(&["U00000001"]);
    let first = register(&mut s, "T1", &requested).unwrap().unwrap();
    let wake = arm(&mut s, &first, Instant::now()).unwrap().unwrap();
    let old = dispatch(&mut s, &wake, wake.due()).unwrap();
    s.core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .directory_members
        .invalidate(&requested[0]);
    assert!(!old.current(&s));
    assert!(
        publish_observed_terminal(&mut s, old.clone(), NetworkTerminal::Failed, vec![], vec![])
            .is_err()
    );
    refuse(&mut s, &old).unwrap();
    assert_eq!(status(&s, &first), Status::Retired);
    let retry = register(&mut s, "T1", &requested).unwrap().unwrap();
    let wake = arm(&mut s, &retry, Instant::now()).unwrap().unwrap();
    let fresh = dispatch(&mut s, &wake, wake.due()).unwrap();
    assert_eq!(fresh.capture().ids(), requested);
    assert!(fresh.current(&s));
    assert!(s.core.workspaces["T1"].users.is_empty());
}

#[test]
fn stale_canceled_receipt_allows_first_fresh_opening_to_dispatch() {
    let mut s = shell();
    let requested = ids(&["U00000001"]);
    let original = register(&mut s, "T1", &requested).unwrap().unwrap();
    let wake = arm(&mut s, &original, Instant::now()).unwrap().unwrap();
    let old = dispatch(&mut s, &wake, wake.due()).unwrap();
    publish_observed_terminal(
        &mut s,
        old.clone(),
        NetworkTerminal::Complete(vec![]),
        vec![],
        vec![],
    )
    .unwrap();
    let (old_adoption, old_receipt) = claim_adoption(&mut s, &original).unwrap().unwrap();
    retire(&mut s, &original);
    s.core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .directory_members
        .invalidate(&requested[0]);

    let fresh = register(&mut s, "T1", &requested).unwrap().unwrap();
    assert_eq!(status(&s, &fresh), Status::Pending);
    assert!(!old_receipt.current(&s));
    assert!(!old_adoption.current(&s));
    assert!(claim_adoption(&mut s, &fresh).unwrap().is_none());
    let wake = arm(&mut s, &fresh, Instant::now()).unwrap().unwrap();
    let next = dispatch(&mut s, &wake, wake.due()).unwrap();
    assert_eq!(next.capture().ids(), requested);
    assert!(next.current(&s));
    assert!(
        publish_observed_terminal(&mut s, old, NetworkTerminal::Failed, vec![], vec![]).is_err()
    );
    assert!(s.core.workspaces["T1"].users.is_empty());
}

#[test]
fn current_canceled_receipt_is_reused_without_another_read() {
    let mut s = shell();
    let requested = ids(&["U00000001"]);
    let original = register(&mut s, "T1", &requested).unwrap().unwrap();
    let wake = arm(&mut s, &original, Instant::now()).unwrap().unwrap();
    let d = dispatch(&mut s, &wake, wake.due()).unwrap();
    publish_observed_terminal(&mut s, d, NetworkTerminal::Complete(vec![]), vec![], vec![])
        .unwrap();
    let (old_adoption, receipt) = claim_adoption(&mut s, &original).unwrap().unwrap();
    retire(&mut s, &original);

    let next = register(&mut s, "T1", &requested).unwrap().unwrap();
    assert!(arm(&mut s, &next, Instant::now()).unwrap().is_none());
    let (adoption, reused) = claim_adoption(&mut s, &next).unwrap().unwrap();
    assert!(Arc::ptr_eq(&receipt, &reused));
    assert!(adoption.current(&s));
    assert!(reused.current(&s));
    assert!(release_adoption(&mut s, &old_adoption).is_err());
    assert!(adoption.current(&s));
}

#[test]
fn stale_receipt_cleanup_preserves_cooldown_and_deferred_id_order() {
    let mut s = shell();
    let requested = ids(&["U00000001", "U00000002"]);
    let original = register(&mut s, "T1", &requested).unwrap().unwrap();
    let wake = arm(&mut s, &original, Instant::now()).unwrap().unwrap();
    let started = wake.due();
    let d = dispatch(&mut s, &wake, started).unwrap();
    let failure = Arc::new(Error::RateLimited {
        retry_after_secs: Some(17),
    });
    let deadline = started + std::time::Duration::from_secs(17);
    publish_observed_completed(
        &mut s,
        d,
        vec![requested[0].clone()],
        NetworkTerminal::Complete(vec![]),
        vec![],
        vec![failure.clone()],
    )
    .unwrap();
    {
        let slot = s
            .directory_member_fetches
            .slot_mut(&original.context)
            .unwrap();
        slot.retry_at = Some(Some(deadline));
        slot.last_failure = Some(failure.clone());
    }
    retire(&mut s, &original);
    s.core
        .workspaces
        .get_mut("T1")
        .unwrap()
        .directory_members
        .invalidate(&requested[0]);

    let fresh = register(&mut s, "T1", &requested).unwrap().unwrap();
    {
        let slot = s.directory_member_fetches.slot(&fresh.context).unwrap();
        assert_eq!(slot.retry_at, Some(Some(deadline)));
        assert!(Arc::ptr_eq(slot.last_failure.as_ref().unwrap(), &failure));
        assert!(slot.manager.retained_receipt().is_none());
    }
    let wake = arm(&mut s, &fresh, started).unwrap().unwrap();
    assert_eq!(
        wake.due(),
        started + std::time::Duration::from_millis(claim::DEBOUNCE_MS)
    );
    // The production worker waits max(wake.due(), retained cooldown). Inspect
    // its eventual dispatch without waiting or clearing that cooldown.
    let next = dispatch(&mut s, &wake, deadline).unwrap();
    assert_eq!(next.capture().ids(), ids(&["U00000002", "U00000001"]));
    assert!(next.current(&s));
}
